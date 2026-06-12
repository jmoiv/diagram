//! Parse the YAML diagram language into a [`Document`].
//!
//! Steps:
//! 1. Load YAML (via `saphyr`) and convert to a small owned value tree ([`Yv`]).
//! 2. Resolve the `vars:` block (in document order; later vars may use earlier ones).
//! 3. Interpolate every `${ ... }` expression in the rest of the tree (using [`crate::expr`]),
//!    so numeric fields become numbers and embedded expressions become strings.
//! 4. Build the typed [`Document`].
//!
//! ## Language sketch
//! ```yaml
//! vars: { pad: 8, box_w: 120, accent: "#36c" }
//! canvas: { width: 400, padding: 10, background: white, font_size: 14 }
//! root:
//!   vbox:                      # vbox / hbox / box hold a list of child nodes
//!     - text: "Title"
//!       font_size: ${pad * 2}
//!     - symbol: schematic.resistor
//!       id: R1
//!       props: { ohms: 220 }
//!   gap: 10
//!   align: center
//! connect:
//!   - { from: R1.a, to: R1.b, arrow: end, routing: orthogonal }
//! ```

use std::collections::HashMap;

use saphyr::{LoadableYamlNode, Scalar, Yaml};

use crate::draw::PathCmd;
use crate::error::{Error, Result};
use crate::expr::{self, EvalContext, Value as EVal};
use crate::geom::{Point, Size};
use crate::measure::TextMeasurer;
use crate::model::*;
use crate::style::{Color, StylePatch, TextAnchor};
use crate::symbols::{PropValue, Props};

/// Parse a diagram document from YAML source.
pub fn parse_document(source: &str, measurer: &dyn TextMeasurer) -> Result<Document> {
    let docs = Yaml::load_from_str(source).map_err(|e| Error::Yaml(e.to_string()))?;
    let root = docs
        .into_iter()
        .next()
        .ok_or_else(|| Error::Yaml("empty document".into()))?;
    let mut yv = convert(&root)?;

    // Resolve vars (consumes the `vars` entry).
    let vars = resolve_vars(&mut yv, measurer)?;
    let ctx = EvalContext {
        vars: &vars,
        measurer,
        default_font: "sans-serif".to_string(),
        default_size: 14.0,
    };
    interpolate(&mut yv, &ctx)?;

    build_document(&yv)
}

// ---------------------------------------------------------------------------
// Owned value tree
// ---------------------------------------------------------------------------

/// A small owned YAML value, easy to interpolate and walk. Mappings keep document order.
#[derive(Debug, Clone)]
enum Yv {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Seq(Vec<Yv>),
    Map(Vec<(String, Yv)>),
}

impl Yv {
    fn as_f64(&self) -> Option<f64> {
        match self {
            Yv::Int(i) => Some(*i as f64),
            Yv::Float(f) => Some(*f),
            Yv::Str(s) => s.trim().parse().ok(),
            _ => None,
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Yv::Str(s) => Some(s),
            _ => None,
        }
    }

    fn as_seq(&self) -> Option<&[Yv]> {
        match self {
            Yv::Seq(v) => Some(v),
            _ => None,
        }
    }

    fn as_map(&self) -> Option<&[(String, Yv)]> {
        match self {
            Yv::Map(m) => Some(m),
            _ => None,
        }
    }

    /// Look up a key in a mapping.
    fn get(&self, key: &str) -> Option<&Yv> {
        self.as_map()?.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
}

fn convert(y: &Yaml) -> Result<Yv> {
    match y {
        Yaml::Value(scalar) => Ok(scalar_to_yv(scalar)),
        Yaml::Representation(cow, _, _) => Ok(resolve_repr(cow)),
        Yaml::Sequence(seq) => {
            let mut out = Vec::with_capacity(seq.len());
            for item in seq {
                out.push(convert(item)?);
            }
            Ok(Yv::Seq(out))
        }
        Yaml::Mapping(map) => {
            let mut out = Vec::with_capacity(map.len());
            for (k, v) in map.iter() {
                let key = yaml_key_string(k)?;
                out.push((key, convert(v)?));
            }
            Ok(Yv::Map(out))
        }
        Yaml::Tagged(_, inner) => convert(inner),
        Yaml::Alias(_) => Err(Error::Yaml("YAML aliases are not supported".into())),
        Yaml::BadValue => Err(Error::Yaml("invalid YAML value".into())),
    }
}

fn scalar_to_yv(s: &Scalar) -> Yv {
    match s {
        Scalar::Null => Yv::Null,
        Scalar::Boolean(b) => Yv::Bool(*b),
        Scalar::Integer(i) => Yv::Int(*i),
        Scalar::FloatingPoint(f) => Yv::Float(f.into_inner()),
        Scalar::String(c) => Yv::Str(c.to_string()),
    }
}

/// Resolve a lazily-represented scalar string into a typed value.
fn resolve_repr(s: &str) -> Yv {
    match s {
        "null" | "~" | "" => Yv::Null,
        "true" => Yv::Bool(true),
        "false" => Yv::Bool(false),
        _ => {
            if let Ok(i) = s.parse::<i64>() {
                Yv::Int(i)
            } else if let Ok(f) = s.parse::<f64>() {
                Yv::Float(f)
            } else {
                Yv::Str(s.to_string())
            }
        }
    }
}

fn yaml_key_string(y: &Yaml) -> Result<String> {
    match convert(y)? {
        Yv::Str(s) => Ok(s),
        Yv::Int(i) => Ok(i.to_string()),
        Yv::Bool(b) => Ok(b.to_string()),
        other => Err(Error::Yaml(format!("unsupported mapping key: {other:?}"))),
    }
}

// ---------------------------------------------------------------------------
// Variables + interpolation
// ---------------------------------------------------------------------------

fn resolve_vars(root: &mut Yv, measurer: &dyn TextMeasurer) -> Result<HashMap<String, EVal>> {
    let mut vars: HashMap<String, EVal> = HashMap::new();
    let Yv::Map(entries) = root else {
        return Err(Error::Parse("top-level document must be a mapping".into()));
    };

    // Take the `vars` entry out so it isn't reinterpreted as a node later.
    let vars_yv = entries
        .iter()
        .position(|(k, _)| k == "vars")
        .map(|i| entries.remove(i).1);

    if let Some(vy) = vars_yv {
        let pairs = vy
            .as_map()
            .ok_or_else(|| Error::Parse("`vars` must be a mapping".into()))?
            .to_vec();
        for (name, value) in pairs {
            let ctx = EvalContext {
                vars: &vars,
                measurer,
                default_font: "sans-serif".to_string(),
                default_size: 14.0,
            };
            let v = eval_var_value(&value, &ctx)?;
            vars.insert(name, v);
        }
    }
    Ok(vars)
}

fn eval_var_value(value: &Yv, ctx: &EvalContext) -> Result<EVal> {
    match value {
        Yv::Int(i) => Ok(EVal::Num(*i as f64)),
        Yv::Float(f) => Ok(EVal::Num(*f)),
        Yv::Bool(b) => Ok(EVal::Str(b.to_string())),
        Yv::Str(s) => match interp_string(s, ctx)? {
            Yv::Int(i) => Ok(EVal::Num(i as f64)),
            Yv::Float(f) => Ok(EVal::Num(f)),
            Yv::Str(s) => Ok(EVal::Str(s)),
            _ => Ok(EVal::Str(String::new())),
        },
        other => Err(Error::Parse(format!("unsupported variable value: {other:?}"))),
    }
}

/// Walk the tree, interpolating `${ ... }` inside every string scalar.
fn interpolate(yv: &mut Yv, ctx: &EvalContext) -> Result<()> {
    match yv {
        Yv::Str(s) => {
            *yv = interp_string(s, ctx)?;
        }
        Yv::Seq(items) => {
            for it in items {
                interpolate(it, ctx)?;
            }
        }
        Yv::Map(entries) => {
            for (_, v) in entries.iter_mut() {
                interpolate(v, ctx)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Interpolate `${...}` in a string. A string that is exactly one `${expr}` keeps the
/// expression's type (number stays a number); otherwise substitutions are stringified.
fn interp_string(s: &str, ctx: &EvalContext) -> Result<Yv> {
    if !s.contains("${") {
        return Ok(Yv::Str(s.to_string()));
    }

    let trimmed = s.trim();
    if let Some(inner) = whole_expression(trimmed) {
        return Ok(match expr::eval(inner, ctx)? {
            EVal::Num(n) => {
                if n == n.trunc() && n.abs() < 1e15 {
                    Yv::Int(n as i64)
                } else {
                    Yv::Float(n)
                }
            }
            EVal::Str(text) => Yv::Str(text),
        });
    }

    // Embedded: replace each ${...} with its stringified value.
    let mut out = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            let close = find_close(s, i + 2)
                .ok_or_else(|| Error::Expr(format!("unterminated ${{ in {s:?}")))?;
            let inner = &s[i + 2..close];
            let v = expr::eval(inner, ctx)?;
            out.push_str(&match v {
                EVal::Num(n) => expr::format_num(n),
                EVal::Str(t) => t,
            });
            i = close + 1;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    Ok(Yv::Str(out))
}

/// If `s` is exactly `${ ... }`, return the inner expression.
fn whole_expression(s: &str) -> Option<&str> {
    let rest = s.strip_prefix("${")?;
    let inner = rest.strip_suffix('}')?;
    // Ensure there's no earlier closing brace that would split it.
    if find_close(s, 2)? == s.len() - 1 {
        Some(inner)
    } else {
        None
    }
}

fn find_close(s: &str, start: usize) -> Option<usize> {
    s[start..].find('}').map(|rel| start + rel)
}

// ---------------------------------------------------------------------------
// Build typed model
// ---------------------------------------------------------------------------

const TYPE_KEYS: &[&str] = &[
    "vbox", "hbox", "box", "symbol", "text", "line", "rect", "circle", "path", "spacer",
];

fn build_document(root: &Yv) -> Result<Document> {
    let canvas = match root.get("canvas") {
        Some(c) => parse_canvas(c)?,
        None => Canvas::default(),
    };
    let root_node_yv = root
        .get("root")
        .ok_or_else(|| Error::Parse("document is missing a `root` node".into()))?;
    let root_node = parse_node(root_node_yv)?;

    let mut connections = Vec::new();
    if let Some(conns) = root.get("connect") {
        let seq = conns
            .as_seq()
            .ok_or_else(|| Error::Parse("`connect` must be a sequence".into()))?;
        for c in seq {
            connections.push(parse_connect(c)?);
        }
    }

    Ok(Document { canvas, root: root_node, connections })
}

fn parse_canvas(yv: &Yv) -> Result<Canvas> {
    let width = yv.get("width").and_then(opt_size);
    let height = yv.get("height").and_then(opt_size);
    let padding = yv.get("padding").and_then(Yv::as_f64).unwrap_or(10.0);
    let background = yv.get("background").and_then(parse_color);
    let base_style = parse_style_patch(yv);
    Ok(Canvas { width, height, padding, background, base_style })
}

/// `auto` (or null) means content-sized; a number means fixed.
fn opt_size(yv: &Yv) -> Option<f64> {
    match yv {
        Yv::Str(s) if s.eq_ignore_ascii_case("auto") => None,
        Yv::Null => None,
        _ => yv.as_f64(),
    }
}

fn parse_node(yv: &Yv) -> Result<Node> {
    let map = yv
        .as_map()
        .ok_or_else(|| Error::Parse(format!("a node must be a mapping, found {yv:?}")))?;

    // Find the single type key present.
    let type_key = map
        .iter()
        .find(|(k, _)| TYPE_KEYS.contains(&k.as_str()))
        .map(|(k, _)| k.clone())
        .ok_or_else(|| {
            Error::Parse(format!(
                "node has no type key (one of {TYPE_KEYS:?}); got keys {:?}",
                map.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>()
            ))
        })?;
    let type_val = yv.get(&type_key).unwrap();

    let kind = match type_key.as_str() {
        "vbox" => parse_container(Direction::Column, type_val, yv)?,
        "hbox" => parse_container(Direction::Row, type_val, yv)?,
        "box" => parse_container(Direction::Stack, type_val, yv)?,
        "symbol" => parse_symbol(type_val)?,
        "text" => parse_text(type_val, yv)?,
        "line" => parse_line(type_val)?,
        "rect" => parse_rect(type_val)?,
        "circle" => parse_circle(type_val)?,
        "path" => parse_path(type_val)?,
        "spacer" => NodeKind::Spacer,
        other => return Err(Error::Parse(format!("unknown node type {other:?}"))),
    };

    let mut node = Node::new(kind);
    attach_props(&mut node, yv);
    node.id = yv.get("id").and_then(Yv::as_str).map(str::to_string);
    node.style = parse_style_patch(yv);
    if let Some(w) = yv.get("width") {
        node.width = parse_size_spec(w);
    }
    if let Some(h) = yv.get("height") {
        node.height = parse_size_spec(h);
    }
    if let Some(m) = yv.get("margin") {
        node.margin = parse_edges(m);
    }
    node.align_self = yv.get("align_self").and_then(parse_align);
    Ok(node)
}

fn parse_container(direction: Direction, children_val: &Yv, node: &Yv) -> Result<NodeKind> {
    let mut children = Vec::new();
    if let Some(seq) = children_val.as_seq() {
        for c in seq {
            children.push(parse_node(c)?);
        }
    } else if !matches!(children_val, Yv::Null) {
        return Err(Error::Parse("a container's value must be a list of child nodes".into()));
    }
    let gap = node.get("gap").and_then(Yv::as_f64).unwrap_or(0.0);
    let padding = node.get("padding").map(parse_edges).unwrap_or_default();
    let align = node.get("align").and_then(parse_align).unwrap_or(Align::Start);
    let justify = node.get("justify").and_then(parse_justify).unwrap_or(Justify::Start);
    Ok(NodeKind::Container(Container { direction, children, gap, padding, align, justify }))
}

fn parse_symbol(val: &Yv) -> Result<NodeKind> {
    let name = val
        .as_str()
        .ok_or_else(|| Error::Parse("`symbol` value must be a \"plugin.name\" string".into()))?
        .to_string();
    // Props live under a sibling `props:` key and are attached by `attach_props`.
    Ok(NodeKind::Symbol(SymbolNode { name, props: Props::new() }))
}

fn parse_props(node: &Yv) -> Props {
    let mut props = Props::new();
    if let Some(map) = node.get("props").and_then(Yv::as_map) {
        for (k, v) in map {
            if let Some(pv) = prop_value(v) {
                props.insert(k.clone(), pv);
            }
        }
    }
    props
}

fn prop_value(v: &Yv) -> Option<PropValue> {
    match v {
        Yv::Int(i) => Some(PropValue::Number(*i as f64)),
        Yv::Float(f) => Some(PropValue::Number(*f)),
        Yv::Bool(b) => Some(PropValue::Bool(*b)),
        Yv::Str(s) => Some(PropValue::Text(s.clone())),
        Yv::Seq(items) => Some(PropValue::List(items.iter().filter_map(prop_value).collect())),
        Yv::Null | Yv::Map(_) => None,
    }
}

fn parse_text(val: &Yv, node: &Yv) -> Result<NodeKind> {
    let text = match val {
        Yv::Str(s) => s.clone(),
        Yv::Int(i) => i.to_string(),
        Yv::Float(f) => expr::format_num(*f),
        Yv::Bool(b) => b.to_string(),
        _ => return Err(Error::Parse("`text` value must be a string".into())),
    };
    let wrap = node.get("wrap").and_then(Yv::as_f64);
    Ok(NodeKind::Text(TextNode { text, wrap }))
}

fn parse_line(val: &Yv) -> Result<NodeKind> {
    let a = val.get("from").and_then(parse_point).ok_or_else(|| {
        Error::Parse("`line` needs `from: [x, y]`".into())
    })?;
    let b = val
        .get("to")
        .and_then(parse_point)
        .ok_or_else(|| Error::Parse("`line` needs `to: [x, y]`".into()))?;
    Ok(NodeKind::Line(LineShape { a, b }))
}

fn parse_rect(val: &Yv) -> Result<NodeKind> {
    let w = val.get("width").and_then(Yv::as_f64).unwrap_or(0.0);
    let h = val.get("height").and_then(Yv::as_f64).unwrap_or(0.0);
    let rx = val.get("rx").and_then(Yv::as_f64).unwrap_or(0.0);
    Ok(NodeKind::Rect(RectShape { size: Size::new(w, h), rx }))
}

fn parse_circle(val: &Yv) -> Result<NodeKind> {
    let r = val.get("r").and_then(Yv::as_f64).unwrap_or(0.0);
    Ok(NodeKind::Circle(CircleShape { radius: r }))
}

fn parse_path(val: &Yv) -> Result<NodeKind> {
    let d = val
        .get("d")
        .and_then(Yv::as_str)
        .ok_or_else(|| Error::Parse("`path` needs a `d` string".into()))?;
    let cmds = parse_path_data(d)?;
    Ok(NodeKind::Path(PathShape { cmds }))
}

fn parse_point(yv: &Yv) -> Option<Point> {
    let seq = yv.as_seq()?;
    if seq.len() != 2 {
        return None;
    }
    Some(Point::new(seq[0].as_f64()?, seq[1].as_f64()?))
}

fn parse_connect(yv: &Yv) -> Result<Connect> {
    let from = yv
        .get("from")
        .and_then(Yv::as_str)
        .map(parse_port_ref)
        .ok_or_else(|| Error::Parse("`connect` entry needs `from`".into()))?;
    let to = yv
        .get("to")
        .and_then(Yv::as_str)
        .map(parse_port_ref)
        .ok_or_else(|| Error::Parse("`connect` entry needs `to`".into()))?;
    let routing = match yv.get("routing").and_then(Yv::as_str) {
        Some(s) if s.eq_ignore_ascii_case("orthogonal") => Routing::Orthogonal,
        _ => Routing::Straight,
    };
    let arrow = match yv.get("arrow").and_then(Yv::as_str) {
        Some(s) if s.eq_ignore_ascii_case("none") => Arrow::None,
        Some(s) if s.eq_ignore_ascii_case("both") => Arrow::Both,
        _ => Arrow::End,
    };
    let label = yv.get("label").and_then(Yv::as_str).map(str::to_string);
    let style = parse_style_patch(yv);
    Ok(Connect { from, to, style, routing, arrow, label })
}

fn parse_port_ref(s: &str) -> PortRef {
    match s.split_once('.') {
        Some((node, port)) => PortRef { node: node.to_string(), port: Some(port.to_string()) },
        None => PortRef { node: s.to_string(), port: None },
    }
}

// ---------------------------------------------------------------------------
// Attribute parsing
// ---------------------------------------------------------------------------

fn parse_size_spec(yv: &Yv) -> SizeSpec {
    match yv {
        Yv::Int(_) | Yv::Float(_) => SizeSpec::Fixed(yv.as_f64().unwrap()),
        Yv::Str(s) => {
            let s = s.trim();
            if s.eq_ignore_ascii_case("auto") {
                SizeSpec::Auto
            } else if let Some(rest) = s.strip_prefix("grow") {
                let weight = rest
                    .trim_start_matches([':', ' '])
                    .trim()
                    .parse::<f64>()
                    .unwrap_or(1.0);
                SizeSpec::Grow(if weight > 0.0 { weight } else { 1.0 })
            } else if let Ok(n) = s.parse::<f64>() {
                SizeSpec::Fixed(n)
            } else {
                SizeSpec::Auto
            }
        }
        _ => SizeSpec::Auto,
    }
}

fn parse_edges(yv: &Yv) -> Edges {
    match yv {
        Yv::Int(_) | Yv::Float(_) => Edges::all(yv.as_f64().unwrap()),
        Yv::Seq(items) => {
            let vals: Vec<f64> = items.iter().filter_map(Yv::as_f64).collect();
            match vals.as_slice() {
                [a] => Edges::all(*a),
                [v, h] => Edges { top: *v, bottom: *v, left: *h, right: *h },
                [t, r, b, l] => Edges { top: *t, right: *r, bottom: *b, left: *l },
                _ => Edges::default(),
            }
        }
        Yv::Map(_) => Edges {
            top: yv.get("top").and_then(Yv::as_f64).unwrap_or(0.0),
            right: yv.get("right").and_then(Yv::as_f64).unwrap_or(0.0),
            bottom: yv.get("bottom").and_then(Yv::as_f64).unwrap_or(0.0),
            left: yv.get("left").and_then(Yv::as_f64).unwrap_or(0.0),
        },
        _ => Edges::default(),
    }
}

fn parse_align(yv: &Yv) -> Option<Align> {
    match yv.as_str()?.to_ascii_lowercase().as_str() {
        "start" => Some(Align::Start),
        "center" => Some(Align::Center),
        "end" => Some(Align::End),
        "stretch" => Some(Align::Stretch),
        _ => None,
    }
}

fn parse_justify(yv: &Yv) -> Option<Justify> {
    match yv.as_str()?.to_ascii_lowercase().replace('_', "-").as_str() {
        "start" => Some(Justify::Start),
        "center" => Some(Justify::Center),
        "end" => Some(Justify::End),
        "space-between" => Some(Justify::SpaceBetween),
        "space-around" => Some(Justify::SpaceAround),
        _ => None,
    }
}

fn parse_color(yv: &Yv) -> Option<Color> {
    Color::parse(yv.as_str()?)
}

fn parse_anchor(yv: &Yv) -> Option<TextAnchor> {
    match yv.as_str()?.to_ascii_lowercase().as_str() {
        "start" | "left" => Some(TextAnchor::Start),
        "middle" | "center" => Some(TextAnchor::Middle),
        "end" | "right" => Some(TextAnchor::End),
        _ => None,
    }
}

/// Read the style override keys present on a node/canvas/connect mapping.
fn parse_style_patch(yv: &Yv) -> StylePatch {
    StylePatch {
        stroke: yv.get("stroke").and_then(parse_color),
        stroke_width: yv.get("stroke_width").and_then(Yv::as_f64),
        fill: yv.get("fill").and_then(parse_color),
        text_color: yv.get("color").and_then(parse_color),
        font_family: yv.get("font").and_then(Yv::as_str).map(str::to_string),
        font_size: yv.get("font_size").and_then(Yv::as_f64),
        text_anchor: yv.get("text_anchor").and_then(parse_anchor),
        opacity: yv.get("opacity").and_then(Yv::as_f64),
    }
}

// ---------------------------------------------------------------------------
// Minimal SVG path-data parser (absolute + relative M,L,H,V,C,Q,Z)
// ---------------------------------------------------------------------------

fn parse_path_data(d: &str) -> Result<Vec<PathCmd>> {
    let mut toks = PathTokenizer::new(d);
    let mut cmds = Vec::new();
    let mut cur = Point::ZERO;
    let mut start = Point::ZERO;
    let mut cmd = ' ';

    loop {
        let next_cmd = toks.peek_command();
        if let Some(c) = next_cmd {
            cmd = c;
            toks.advance();
        } else if cmd == ' ' {
            break; // no command and nothing pending
        }
        let rel = cmd.is_ascii_lowercase();
        let off = |p: Point, cur: Point| if rel { p.translate(cur.x, cur.y) } else { p };

        match cmd.to_ascii_uppercase() {
            'M' => {
                let Some(p) = toks.point()? else { break };
                cur = off(p, cur);
                start = cur;
                cmds.push(PathCmd::MoveTo(cur));
                // Subsequent implicit pairs are line-to.
                while let Some(p) = toks.point()? {
                    cur = off(p, cur);
                    cmds.push(PathCmd::LineTo(cur));
                }
            }
            'L' => {
                while let Some(p) = toks.point()? {
                    cur = off(p, cur);
                    cmds.push(PathCmd::LineTo(cur));
                }
            }
            'H' => {
                while let Some(x) = toks.number()? {
                    let nx = if rel { cur.x + x } else { x };
                    cur = Point::new(nx, cur.y);
                    cmds.push(PathCmd::LineTo(cur));
                }
            }
            'V' => {
                while let Some(y) = toks.number()? {
                    let ny = if rel { cur.y + y } else { y };
                    cur = Point::new(cur.x, ny);
                    cmds.push(PathCmd::LineTo(cur));
                }
            }
            'C' => {
                while let Some(c1) = toks.point()? {
                    let c2 = toks.point()?.ok_or_else(|| path_err("C"))?;
                    let e = toks.point()?.ok_or_else(|| path_err("C"))?;
                    let (c1, c2, e) = (off(c1, cur), off(c2, cur), off(e, cur));
                    cmds.push(PathCmd::CurveTo(c1, c2, e));
                    cur = e;
                }
            }
            'Q' => {
                while let Some(c1) = toks.point()? {
                    let e = toks.point()?.ok_or_else(|| path_err("Q"))?;
                    let (c1, e) = (off(c1, cur), off(e, cur));
                    cmds.push(PathCmd::QuadTo(c1, e));
                    cur = e;
                }
            }
            'Z' => {
                cmds.push(PathCmd::Close);
                cur = start;
            }
            other => return Err(Error::Parse(format!("unsupported path command {other:?}"))),
        }

        if toks.is_done() && toks.peek_command().is_none() {
            break;
        }
    }
    Ok(cmds)
}

fn path_err(cmd: &str) -> Error {
    Error::Parse(format!("path command {cmd} has too few coordinates"))
}

struct PathTokenizer {
    chars: Vec<char>,
    i: usize,
}

impl PathTokenizer {
    fn new(s: &str) -> Self {
        PathTokenizer { chars: s.chars().collect(), i: 0 }
    }

    fn skip_sep(&mut self) {
        while self.i < self.chars.len() {
            let c = self.chars[self.i];
            if c.is_whitespace() || c == ',' {
                self.i += 1;
            } else {
                break;
            }
        }
    }

    fn peek_command(&mut self) -> Option<char> {
        self.skip_sep();
        let c = *self.chars.get(self.i)?;
        if c.is_ascii_alphabetic() && c != 'e' && c != 'E' {
            Some(c)
        } else {
            None
        }
    }

    fn advance(&mut self) {
        self.i += 1;
    }

    fn is_done(&mut self) -> bool {
        self.skip_sep();
        self.i >= self.chars.len()
    }

    fn number(&mut self) -> Result<Option<f64>> {
        self.skip_sep();
        if self.peek_command().is_some() || self.i >= self.chars.len() {
            return Ok(None);
        }
        let start = self.i;
        if matches!(self.chars.get(self.i), Some('+') | Some('-')) {
            self.i += 1;
        }
        while self.i < self.chars.len() {
            let c = self.chars[self.i];
            if c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' || c == '+' || c == '-' {
                // Allow sign only right after exponent.
                if (c == '+' || c == '-') && !matches!(self.chars.get(self.i - 1), Some('e') | Some('E')) {
                    break;
                }
                self.i += 1;
            } else {
                break;
            }
        }
        let lit: String = self.chars[start..self.i].iter().collect();
        lit.parse::<f64>()
            .map(Some)
            .map_err(|_| Error::Parse(format!("invalid number in path: {lit:?}")))
    }

    fn point(&mut self) -> Result<Option<Point>> {
        let Some(x) = self.number()? else { return Ok(None) };
        let y = self
            .number()?
            .ok_or_else(|| Error::Parse("path coordinate missing y".into()))?;
        Ok(Some(Point::new(x, y)))
    }
}

// We resolve a node's `props` after constructing the symbol kind; wire it in here so the
// public parse_node stays linear.
fn attach_props(node: &mut Node, yv: &Yv) {
    if let NodeKind::Symbol(sym) = &mut node.kind {
        sym.props = parse_props(yv);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::BasicMeasurer;

    fn parse(src: &str) -> Result<Document> {
        parse_document(src, &BasicMeasurer::default())
    }

    #[test]
    fn parses_minimal_text_document() {
        let doc = parse("root:\n  text: Hello\n").unwrap();
        match &doc.root.kind {
            NodeKind::Text(t) => assert_eq!(t.text, "Hello"),
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[test]
    fn vars_and_expressions_resolve_to_numbers() {
        let src = "
vars:
  pad: 8
  w: ${pad * 10}
canvas:
  width: ${w + 4}
root:
  rect:
    width: ${w}
    height: 10
";
        let doc = parse(src).unwrap();
        assert_eq!(doc.canvas.width, Some(84.0));
        match &doc.root.kind {
            NodeKind::Rect(r) => assert_eq!(r.size.width, 80.0),
            other => panic!("expected rect, got {other:?}"),
        }
    }

    #[test]
    fn embedded_interpolation_makes_strings() {
        let src = "
vars:
  n: 3
root:
  text: \"R${n}\"
";
        let doc = parse(src).unwrap();
        match &doc.root.kind {
            NodeKind::Text(t) => assert_eq!(t.text, "R3"),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn parses_container_with_children_and_layout() {
        let src = "
root:
  vbox:
    - text: A
    - text: B
  gap: 10
  align: center
";
        let doc = parse(src).unwrap();
        match &doc.root.kind {
            NodeKind::Container(c) => {
                assert_eq!(c.direction, Direction::Column);
                assert_eq!(c.children.len(), 2);
                assert_eq!(c.gap, 10.0);
                assert_eq!(c.align, Align::Center);
            }
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn parses_symbol_with_props() {
        let src = "
root:
  symbol: schematic.resistor
  id: R1
  props:
    ohms: 220
    label: input
";
        let doc = parse(src).unwrap();
        assert_eq!(doc.root.id.as_deref(), Some("R1"));
        match &doc.root.kind {
            NodeKind::Symbol(s) => {
                assert_eq!(s.name, "schematic.resistor");
                assert_eq!(s.props.number_or("ohms", 0.0), 220.0);
                assert_eq!(s.props.text_or("label", ""), "input");
            }
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn parses_size_specs() {
        assert_eq!(parse_size_spec(&Yv::Int(40)), SizeSpec::Fixed(40.0));
        assert_eq!(parse_size_spec(&Yv::Str("auto".into())), SizeSpec::Auto);
        assert_eq!(parse_size_spec(&Yv::Str("grow".into())), SizeSpec::Grow(1.0));
        assert_eq!(parse_size_spec(&Yv::Str("grow 2".into())), SizeSpec::Grow(2.0));
        assert_eq!(parse_size_spec(&Yv::Str("grow:3".into())), SizeSpec::Grow(3.0));
    }

    #[test]
    fn parses_edges_forms() {
        assert_eq!(parse_edges(&Yv::Int(4)), Edges::all(4.0));
        let two = Yv::Seq(vec![Yv::Int(2), Yv::Int(6)]);
        assert_eq!(parse_edges(&two), Edges { top: 2.0, bottom: 2.0, left: 6.0, right: 6.0 });
    }

    #[test]
    fn parses_connections() {
        let src = "
root:
  hbox:
    - rect: { width: 10, height: 10 }
      id: a
    - rect: { width: 10, height: 10 }
      id: b
connect:
  - from: a
    to: b
    arrow: both
    routing: orthogonal
    label: link
";
        let doc = parse(src).unwrap();
        assert_eq!(doc.connections.len(), 1);
        let c = &doc.connections[0];
        assert_eq!(c.from.node, "a");
        assert_eq!(c.to.node, "b");
        assert_eq!(c.arrow, Arrow::Both);
        assert_eq!(c.routing, Routing::Orthogonal);
        assert_eq!(c.label.as_deref(), Some("link"));
    }

    #[test]
    fn connection_port_ref_splits() {
        let r = parse_port_ref("R1.a");
        assert_eq!(r.node, "R1");
        assert_eq!(r.port.as_deref(), Some("a"));
        let r2 = parse_port_ref("R1");
        assert_eq!(r2.node, "R1");
        assert!(r2.port.is_none());
    }

    #[test]
    fn parses_path_data_abs_and_rel() {
        let cmds = parse_path_data("M0 0 L10 0 l0 10 Z").unwrap();
        assert_eq!(cmds[0], PathCmd::MoveTo(Point::new(0.0, 0.0)));
        assert_eq!(cmds[1], PathCmd::LineTo(Point::new(10.0, 0.0)));
        assert_eq!(cmds[2], PathCmd::LineTo(Point::new(10.0, 10.0)));
        assert_eq!(cmds[3], PathCmd::Close);
    }

    #[test]
    fn missing_root_errors() {
        assert!(matches!(parse("canvas: { width: 10 }"), Err(Error::Parse(_))));
    }

    #[test]
    fn malformed_yaml_errors() {
        assert!(matches!(parse("root: [unclosed"), Err(Error::Yaml(_))));
    }
}
