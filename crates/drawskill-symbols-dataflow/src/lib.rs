//! Data-flow diagram (DFD) symbols for drawskill.
//!
//! Provides the `dataflow` plugin: `process` (circle), `entity` (external entity rectangle),
//! `store` (open-ended data store), `flow` (a labeled arrow), and `boundary` (a dashed trust
//! boundary box). Process/entity/store expose compass ports `n`, `e`, `s`, `w`; `flow` uses
//! `a`/`b`.

use drawskill_core::draw::{Painter, ShapeStyle, Stroke, TextStyle};
use drawskill_core::geom::{Point, Rect, Size};
use drawskill_core::measure::TextMeasurer;
use drawskill_core::style::{Style, TextAnchor};
use drawskill_core::symbols::{
    Dir, PropKind, PropValue, PropertySpec, Port, Props, Symbol, SymbolPlugin,
};

/// The data-flow diagram symbol plugin.
pub struct Dataflow;

impl SymbolPlugin for Dataflow {
    fn id(&self) -> &str {
        "dataflow"
    }
    fn description(&self) -> &str {
        "Data-flow diagram (DFD) symbols: process, entity, store, flow, trust boundary."
    }
    fn symbols(&self) -> Vec<Box<dyn Symbol>> {
        vec![
            Box::new(Process),
            Box::new(Entity),
            Box::new(Store),
            Box::new(Flow),
            Box::new(Boundary),
        ]
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn stroke(style: &Style) -> Stroke {
    Stroke::new(style.stroke, style.stroke_width.max(1.0))
}

fn outline(style: &Style) -> ShapeStyle {
    ShapeStyle::outline(style.stroke, style.stroke_width.max(1.0))
}

fn filled(style: &Style) -> ShapeStyle {
    ShapeStyle::new(style.stroke, style.stroke, style.stroke_width.max(1.0))
}

fn centered_text(p: &mut Painter, at: Point, text: &str, size: f64, style: &Style) {
    p.text(
        at,
        text,
        TextStyle {
            color: style.text_color,
            font_family: style.font_family.clone(),
            font_size: size,
            anchor: TextAnchor::Middle,
        },
    );
}

/// Compass ports on the edges of `bounds`.
fn compass_ports(bounds: Rect) -> Vec<Port> {
    let c = bounds.center();
    vec![
        Port::new("n", Point::new(c.x, bounds.y), Dir::Up),
        Port::new("e", Point::new(bounds.right(), c.y), Dir::Right),
        Port::new("s", Point::new(c.x, bounds.bottom()), Dir::Down),
        Port::new("w", Point::new(bounds.x, c.y), Dir::Left),
    ]
}

fn label_of(props: &Props) -> &str {
    props.text_or("label", "")
}

fn number_of(props: &Props) -> Option<String> {
    match props.get("number") {
        Some(PropValue::Number(n)) => Some(drawskill_core::expr::format_num(*n)),
        Some(PropValue::Text(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

fn label_props() -> Vec<PropertySpec> {
    vec![
        PropertySpec::optional("label", PropKind::Text, PropValue::text(""), "Text shown inside the symbol."),
        PropertySpec::optional("number", PropKind::Text, PropValue::text(""), "Optional identifier shown in a corner."),
    ]
}

/// Draw a dashed line from `a` to `b`.
fn dashed_line(p: &mut Painter, a: Point, b: Point, dash: f64, gap: f64, style: &Style) {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-6 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let mut t = 0.0;
    while t < len {
        let end = (t + dash).min(len);
        p.line(
            Point::new(a.x + ux * t, a.y + uy * t),
            Point::new(a.x + ux * end, a.y + uy * end),
            stroke(style),
        );
        t += dash + gap;
    }
}

// ---------------------------------------------------------------------------
// Process (circle)
// ---------------------------------------------------------------------------

pub struct Process;

impl Symbol for Process {
    fn name(&self) -> &str {
        "process"
    }
    fn description(&self) -> &str {
        "A DFD process (circle) with a `label` and optional `number`."
    }
    fn property_schema(&self) -> Vec<PropertySpec> {
        label_props()
    }
    fn measure(&self, props: &Props, m: &dyn TextMeasurer) -> Size {
        let w = m.measure_line(label_of(props), "sans-serif", 12.0).width;
        let d = (w + 28.0).max(56.0);
        Size::new(d, d)
    }
    fn ports(&self, bounds: Rect, _props: &Props) -> Vec<Port> {
        compass_ports(bounds)
    }
    fn draw(&self, p: &mut Painter, bounds: Rect, props: &Props, style: &Style, _m: &dyn TextMeasurer) {
        let c = bounds.center();
        let r = bounds.width.min(bounds.height) / 2.0;
        p.circle(c, r, outline(style));
        centered_text(p, c, label_of(props), 12.0, style);
        if let Some(n) = number_of(props) {
            centered_text(p, Point::new(c.x, bounds.y + 12.0), &n, 10.0, style);
        }
    }
}

// ---------------------------------------------------------------------------
// External entity (rectangle)
// ---------------------------------------------------------------------------

pub struct Entity;

impl Symbol for Entity {
    fn name(&self) -> &str {
        "entity"
    }
    fn description(&self) -> &str {
        "An external entity (rectangle) with a `label`."
    }
    fn property_schema(&self) -> Vec<PropertySpec> {
        label_props()
    }
    fn measure(&self, props: &Props, m: &dyn TextMeasurer) -> Size {
        let w = m.measure_line(label_of(props), "sans-serif", 12.0).width;
        Size::new((w + 24.0).max(64.0), 40.0)
    }
    fn ports(&self, bounds: Rect, _props: &Props) -> Vec<Port> {
        compass_ports(bounds)
    }
    fn draw(&self, p: &mut Painter, bounds: Rect, props: &Props, style: &Style, _m: &dyn TextMeasurer) {
        p.rect(bounds, 0.0, outline(style));
        centered_text(p, bounds.center(), label_of(props), 12.0, style);
    }
}

// ---------------------------------------------------------------------------
// Data store (open-ended)
// ---------------------------------------------------------------------------

pub struct Store;

impl Symbol for Store {
    fn name(&self) -> &str {
        "store"
    }
    fn description(&self) -> &str {
        "A data store (open-ended rectangle) with a `number` cell and a `label`."
    }
    fn property_schema(&self) -> Vec<PropertySpec> {
        label_props()
    }
    fn measure(&self, props: &Props, m: &dyn TextMeasurer) -> Size {
        let w = m.measure_line(label_of(props), "sans-serif", 12.0).width;
        Size::new((w + 48.0).max(80.0), 36.0)
    }
    fn ports(&self, bounds: Rect, _props: &Props) -> Vec<Port> {
        compass_ports(bounds)
    }
    fn draw(&self, p: &mut Painter, bounds: Rect, props: &Props, style: &Style, _m: &dyn TextMeasurer) {
        let top = bounds.y;
        let bottom = bounds.bottom();
        // Two horizontal lines and a closed left edge (right side stays open).
        p.line(Point::new(bounds.x, top), Point::new(bounds.right(), top), stroke(style));
        p.line(Point::new(bounds.x, bottom), Point::new(bounds.right(), bottom), stroke(style));
        p.line(Point::new(bounds.x, top), Point::new(bounds.x, bottom), stroke(style));

        let mut text_x = bounds.x + 8.0;
        if let Some(n) = number_of(props) {
            let cell_x = bounds.x + 26.0;
            p.line(Point::new(cell_x, top), Point::new(cell_x, bottom), stroke(style));
            centered_text(p, Point::new((bounds.x + cell_x) / 2.0, bounds.center().y + 4.0), &n, 11.0, style);
            text_x = cell_x + 8.0;
        }
        // Label, left-aligned after the optional number cell.
        p.text(
            Point::new(text_x, bounds.center().y + 4.0),
            label_of(props),
            TextStyle {
                color: style.text_color,
                font_family: style.font_family.clone(),
                font_size: 12.0,
                anchor: TextAnchor::Start,
            },
        );
    }
}

// ---------------------------------------------------------------------------
// Flow (labeled arrow)
// ---------------------------------------------------------------------------

pub struct Flow;

impl Symbol for Flow {
    fn name(&self) -> &str {
        "flow"
    }
    fn description(&self) -> &str {
        "A standalone labeled data-flow arrow from port `a` (left) to `b` (right)."
    }
    fn property_schema(&self) -> Vec<PropertySpec> {
        vec![PropertySpec::optional("label", PropKind::Text, PropValue::text(""), "Flow label, drawn above the arrow.")]
    }
    fn measure(&self, props: &Props, m: &dyn TextMeasurer) -> Size {
        let w = m.measure_line(label_of(props), "sans-serif", 11.0).width;
        let has_label = !label_of(props).is_empty();
        Size::new((w + 16.0).max(60.0), if has_label { 22.0 } else { 10.0 })
    }
    fn ports(&self, bounds: Rect, _props: &Props) -> Vec<Port> {
        let cy = bounds.bottom() - 4.0;
        vec![
            Port::new("a", Point::new(bounds.x, cy), Dir::Left),
            Port::new("b", Point::new(bounds.right(), cy), Dir::Right),
        ]
    }
    fn draw(&self, p: &mut Painter, bounds: Rect, props: &Props, style: &Style, _m: &dyn TextMeasurer) {
        let cy = bounds.bottom() - 4.0;
        p.line(Point::new(bounds.x, cy), Point::new(bounds.right() - 6.0, cy), stroke(style));
        // Arrowhead at the right.
        let tip = Point::new(bounds.right(), cy);
        p.path(
            vec![
                drawskill_core::draw::PathCmd::MoveTo(tip),
                drawskill_core::draw::PathCmd::LineTo(Point::new(tip.x - 7.0, cy - 3.5)),
                drawskill_core::draw::PathCmd::LineTo(Point::new(tip.x - 7.0, cy + 3.5)),
                drawskill_core::draw::PathCmd::Close,
            ],
            filled(style),
        );
        if !label_of(props).is_empty() {
            centered_text(p, Point::new(bounds.center().x, bounds.y + 10.0), label_of(props), 11.0, style);
        }
    }
}

// ---------------------------------------------------------------------------
// Trust boundary (dashed box)
// ---------------------------------------------------------------------------

pub struct Boundary;

impl Symbol for Boundary {
    fn name(&self) -> &str {
        "boundary"
    }
    fn description(&self) -> &str {
        "A trust boundary: a dashed rounded box with a corner `label`. Set width/height to wrap content."
    }
    fn property_schema(&self) -> Vec<PropertySpec> {
        vec![PropertySpec::optional("label", PropKind::Text, PropValue::text(""), "Boundary label, drawn at the top-left.")]
    }
    fn measure(&self, _props: &Props, _m: &dyn TextMeasurer) -> Size {
        // A sensible default; usually overridden with explicit width/height.
        Size::new(160.0, 100.0)
    }
    fn ports(&self, bounds: Rect, _props: &Props) -> Vec<Port> {
        compass_ports(bounds)
    }
    fn draw(&self, p: &mut Painter, bounds: Rect, props: &Props, style: &Style, _m: &dyn TextMeasurer) {
        let tl = Point::new(bounds.x, bounds.y);
        let tr = Point::new(bounds.right(), bounds.y);
        let br = Point::new(bounds.right(), bounds.bottom());
        let bl = Point::new(bounds.x, bounds.bottom());
        for (a, b) in [(tl, tr), (tr, br), (br, bl), (bl, tl)] {
            dashed_line(p, a, b, 6.0, 4.0, style);
        }
        if !label_of(props).is_empty() {
            p.text(
                Point::new(bounds.x + 6.0, bounds.y + 14.0),
                label_of(props),
                TextStyle {
                    color: style.text_color,
                    font_family: style.font_family.clone(),
                    font_size: 11.0,
                    anchor: TextAnchor::Start,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use drawskill_core::draw::Primitive;
    use drawskill_core::measure::BasicMeasurer;
    use drawskill_core::symbols::Registry;

    fn registry() -> Registry {
        let mut r = Registry::new();
        r.register(&Dataflow);
        r
    }

    #[test]
    fn registers_all_symbols() {
        let reg = registry();
        for n in [
            "dataflow.process",
            "dataflow.entity",
            "dataflow.store",
            "dataflow.flow",
            "dataflow.boundary",
        ] {
            assert!(reg.get(n).is_some(), "missing {n}");
        }
    }

    #[test]
    fn process_has_compass_ports_and_circle() {
        let proc = Process;
        let mut props = Props::new();
        props.insert("label", PropValue::text("validate"));
        let m = BasicMeasurer::default();
        let size = proc.measure(&props, &m);
        let bounds = Rect::from_origin_size(Point::ZERO, size);
        let ports = proc.ports(bounds, &props);
        let names: Vec<&str> = ports.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["n", "e", "s", "w"]);

        let mut p = Painter::new();
        proc.draw(&mut p, bounds, &props, &Style::default(), &m);
        assert!(p.primitives().iter().any(|x| matches!(x, Primitive::Circle { .. })));
        assert!(p
            .primitives()
            .iter()
            .any(|x| matches!(x, Primitive::Text { text, .. } if text == "validate")));
    }

    #[test]
    fn process_grows_with_label() {
        let proc = Process;
        let m = BasicMeasurer::default();
        let small = proc.measure(&{
            let mut p = Props::new();
            p.insert("label", PropValue::text("x"));
            p
        }, &m);
        let big = proc.measure(&{
            let mut p = Props::new();
            p.insert("label", PropValue::text("a very long process name"));
            p
        }, &m);
        assert!(big.width > small.width);
    }

    #[test]
    fn store_draws_number_cell_and_label() {
        let store = Store;
        let mut props = Props::new();
        props.insert("number", PropValue::text("D1"));
        props.insert("label", PropValue::text("Users"));
        let m = BasicMeasurer::default();
        let bounds = Rect::from_origin_size(Point::ZERO, store.measure(&props, &m));
        let mut p = Painter::new();
        store.draw(&mut p, bounds, &props, &Style::default(), &m);
        let texts: Vec<&str> = p
            .primitives()
            .iter()
            .filter_map(|x| match x {
                Primitive::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(texts.contains(&"D1"));
        assert!(texts.contains(&"Users"));
        // Open-ended: it draws individual lines, not a closed rect.
        assert!(p.primitives().iter().filter(|x| matches!(x, Primitive::Line { .. })).count() >= 3);
    }

    #[test]
    fn boundary_is_dashed_many_segments() {
        let b = Boundary;
        let bounds = Rect::new(0.0, 0.0, 160.0, 100.0);
        let mut p = Painter::new();
        b.draw(&mut p, bounds, &Props::new(), &Style::default(), &BasicMeasurer::default());
        // A dashed box produces many short line segments.
        let lines = p.primitives().iter().filter(|x| matches!(x, Primitive::Line { .. })).count();
        assert!(lines > 8, "expected many dash segments, got {lines}");
    }

    #[test]
    fn flow_has_two_ports_and_arrowhead() {
        let flow = Flow;
        let mut props = Props::new();
        props.insert("label", PropValue::text("request"));
        let m = BasicMeasurer::default();
        let bounds = Rect::from_origin_size(Point::ZERO, flow.measure(&props, &m));
        let ports = flow.ports(bounds, &props);
        assert_eq!(ports.iter().map(|p| p.name.clone()).collect::<Vec<_>>(), vec!["a", "b"]);
        let mut p = Painter::new();
        flow.draw(&mut p, bounds, &props, &Style::default(), &m);
        assert!(p.primitives().iter().any(|x| matches!(x, Primitive::Path { .. })));
    }
}
