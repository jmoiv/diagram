//! The box-model layout engine.
//!
//! Two passes:
//! 1. **measure** (`build`): bottom-up, compute each node's resolved [`Style`] and intrinsic
//!    [`Size`] (text via the [`TextMeasurer`], symbols via [`Symbol::measure`]).
//! 2. **place**: top-down, assign each node a concrete [`Rect`], distributing leftover space
//!    to `grow` children and positioning per `align`/`justify`; then draw each node into a
//!    flat list of [`Primitive`]s and record node rects/ports.
//!
//! Finally, [`Connect`]ions are routed between recorded ports. The result is a [`Scene`].

use std::collections::HashMap;

use crate::draw::{PathCmd, Primitive, ShapeStyle, Stroke, TextStyle};
use crate::error::{Error, Result};
use crate::geom::{Point, Rect, Size};
use crate::measure::TextMeasurer;
use crate::model::{
    Align, Arrow, Connect, Direction, Document, Justify, Node, NodeKind, PortRef, Routing, SizeSpec,
};
use crate::style::{Color, Style};
use crate::symbols::{Dir, Port, Props, Registry, Symbol};

/// A fully laid-out diagram, ready to render.
#[derive(Debug)]
pub struct Scene {
    pub size: Size,
    pub background: Option<Color>,
    pub primitives: Vec<Primitive>,
}

/// Lay out a document into a [`Scene`].
pub fn layout_document(
    doc: &Document,
    registry: &Registry,
    measurer: &dyn TextMeasurer,
) -> Result<Scene> {
    let base = Style::default().inherit(&doc.canvas.base_style);
    let lroot = build(&doc.root, &base, registry, measurer)?;

    let pad = doc.canvas.padding;
    let outer = lroot.outer_size();
    let canvas_w = doc.canvas.width.unwrap_or(outer.width + 2.0 * pad);
    let canvas_h = doc.canvas.height.unwrap_or(outer.height + 2.0 * pad);
    let content = Rect::new(
        pad,
        pad,
        (canvas_w - 2.0 * pad).max(0.0),
        (canvas_h - 2.0 * pad).max(0.0),
    );

    let mut out = LayoutOut::default();
    place(&lroot, content, measurer, &mut out);

    for c in &doc.connections {
        draw_connection(c, &out.nodes, &base, measurer, &mut out.primitives)?;
    }

    Ok(Scene {
        size: Size::new(canvas_w, canvas_h),
        background: doc.canvas.background,
        primitives: out.primitives,
    })
}

// ---------------------------------------------------------------------------
// Measure pass
// ---------------------------------------------------------------------------

/// A measured node: resolved style + intrinsic size, mirroring the model tree.
struct LNode<'a> {
    model: &'a Node,
    style: Style,
    intrinsic: Size,
    kind: LKind<'a>,
}

enum LKind<'a> {
    Container { children: Vec<LNode<'a>> },
    Symbol { sym: &'a dyn Symbol, props: Props },
    Leaf, // text / shapes / spacer; redrawn from model at place time
}

impl<'a> LNode<'a> {
    /// Effective base size along an axis: a `Fixed` spec overrides intrinsic.
    fn base_size(&self, axis: Axis) -> f64 {
        let (spec, intr) = match axis {
            Axis::Horizontal => (self.model.width, self.intrinsic.width),
            Axis::Vertical => (self.model.height, self.intrinsic.height),
        };
        match spec {
            SizeSpec::Fixed(v) => v,
            _ => intr,
        }
    }

    fn margin_main(&self, dir: Direction) -> f64 {
        let m = self.model.margin;
        match dir {
            Direction::Row => m.horizontal(),
            Direction::Column | Direction::Stack => m.vertical(),
        }
    }

    /// Outer intrinsic size (base size + margin), used to size the parent.
    fn outer_size(&self) -> Size {
        let m = self.model.margin;
        Size::new(
            self.base_size(Axis::Horizontal) + m.horizontal(),
            self.base_size(Axis::Vertical) + m.vertical(),
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis {
    Horizontal,
    Vertical,
}

fn build<'a>(
    node: &'a Node,
    parent_style: &Style,
    registry: &'a Registry,
    measurer: &dyn TextMeasurer,
) -> Result<LNode<'a>> {
    let style = parent_style.inherit(&node.style);

    let (intrinsic, kind) = match &node.kind {
        NodeKind::Container(c) => {
            let mut children = Vec::with_capacity(c.children.len());
            for child in &c.children {
                children.push(build(child, &style, registry, measurer)?);
            }
            let intrinsic = container_intrinsic(c.direction, c.gap, c.padding, &children);
            (intrinsic, LKind::Container { children })
        }
        NodeKind::Symbol(s) => {
            let sym = registry
                .get(&s.name)
                .ok_or_else(|| Error::UnknownSymbol(s.name.clone()))?;
            let props = s.props.clone().with_defaults(&sym.property_schema());
            let size = sym.measure(&props, measurer);
            (size, LKind::Symbol { sym, props })
        }
        NodeKind::Text(t) => {
            let size = match t.wrap {
                Some(w) => {
                    let p =
                        measurer.measure_paragraph(&t.text, w, &style.font_family, style.font_size);
                    Size::new(p.width, p.height)
                }
                None => {
                    let m = measurer.measure_line(&t.text, &style.font_family, style.font_size);
                    Size::new(m.width, m.height())
                }
            };
            (size, LKind::Leaf)
        }
        NodeKind::Line(l) => {
            let w = l.a.x.max(l.b.x);
            let h = l.a.y.max(l.b.y);
            (Size::new(w, h), LKind::Leaf)
        }
        NodeKind::Rect(r) => (r.size, LKind::Leaf),
        NodeKind::Circle(c) => (Size::new(2.0 * c.radius, 2.0 * c.radius), LKind::Leaf),
        NodeKind::Path(p) => (path_bbox(&p.cmds), LKind::Leaf),
        NodeKind::Spacer => (Size::ZERO, LKind::Leaf),
    };

    Ok(LNode {
        model: node,
        style,
        intrinsic,
        kind,
    })
}

fn container_intrinsic(
    dir: Direction,
    gap: f64,
    padding: crate::model::Edges,
    children: &[LNode],
) -> Size {
    let n = children.len();
    let (mut main, mut cross) = (0.0f64, 0.0f64);
    match dir {
        Direction::Row => {
            for ch in children {
                let o = ch.outer_size();
                main += o.width;
                cross = cross.max(o.height);
            }
            if n > 1 {
                main += gap * (n as f64 - 1.0);
            }
            Size::new(main + padding.horizontal(), cross + padding.vertical())
        }
        Direction::Column => {
            for ch in children {
                let o = ch.outer_size();
                main += o.height;
                cross = cross.max(o.width);
            }
            if n > 1 {
                main += gap * (n as f64 - 1.0);
            }
            Size::new(cross + padding.horizontal(), main + padding.vertical())
        }
        Direction::Stack => {
            for ch in children {
                let o = ch.outer_size();
                main = main.max(o.width);
                cross = cross.max(o.height);
            }
            Size::new(main + padding.horizontal(), cross + padding.vertical())
        }
    }
}

fn path_bbox(cmds: &[PathCmd]) -> Size {
    let mut maxx = 0.0f64;
    let mut maxy = 0.0f64;
    let mut acc = |p: &Point| {
        maxx = maxx.max(p.x);
        maxy = maxy.max(p.y);
    };
    for c in cmds {
        match c {
            PathCmd::MoveTo(p) | PathCmd::LineTo(p) => acc(p),
            PathCmd::QuadTo(c1, e) => {
                acc(c1);
                acc(e);
            }
            PathCmd::CurveTo(c1, c2, e) => {
                acc(c1);
                acc(c2);
                acc(e);
            }
            PathCmd::Close => {}
        }
    }
    Size::new(maxx, maxy)
}

// ---------------------------------------------------------------------------
// Place pass
// ---------------------------------------------------------------------------

#[derive(Default)]
struct LayoutOut {
    primitives: Vec<Primitive>,
    nodes: HashMap<String, ResolvedNode>,
}

struct ResolvedNode {
    rect: Rect,
    ports: Vec<Port>,
}

/// Place `lnode` into the outer rectangle `outer` (which includes the node's margin), draw
/// it, and record its rect/ports.
fn place(lnode: &LNode, outer: Rect, measurer: &dyn TextMeasurer, out: &mut LayoutOut) {
    let m = lnode.model.margin;
    let inner = Rect::new(
        outer.x + m.left,
        outer.y + m.top,
        (outer.width - m.horizontal()).max(0.0),
        (outer.height - m.vertical()).max(0.0),
    );

    match &lnode.kind {
        LKind::Container { children } => {
            if let NodeKind::Container(c) = &lnode.model.kind {
                place_container(c, children, inner, measurer, out);
            }
        }
        LKind::Symbol { sym, props } => {
            let mut painter = crate::draw::Painter::new();
            sym.draw(&mut painter, inner, props, &lnode.style, measurer);
            out.primitives.extend(painter.into_primitives());
            let ports = sym.ports(inner, props);
            record(out, lnode.model.id.as_deref(), inner, ports);
            return;
        }
        LKind::Leaf => {
            draw_leaf(lnode, inner, measurer, &mut out.primitives);
        }
    }
    record(out, lnode.model.id.as_deref(), inner, Vec::new());
}

fn record(out: &mut LayoutOut, id: Option<&str>, rect: Rect, ports: Vec<Port>) {
    if let Some(id) = id {
        out.nodes
            .insert(id.to_string(), ResolvedNode { rect, ports });
    }
}

fn place_container(
    c: &crate::model::Container,
    children: &[LNode],
    inner: Rect,
    measurer: &dyn TextMeasurer,
    out: &mut LayoutOut,
) {
    let content = Rect::new(
        inner.x + c.padding.left,
        inner.y + c.padding.top,
        (inner.width - c.padding.horizontal()).max(0.0),
        (inner.height - c.padding.vertical()).max(0.0),
    );

    if children.is_empty() {
        return;
    }

    if c.direction == Direction::Stack {
        for ch in children {
            let rect = stack_child_rect(c, ch, content);
            place(ch, rect, measurer, out);
        }
        return;
    }

    let dir = c.direction;
    let n = children.len();
    let main_avail = main_of(dir, content.size());
    let cross_avail = cross_of(dir, content.size());

    // Base main sizes (including margins) and grow weights.
    let mut base_total = 0.0;
    let mut grow_total = 0.0;
    for ch in children {
        base_total += child_base_main(ch, dir) + ch.margin_main(dir);
        if let Grow(w) = grow_spec(ch, dir) {
            grow_total += w;
        }
    }
    let gaps = c.gap * (n as f64 - 1.0);
    let leftover = (main_avail - base_total - gaps).max(0.0);

    // Distribute leftover: prefer grow; otherwise use justify.
    let (mut cursor, spacing) = if grow_total > 0.0 {
        (0.0, c.gap)
    } else {
        justify_offsets(c.justify, leftover, n, c.gap)
    };

    for ch in children {
        let mut child_main = child_base_main(ch, dir);
        if let Grow(w) = grow_spec(ch, dir) {
            if grow_total > 0.0 {
                child_main += leftover * (w / grow_total);
            }
        }
        let child_cross = child_cross_size(c, ch, dir, cross_avail);

        let (mm_lead, mm_trail) = margin_main_lead_trail(ch, dir);
        let (cm_lead, _) = margin_cross_lead_trail(ch, dir);

        let main_pos = cursor + mm_lead;
        let align = ch.model.align_self.unwrap_or(c.align);
        let cross_pos =
            align_offset(align, cross_avail, child_cross + cross_margin(ch, dir)) + cm_lead;

        let outer_rect = rect_from_axes(
            dir,
            content.origin(),
            main_pos,
            cross_pos,
            child_main,
            child_cross,
        );
        // Re-expand to outer (add margins back) so place() can subtract them.
        let outer_rect = expand_margins(ch, outer_rect);
        place(ch, outer_rect, measurer, out);

        cursor += mm_lead + child_main + mm_trail + spacing;
    }
}

use crate::model::SizeSpec::Grow;

fn grow_spec(ch: &LNode, dir: Direction) -> SizeSpec {
    match dir {
        Direction::Row => ch.model.width,
        Direction::Column | Direction::Stack => ch.model.height,
    }
}

fn child_base_main(ch: &LNode, dir: Direction) -> f64 {
    match dir {
        Direction::Row => ch.base_size(Axis::Horizontal),
        Direction::Column | Direction::Stack => ch.base_size(Axis::Vertical),
    }
}

fn cross_margin(ch: &LNode, dir: Direction) -> f64 {
    let m = ch.model.margin;
    match dir {
        Direction::Row => m.vertical(),
        Direction::Column | Direction::Stack => m.horizontal(),
    }
}

fn child_cross_size(
    c: &crate::model::Container,
    ch: &LNode,
    dir: Direction,
    cross_avail: f64,
) -> f64 {
    let cross_axis = match dir {
        Direction::Row => Axis::Vertical,
        Direction::Column | Direction::Stack => Axis::Horizontal,
    };
    let spec = match cross_axis {
        Axis::Horizontal => ch.model.width,
        Axis::Vertical => ch.model.height,
    };
    let align = ch.model.align_self.unwrap_or(c.align);
    match spec {
        SizeSpec::Fixed(v) => v,
        SizeSpec::Grow(_) => (cross_avail - cross_margin(ch, dir)).max(0.0),
        SizeSpec::Auto => {
            if align == Align::Stretch {
                (cross_avail - cross_margin(ch, dir)).max(0.0)
            } else {
                ch.base_size(cross_axis)
            }
        }
    }
}

fn margin_main_lead_trail(ch: &LNode, dir: Direction) -> (f64, f64) {
    let m = ch.model.margin;
    match dir {
        Direction::Row => (m.left, m.right),
        Direction::Column | Direction::Stack => (m.top, m.bottom),
    }
}

fn margin_cross_lead_trail(ch: &LNode, dir: Direction) -> (f64, f64) {
    let m = ch.model.margin;
    match dir {
        Direction::Row => (m.top, m.bottom),
        Direction::Column | Direction::Stack => (m.left, m.right),
    }
}

fn expand_margins(ch: &LNode, inner_rect: Rect) -> Rect {
    let m = ch.model.margin;
    Rect::new(
        inner_rect.x - m.left,
        inner_rect.y - m.top,
        inner_rect.width + m.horizontal(),
        inner_rect.height + m.vertical(),
    )
}

fn stack_child_rect(c: &crate::model::Container, ch: &LNode, content: Rect) -> Rect {
    let align = ch.model.align_self.unwrap_or(c.align);
    let w = match ch.model.width {
        SizeSpec::Fixed(v) => v,
        SizeSpec::Grow(_) => content.width,
        SizeSpec::Auto if align == Align::Stretch => content.width,
        SizeSpec::Auto => ch.base_size(Axis::Horizontal),
    };
    let h = match ch.model.height {
        SizeSpec::Fixed(v) => v,
        SizeSpec::Grow(_) => content.height,
        SizeSpec::Auto if align == Align::Stretch => content.height,
        SizeSpec::Auto => ch.base_size(Axis::Vertical),
    };
    // Center stacked children within the content box (a sensible default for overlays).
    let x = content.x + (content.width - w) / 2.0;
    let y = content.y + (content.height - h) / 2.0;
    expand_margins(ch, Rect::new(x, y, w, h))
}

fn main_of(dir: Direction, s: Size) -> f64 {
    match dir {
        Direction::Row => s.width,
        Direction::Column | Direction::Stack => s.height,
    }
}

fn cross_of(dir: Direction, s: Size) -> f64 {
    match dir {
        Direction::Row => s.height,
        Direction::Column | Direction::Stack => s.width,
    }
}

fn rect_from_axes(
    dir: Direction,
    origin: Point,
    main: f64,
    cross: f64,
    main_sz: f64,
    cross_sz: f64,
) -> Rect {
    match dir {
        Direction::Row => Rect::new(origin.x + main, origin.y + cross, main_sz, cross_sz),
        Direction::Column | Direction::Stack => {
            Rect::new(origin.x + cross, origin.y + main, cross_sz, main_sz)
        }
    }
}

fn align_offset(align: Align, avail: f64, used: f64) -> f64 {
    let free = (avail - used).max(0.0);
    match align {
        Align::Start | Align::Stretch => 0.0,
        Align::Center => free / 2.0,
        Align::End => free,
    }
}

/// Returns (start offset, spacing) for distributing `free` space among `n` items.
fn justify_offsets(justify: Justify, free: f64, n: usize, gap: f64) -> (f64, f64) {
    match justify {
        Justify::Start => (0.0, gap),
        Justify::Center => (free / 2.0, gap),
        Justify::End => (free, gap),
        Justify::SpaceBetween => {
            if n > 1 {
                (0.0, gap + free / (n as f64 - 1.0))
            } else {
                (free / 2.0, gap)
            }
        }
        Justify::SpaceAround => {
            let unit = free / n as f64;
            (unit / 2.0, gap + unit)
        }
    }
}

// ---------------------------------------------------------------------------
// Leaf drawing
// ---------------------------------------------------------------------------

fn stroke_of(style: &Style) -> Stroke {
    Stroke::new(style.stroke, style.stroke_width)
}

fn shape_of(style: &Style) -> ShapeStyle {
    ShapeStyle::new(style.fill, style.stroke, style.stroke_width)
}

fn text_style_of(style: &Style) -> TextStyle {
    TextStyle {
        color: style.text_color,
        font_family: style.font_family.clone(),
        font_size: style.font_size,
        anchor: style.text_anchor,
    }
}

fn anchored_x(style: &Style, rect: Rect) -> f64 {
    use crate::style::TextAnchor::*;
    match style.text_anchor {
        Start => rect.x,
        Middle => rect.center().x,
        End => rect.right(),
    }
}

fn draw_leaf(lnode: &LNode, inner: Rect, measurer: &dyn TextMeasurer, prims: &mut Vec<Primitive>) {
    let style = &lnode.style;
    match &lnode.model.kind {
        NodeKind::Text(t) => {
            let x = anchored_x(style, inner);
            match t.wrap {
                Some(w) => {
                    let p =
                        measurer.layout_paragraph(&t.text, w, &style.font_family, style.font_size);
                    let line = measurer.measure_line("Mg", &style.font_family, style.font_size);
                    for (i, wl) in p.lines.iter().enumerate() {
                        let baseline = inner.y + line.ascent + i as f64 * p.line_height;
                        prims.push(Primitive::Text {
                            pos: Point::new(x, baseline),
                            text: wl.text.clone(),
                            style: text_style_of(style),
                        });
                    }
                }
                None => {
                    let m = measurer.measure_line(&t.text, &style.font_family, style.font_size);
                    let baseline = inner.y + m.ascent;
                    prims.push(Primitive::Text {
                        pos: Point::new(x, baseline),
                        text: t.text.clone(),
                        style: text_style_of(style),
                    });
                }
            }
        }
        NodeKind::Line(l) => {
            prims.push(Primitive::Line {
                a: l.a.translate(inner.x, inner.y),
                b: l.b.translate(inner.x, inner.y),
                stroke: stroke_of(style),
            });
        }
        NodeKind::Rect(r) => {
            prims.push(Primitive::Rect {
                rect: inner,
                rx: r.rx,
                style: shape_of(style),
            });
        }
        NodeKind::Circle(_) => {
            let radius = inner.width.min(inner.height) / 2.0;
            prims.push(Primitive::Circle {
                center: inner.center(),
                radius,
                style: shape_of(style),
            });
        }
        NodeKind::Path(p) => {
            let cmds = p
                .cmds
                .iter()
                .map(|c| translate_cmd(c, inner.x, inner.y))
                .collect();
            prims.push(Primitive::Path {
                cmds,
                style: shape_of(style),
            });
        }
        NodeKind::Spacer | NodeKind::Container(_) | NodeKind::Symbol(_) => {}
    }
}

fn translate_cmd(c: &PathCmd, dx: f64, dy: f64) -> PathCmd {
    let t = |p: &Point| p.translate(dx, dy);
    match c {
        PathCmd::MoveTo(p) => PathCmd::MoveTo(t(p)),
        PathCmd::LineTo(p) => PathCmd::LineTo(t(p)),
        PathCmd::QuadTo(c1, e) => PathCmd::QuadTo(t(c1), t(e)),
        PathCmd::CurveTo(c1, c2, e) => PathCmd::CurveTo(t(c1), t(c2), t(e)),
        PathCmd::Close => PathCmd::Close,
    }
}

// ---------------------------------------------------------------------------
// Connections
// ---------------------------------------------------------------------------

fn resolve_port(
    r: &PortRef,
    nodes: &HashMap<String, ResolvedNode>,
) -> Result<(Point, Option<Dir>)> {
    let node = nodes
        .get(&r.node)
        .ok_or_else(|| Error::UnknownPort(format!("no node with id '{}'", r.node)))?;
    match &r.port {
        Some(pname) => {
            let port = node
                .ports
                .iter()
                .find(|p| &p.name == pname)
                .ok_or_else(|| Error::UnknownPort(format!("{}.{}", r.node, pname)))?;
            Ok((port.point, Some(port.dir)))
        }
        None => Ok((node.rect.center(), None)),
    }
}

fn draw_connection(
    c: &Connect,
    nodes: &HashMap<String, ResolvedNode>,
    base: &Style,
    measurer: &dyn TextMeasurer,
    prims: &mut Vec<Primitive>,
) -> Result<()> {
    let style = base.inherit(&c.style);
    let (p1, _d1) = resolve_port(&c.from, nodes)?;
    let (p2, _d2) = resolve_port(&c.to, nodes)?;

    let pts: Vec<Point> = match c.routing {
        Routing::Straight => vec![p1, p2],
        Routing::Orthogonal => {
            let midx = (p1.x + p2.x) / 2.0;
            vec![p1, Point::new(midx, p1.y), Point::new(midx, p2.y), p2]
        }
    };

    prims.push(Primitive::Polyline {
        points: pts.clone(),
        stroke: stroke_of(&style),
    });

    // Arrowheads.
    if matches!(c.arrow, Arrow::End | Arrow::Both) {
        let tip = pts[pts.len() - 1];
        let prev = pts[pts.len() - 2];
        push_arrowhead(prims, prev, tip, &style);
    }
    if matches!(c.arrow, Arrow::Both) {
        let tip = pts[0];
        let prev = pts[1];
        push_arrowhead(prims, prev, tip, &style);
    }

    // Optional midpoint label.
    if let Some(label) = &c.label {
        let mid = midpoint(&pts);
        let m = measurer.measure_line(label, &style.font_family, style.font_size);
        prims.push(Primitive::Text {
            pos: Point::new(mid.x, mid.y - m.descent - 2.0),
            text: label.clone(),
            style: TextStyle {
                color: style.text_color,
                font_family: style.font_family.clone(),
                font_size: style.font_size,
                anchor: crate::style::TextAnchor::Middle,
            },
        });
    }

    Ok(())
}

fn midpoint(pts: &[Point]) -> Point {
    // Midpoint of the polyline by arc length (good enough: midpoint of the middle segment).
    let mid_seg = pts.len() / 2;
    let a = pts[mid_seg.saturating_sub(1)];
    let b = pts[mid_seg];
    Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)
}

fn push_arrowhead(prims: &mut Vec<Primitive>, from: Point, tip: Point, style: &Style) {
    let dx = tip.x - from.x;
    let dy = tip.y - from.y;
    let len = (dx * dx + dy * dy).sqrt().max(1e-6);
    let (ux, uy) = (dx / len, dy / len);
    let size = 6.0 + style.stroke_width;
    // Perpendicular.
    let (px, py) = (-uy, ux);
    let back = Point::new(tip.x - ux * size, tip.y - uy * size);
    let left = Point::new(back.x + px * size * 0.5, back.y + py * size * 0.5);
    let right = Point::new(back.x - px * size * 0.5, back.y - py * size * 0.5);
    prims.push(Primitive::Path {
        cmds: vec![
            PathCmd::MoveTo(tip),
            PathCmd::LineTo(left),
            PathCmd::LineTo(right),
            PathCmd::Close,
        ],
        style: ShapeStyle::new(style.stroke, style.stroke, style.stroke_width),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::BasicMeasurer;
    use crate::model::*;
    use crate::style::Color;

    fn registry() -> Registry {
        Registry::new()
    }

    fn text_node(s: &str) -> Node {
        Node::new(NodeKind::Text(TextNode {
            text: s.to_string(),
            wrap: None,
        }))
    }

    fn hbox(children: Vec<Node>, gap: f64) -> Node {
        Node::new(NodeKind::Container(Container {
            direction: Direction::Row,
            children,
            gap,
            padding: Edges::default(),
            align: Align::Start,
            justify: Justify::Start,
        }))
    }

    fn doc(root: Node) -> Document {
        Document {
            canvas: Canvas::default(),
            root,
            connections: vec![],
        }
    }

    #[test]
    fn auto_canvas_fits_content_plus_padding() {
        // A 40x10 rect; canvas auto-sizes to rect + 2*padding(10) = 60x30.
        let mut rect = Node::new(NodeKind::Rect(RectShape {
            size: Size::new(40.0, 10.0),
            rx: 0.0,
        }));
        rect.width = SizeSpec::Fixed(40.0);
        rect.height = SizeSpec::Fixed(10.0);
        let scene = layout_document(&doc(rect), &registry(), &BasicMeasurer::default()).unwrap();
        assert_eq!(scene.size, Size::new(60.0, 30.0));
    }

    #[test]
    fn row_lays_children_left_to_right_with_gap() {
        let mut a = Node::new(NodeKind::Rect(RectShape {
            size: Size::new(10.0, 10.0),
            rx: 0.0,
        }));
        a.width = SizeSpec::Fixed(10.0);
        a.height = SizeSpec::Fixed(10.0);
        a.id = Some("a".into());
        let mut b = a.clone();
        b.id = Some("b".into());
        let root = hbox(vec![a, b], 5.0);
        let scene = layout_document(&doc(root), &registry(), &BasicMeasurer::default()).unwrap();
        // Intrinsic main = 10 + 5 + 10 = 25, cross 10; canvas = 45 x 30.
        assert_eq!(scene.size, Size::new(45.0, 30.0));
        // Find the two rects among primitives.
        let rects: Vec<&Rect> = scene
            .primitives
            .iter()
            .filter_map(|p| match p {
                Primitive::Rect { rect, .. } => Some(rect),
                _ => None,
            })
            .collect();
        assert_eq!(rects.len(), 2);
        // Second rect starts 15 to the right of the first (10 width + 5 gap).
        assert!((rects[1].x - rects[0].x - 15.0).abs() < 1e-9);
    }

    #[test]
    fn grow_child_fills_remaining_space() {
        // Fixed canvas 120 wide; one fixed 20 child + one grow child share the row.
        // content width = 120 - 2*padding(10) = 100.
        let canvas = Canvas { width: Some(120.0), height: Some(40.0), padding: 10.0, ..Default::default() };

        let mut fixed = Node::new(NodeKind::Spacer);
        fixed.width = SizeSpec::Fixed(20.0);
        fixed.height = SizeSpec::Fixed(10.0);
        let mut grow = Node::new(NodeKind::Rect(RectShape {
            size: Size::ZERO,
            rx: 0.0,
        }));
        grow.width = SizeSpec::Grow(1.0);
        grow.height = SizeSpec::Fixed(10.0);
        grow.id = Some("g".into());

        let root = Node::new(NodeKind::Container(Container {
            direction: Direction::Row,
            children: vec![fixed, grow],
            gap: 0.0,
            padding: Edges::default(),
            align: Align::Start,
            justify: Justify::Start,
        }));
        let document = Document {
            canvas,
            root,
            connections: vec![],
        };
        let scene = layout_document(&document, &registry(), &BasicMeasurer::default()).unwrap();
        let grow_rect = scene
            .primitives
            .iter()
            .find_map(|p| match p {
                Primitive::Rect { rect, .. } => Some(rect),
                _ => None,
            })
            .unwrap();
        // Content width 100, fixed 20 -> grow gets 80.
        assert!(
            (grow_rect.width - 80.0).abs() < 1e-9,
            "grow width was {}",
            grow_rect.width
        );
    }

    #[test]
    fn text_emits_a_text_primitive() {
        let scene = layout_document(
            &doc(text_node("hello")),
            &registry(),
            &BasicMeasurer::default(),
        )
        .unwrap();
        assert!(scene
            .primitives
            .iter()
            .any(|p| matches!(p, Primitive::Text { text, .. } if text == "hello")));
    }

    #[test]
    fn center_align_centers_cross_axis() {
        // Column with a wide and a narrow child, align center.
        let mut wide = Node::new(NodeKind::Rect(RectShape {
            size: Size::new(40.0, 10.0),
            rx: 0.0,
        }));
        wide.width = SizeSpec::Fixed(40.0);
        wide.height = SizeSpec::Fixed(10.0);
        let mut narrow = Node::new(NodeKind::Rect(RectShape {
            size: Size::new(10.0, 10.0),
            rx: 0.0,
        }));
        narrow.width = SizeSpec::Fixed(10.0);
        narrow.height = SizeSpec::Fixed(10.0);
        narrow.id = Some("n".into());
        let root = Node::new(NodeKind::Container(Container {
            direction: Direction::Column,
            children: vec![wide, narrow],
            gap: 0.0,
            padding: Edges::default(),
            align: Align::Center,
            justify: Justify::Start,
        }));
        let scene = layout_document(&doc(root), &registry(), &BasicMeasurer::default()).unwrap();
        let rects: Vec<&Rect> = scene
            .primitives
            .iter()
            .filter_map(|p| match p {
                Primitive::Rect { rect, .. } => Some(rect),
                _ => None,
            })
            .collect();
        // Narrow (10 wide) centered under wide (40) -> offset 15 from wide's left.
        let wide_x = rects[0].x;
        let narrow_x = rects[1].x;
        assert!(
            (narrow_x - wide_x - 15.0).abs() < 1e-9,
            "narrow_x={narrow_x} wide_x={wide_x}"
        );
    }

    #[test]
    fn unknown_symbol_errors() {
        let root = Node::new(NodeKind::Symbol(SymbolNode {
            name: "nope.nothing".into(),
            props: Props::new(),
        }));
        let err = layout_document(&doc(root), &registry(), &BasicMeasurer::default()).unwrap_err();
        assert!(matches!(err, Error::UnknownSymbol(_)));
    }

    #[test]
    fn connection_between_nodes_emits_polyline() {
        let mut a = Node::new(NodeKind::Rect(RectShape {
            size: Size::new(10.0, 10.0),
            rx: 0.0,
        }));
        a.width = SizeSpec::Fixed(10.0);
        a.height = SizeSpec::Fixed(10.0);
        a.id = Some("a".into());
        let mut b = a.clone();
        b.id = Some("b".into());
        let root = hbox(vec![a, b], 20.0);
        let mut document = doc(root);
        document.connections.push(Connect {
            from: PortRef {
                node: "a".into(),
                port: None,
            },
            to: PortRef {
                node: "b".into(),
                port: None,
            },
            style: Default::default(),
            routing: Routing::Straight,
            arrow: Arrow::End,
            label: Some("x".into()),
        });
        let _ = Color::BLACK; // keep import used
        let scene = layout_document(&document, &registry(), &BasicMeasurer::default()).unwrap();
        assert!(scene
            .primitives
            .iter()
            .any(|p| matches!(p, Primitive::Polyline { .. })));
        // Arrowhead path + label text present.
        assert!(scene
            .primitives
            .iter()
            .any(|p| matches!(p, Primitive::Path { .. })));
        assert!(scene
            .primitives
            .iter()
            .any(|p| matches!(p, Primitive::Text { text, .. } if text == "x")));
    }

    #[test]
    fn connection_to_missing_node_errors() {
        let root = text_node("x");
        let mut document = doc(root);
        document.connections.push(Connect {
            from: PortRef {
                node: "missing".into(),
                port: None,
            },
            to: PortRef {
                node: "also_missing".into(),
                port: None,
            },
            style: Default::default(),
            routing: Routing::Straight,
            arrow: Arrow::None,
            label: None,
        });
        let err = layout_document(&document, &registry(), &BasicMeasurer::default()).unwrap_err();
        assert!(matches!(err, Error::UnknownPort(_)));
    }
}
