//! Generic shape symbols for drawskill.
//!
//! Provides the `shapes` plugin: `rectangle`, `circle`, `oval` (ellipse), `explosion`
//! (a starburst callout), and `arrow` (a big block arrow). Every shape accepts a `fill`
//! (background color) and a `text_color` property in addition to its `label`. Filled
//! shapes (`rectangle`, `circle`, `oval`, `explosion`) expose compass ports `n`, `e`, `s`,
//! `w`; `arrow` exposes its tail port `a` and tip port `b`.

use drawskill_core::draw::{Painter, PathCmd, ShapeStyle, TextStyle};
use drawskill_core::geom::{Point, Rect, Size};
use drawskill_core::measure::TextMeasurer;
use drawskill_core::style::{Color, Style, TextAnchor};
use drawskill_core::symbols::{
    Dir, Port, PropKind, PropValue, PropertySpec, Props, Symbol, SymbolPlugin,
};

/// The generic-shapes symbol plugin.
pub struct Shapes;

impl SymbolPlugin for Shapes {
    fn id(&self) -> &str {
        "shapes"
    }
    fn description(&self) -> &str {
        "Generic shapes: rectangle, circle, oval, explosion callout, and block arrow."
    }
    fn symbols(&self) -> Vec<Box<dyn Symbol>> {
        vec![
            Box::new(Rectangle),
            Box::new(Circle),
            Box::new(Oval),
            Box::new(Explosion),
            Box::new(Arrow),
        ]
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Bézier control-point ratio for approximating a quarter ellipse with a cubic curve.
const KAPPA: f64 = 0.552_284_749_830_793_4;

fn label_of(props: &Props) -> &str {
    props.text_or("label", "")
}

/// Resolve the background fill: the `fill` property if set, else white. Unparseable colors
/// fall back to the inherited fill (or white if that is transparent).
fn fill_color(props: &Props, style: &Style) -> Color {
    match props.get("fill") {
        Some(PropValue::Text(s)) if !s.is_empty() => Color::parse(s).unwrap_or_else(|| {
            if style.fill.is_transparent() {
                Color::WHITE
            } else {
                style.fill
            }
        }),
        _ => Color::WHITE,
    }
}

/// Resolve the text color: the `text_color` property if set, else the inherited text color.
fn text_color(props: &Props, style: &Style) -> Color {
    match props.get("text_color") {
        Some(PropValue::Text(s)) if !s.is_empty() => Color::parse(s).unwrap_or(style.text_color),
        _ => style.text_color,
    }
}

/// A filled-shape style: the resolved `fill` background, the inherited stroke.
fn shape_style(props: &Props, style: &Style) -> ShapeStyle {
    ShapeStyle::new(
        fill_color(props, style),
        style.stroke,
        style.stroke_width.max(1.0),
    )
}

fn centered_text(p: &mut Painter, at: Point, text: &str, size: f64, color: Color, style: &Style) {
    if text.is_empty() {
        return;
    }
    p.text(
        // Nudge down so the baseline sits roughly on the vertical center.
        Point::new(at.x, at.y + size * 0.34),
        text,
        TextStyle {
            color,
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

/// Common properties shared by every shape: a label plus the background and text colors.
fn color_props(extra: &str) -> Vec<PropertySpec> {
    vec![
        PropertySpec::optional(
            "label",
            PropKind::Text,
            PropValue::text(""),
            "Text drawn inside the shape.",
        ),
        PropertySpec::optional(
            "fill",
            PropKind::Text,
            PropValue::text("white"),
            "Background color (name or #hex). Defaults to white.",
        ),
        PropertySpec::optional(
            "text_color",
            PropKind::Text,
            PropValue::text(""),
            "Label color (name or #hex). Defaults to the inherited text color.",
        ),
    ]
    .into_iter()
    .chain(extra_props(extra))
    .collect()
}

/// Per-shape extra properties keyed by shape name (keeps `color_props` readable).
fn extra_props(which: &str) -> Vec<PropertySpec> {
    match which {
        "rectangle" => vec![PropertySpec::optional(
            "rx",
            PropKind::Number,
            PropValue::Number(0.0),
            "Corner radius (0 = square corners).",
        )],
        "explosion" => vec![PropertySpec::optional(
            "spikes",
            PropKind::Number,
            PropValue::Number(12.0),
            "Number of spikes on the starburst.",
        )],
        "arrow" => vec![PropertySpec::optional(
            "direction",
            PropKind::Enum(&["right", "left", "up", "down"]),
            PropValue::text("right"),
            "Direction the arrow points.",
        )],
        _ => vec![],
    }
}

/// Build a closed polygon path (filled) from a list of vertices.
fn polygon(points: &[Point]) -> Vec<PathCmd> {
    let mut cmds = Vec::with_capacity(points.len() + 2);
    if let Some(first) = points.first() {
        cmds.push(PathCmd::MoveTo(*first));
        for pt in &points[1..] {
            cmds.push(PathCmd::LineTo(*pt));
        }
        cmds.push(PathCmd::Close);
    }
    cmds
}

// ---------------------------------------------------------------------------
// Rectangle
// ---------------------------------------------------------------------------

pub struct Rectangle;

impl Symbol for Rectangle {
    fn name(&self) -> &str {
        "rectangle"
    }
    fn description(&self) -> &str {
        "A rectangle (optionally rounded via `rx`) with a `label`, `fill`, and `text_color`."
    }
    fn property_schema(&self) -> Vec<PropertySpec> {
        color_props("rectangle")
    }
    fn measure(&self, props: &Props, m: &dyn TextMeasurer) -> Size {
        let w = m.measure_line(label_of(props), "sans-serif", 13.0).width;
        Size::new((w + 28.0).max(80.0), 44.0)
    }
    fn ports(&self, bounds: Rect, _props: &Props) -> Vec<Port> {
        compass_ports(bounds)
    }
    fn draw(
        &self,
        p: &mut Painter,
        bounds: Rect,
        props: &Props,
        style: &Style,
        _m: &dyn TextMeasurer,
    ) {
        let rx = props.number_or("rx", 0.0).max(0.0);
        p.rect(bounds, rx, shape_style(props, style));
        centered_text(
            p,
            bounds.center(),
            label_of(props),
            13.0,
            text_color(props, style),
            style,
        );
    }
}

// ---------------------------------------------------------------------------
// Circle
// ---------------------------------------------------------------------------

pub struct Circle;

impl Symbol for Circle {
    fn name(&self) -> &str {
        "circle"
    }
    fn description(&self) -> &str {
        "A circle with a `label`, `fill`, and `text_color`."
    }
    fn property_schema(&self) -> Vec<PropertySpec> {
        color_props("")
    }
    fn measure(&self, props: &Props, m: &dyn TextMeasurer) -> Size {
        let w = m.measure_line(label_of(props), "sans-serif", 13.0).width;
        let d = (w + 28.0).max(60.0);
        Size::new(d, d)
    }
    fn ports(&self, bounds: Rect, _props: &Props) -> Vec<Port> {
        compass_ports(bounds)
    }
    fn draw(
        &self,
        p: &mut Painter,
        bounds: Rect,
        props: &Props,
        style: &Style,
        _m: &dyn TextMeasurer,
    ) {
        let c = bounds.center();
        let r = bounds.width.min(bounds.height) / 2.0;
        p.circle(c, r, shape_style(props, style));
        centered_text(p, c, label_of(props), 13.0, text_color(props, style), style);
    }
}

// ---------------------------------------------------------------------------
// Oval (ellipse)
// ---------------------------------------------------------------------------

pub struct Oval;

/// A closed elliptical path inscribed in `bounds` (four cubic-Bézier quarters).
fn ellipse_path(bounds: Rect) -> Vec<PathCmd> {
    let c = bounds.center();
    let rx = bounds.width / 2.0;
    let ry = bounds.height / 2.0;
    let ox = rx * KAPPA;
    let oy = ry * KAPPA;
    let (cx, cy) = (c.x, c.y);
    vec![
        PathCmd::MoveTo(Point::new(cx + rx, cy)),
        PathCmd::CurveTo(
            Point::new(cx + rx, cy + oy),
            Point::new(cx + ox, cy + ry),
            Point::new(cx, cy + ry),
        ),
        PathCmd::CurveTo(
            Point::new(cx - ox, cy + ry),
            Point::new(cx - rx, cy + oy),
            Point::new(cx - rx, cy),
        ),
        PathCmd::CurveTo(
            Point::new(cx - rx, cy - oy),
            Point::new(cx - ox, cy - ry),
            Point::new(cx, cy - ry),
        ),
        PathCmd::CurveTo(
            Point::new(cx + ox, cy - ry),
            Point::new(cx + rx, cy - oy),
            Point::new(cx + rx, cy),
        ),
        PathCmd::Close,
    ]
}

impl Symbol for Oval {
    fn name(&self) -> &str {
        "oval"
    }
    fn description(&self) -> &str {
        "An oval / ellipse with a `label`, `fill`, and `text_color`."
    }
    fn property_schema(&self) -> Vec<PropertySpec> {
        color_props("")
    }
    fn measure(&self, props: &Props, m: &dyn TextMeasurer) -> Size {
        let w = m.measure_line(label_of(props), "sans-serif", 13.0).width;
        // Text needs more horizontal room inside an ellipse than a box.
        Size::new((w * 1.4 + 32.0).max(96.0), 56.0)
    }
    fn ports(&self, bounds: Rect, _props: &Props) -> Vec<Port> {
        compass_ports(bounds)
    }
    fn draw(
        &self,
        p: &mut Painter,
        bounds: Rect,
        props: &Props,
        style: &Style,
        _m: &dyn TextMeasurer,
    ) {
        p.path(ellipse_path(bounds), shape_style(props, style));
        centered_text(
            p,
            bounds.center(),
            label_of(props),
            13.0,
            text_color(props, style),
            style,
        );
    }
}

// ---------------------------------------------------------------------------
// Explosion (starburst callout)
// ---------------------------------------------------------------------------

pub struct Explosion;

impl Explosion {
    fn spike_count(props: &Props) -> usize {
        (props.number_or("spikes", 12.0).round() as i64).clamp(5, 60) as usize
    }

    /// Vertices of the starburst inscribed in `bounds`: outer points ride the bounding
    /// ellipse, inner points ride a smaller concentric ellipse.
    fn points(bounds: Rect, spikes: usize) -> Vec<Point> {
        let c = bounds.center();
        let rx = bounds.width / 2.0;
        let ry = bounds.height / 2.0;
        let inner = 0.62;
        let mut pts = Vec::with_capacity(spikes * 2);
        // Start at the top (-90°) so a spike points straight up.
        let start = -std::f64::consts::FRAC_PI_2;
        let step = std::f64::consts::PI / spikes as f64;
        for i in 0..(spikes * 2) {
            let ang = start + step * i as f64;
            let (rrx, rry) = if i % 2 == 0 {
                (rx, ry)
            } else {
                (rx * inner, ry * inner)
            };
            pts.push(Point::new(c.x + rrx * ang.cos(), c.y + rry * ang.sin()));
        }
        pts
    }
}

impl Symbol for Explosion {
    fn name(&self) -> &str {
        "explosion"
    }
    fn description(&self) -> &str {
        "A starburst / explosion callout with a `label`, `fill`, `text_color`, and `spikes` count."
    }
    fn property_schema(&self) -> Vec<PropertySpec> {
        color_props("explosion")
    }
    fn measure(&self, props: &Props, m: &dyn TextMeasurer) -> Size {
        let w = m.measure_line(label_of(props), "sans-serif", 12.0).width;
        // The label sits in the inner ~62%, so the overall shape must be larger.
        let d = (w / 0.62 + 40.0).max(90.0);
        Size::new(d, d)
    }
    fn ports(&self, bounds: Rect, _props: &Props) -> Vec<Port> {
        compass_ports(bounds)
    }
    fn draw(
        &self,
        p: &mut Painter,
        bounds: Rect,
        props: &Props,
        style: &Style,
        _m: &dyn TextMeasurer,
    ) {
        let pts = Self::points(bounds, Self::spike_count(props));
        p.path(polygon(&pts), shape_style(props, style));
        centered_text(
            p,
            bounds.center(),
            label_of(props),
            12.0,
            text_color(props, style),
            style,
        );
    }
}

// ---------------------------------------------------------------------------
// Arrow (block arrow)
// ---------------------------------------------------------------------------

pub struct Arrow;

impl Arrow {
    fn direction(props: &Props) -> Dir {
        match props.text_or("direction", "right") {
            "left" => Dir::Left,
            "up" => Dir::Up,
            "down" => Dir::Down,
            _ => Dir::Right,
        }
    }

    /// Map a normalized point in the unit square (right-pointing arrow) to `bounds`,
    /// rotating for the requested direction.
    fn place(bounds: Rect, dir: Dir, u: f64, v: f64) -> Point {
        let (nx, ny) = match dir {
            Dir::Right => (u, v),
            Dir::Left => (1.0 - u, v),
            // 90° clockwise: tip moves to the bottom.
            Dir::Down => (1.0 - v, u),
            // 90° counter-clockwise: tip moves to the top.
            Dir::Up => (v, 1.0 - u),
        };
        Point::new(bounds.x + nx * bounds.width, bounds.y + ny * bounds.height)
    }

    /// The seven vertices of a right-pointing block arrow, in normalized coordinates.
    const SHAPE: [(f64, f64); 7] = [
        (0.0, 0.30),
        (0.6, 0.30),
        (0.6, 0.05),
        (1.0, 0.5),
        (0.6, 0.95),
        (0.6, 0.70),
        (0.0, 0.70),
    ];

    fn outline(bounds: Rect, dir: Dir) -> Vec<Point> {
        Self::SHAPE
            .iter()
            .map(|&(u, v)| Self::place(bounds, dir, u, v))
            .collect()
    }
}

impl Symbol for Arrow {
    fn name(&self) -> &str {
        "arrow"
    }
    fn description(&self) -> &str {
        "A big block arrow pointing right/left/up/down (`direction`), with `label`, `fill`, `text_color`. Ports: tail `a`, tip `b`."
    }
    fn property_schema(&self) -> Vec<PropertySpec> {
        color_props("arrow")
    }
    fn measure(&self, props: &Props, m: &dyn TextMeasurer) -> Size {
        let w = m.measure_line(label_of(props), "sans-serif", 13.0).width;
        let long = (w + 70.0).max(120.0);
        match Self::direction(props) {
            Dir::Up | Dir::Down => Size::new(70.0, long),
            _ => Size::new(long, 64.0),
        }
    }
    fn ports(&self, bounds: Rect, props: &Props) -> Vec<Port> {
        let dir = Self::direction(props);
        // Tail rides the back edge (mid-shaft); tip is the arrowhead point.
        let tail = Self::place(bounds, dir, 0.0, 0.5);
        let tip = Self::place(bounds, dir, 1.0, 0.5);
        let (tail_dir, tip_dir) = match dir {
            Dir::Right => (Dir::Left, Dir::Right),
            Dir::Left => (Dir::Right, Dir::Left),
            Dir::Up => (Dir::Down, Dir::Up),
            Dir::Down => (Dir::Up, Dir::Down),
        };
        vec![Port::new("a", tail, tail_dir), Port::new("b", tip, tip_dir)]
    }
    fn draw(
        &self,
        p: &mut Painter,
        bounds: Rect,
        props: &Props,
        style: &Style,
        _m: &dyn TextMeasurer,
    ) {
        let dir = Self::direction(props);
        p.path(
            polygon(&Self::outline(bounds, dir)),
            shape_style(props, style),
        );
        // Center the label on the shaft (just behind the head for horizontal arrows).
        let label_at = match dir {
            Dir::Right | Dir::Left => Self::place(bounds, dir, 0.3, 0.5),
            Dir::Up | Dir::Down => Self::place(bounds, dir, 0.3, 0.5),
        };
        centered_text(
            p,
            label_at,
            label_of(props),
            13.0,
            text_color(props, style),
            style,
        );
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
        r.register(&Shapes);
        r
    }

    fn props(pairs: &[(&str, PropValue)]) -> Props {
        let mut p = Props::new();
        for (k, v) in pairs {
            p.insert(*k, v.clone());
        }
        p
    }

    #[test]
    fn registers_all_symbols() {
        let reg = registry();
        for n in [
            "shapes.rectangle",
            "shapes.circle",
            "shapes.oval",
            "shapes.explosion",
            "shapes.arrow",
        ] {
            assert!(reg.get(n).is_some(), "missing {n}");
        }
    }

    #[test]
    fn rectangle_fills_with_color_and_compass_ports() {
        let r = Rectangle;
        let p0 = props(&[
            ("label", PropValue::text("Box")),
            ("fill", PropValue::text("blue")),
            ("text_color", PropValue::text("white")),
        ]);
        let m = BasicMeasurer::default();
        let bounds = Rect::from_origin_size(Point::ZERO, r.measure(&p0, &m));

        let ports = r.ports(bounds, &p0);
        assert_eq!(
            ports.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            vec!["n", "e", "s", "w"]
        );

        let mut p = Painter::new();
        r.draw(&mut p, bounds, &p0, &Style::default(), &m);
        // The rect is filled blue and the label is drawn white.
        let blue = Color::parse("blue").unwrap();
        assert!(p
            .primitives()
            .iter()
            .any(|x| matches!(x, Primitive::Rect { style, .. } if style.fill == blue)));
        assert!(p
            .primitives()
            .iter()
            .any(|x| matches!(x, Primitive::Text { text, style, .. }
                if text == "Box" && style.color == Color::WHITE)));
    }

    #[test]
    fn fill_defaults_to_white_and_bad_color_falls_back() {
        let r = Rectangle;
        let m = BasicMeasurer::default();
        let bounds = Rect::new(0.0, 0.0, 80.0, 44.0);

        // No fill property -> white.
        let mut p = Painter::new();
        r.draw(&mut p, bounds, &Props::new(), &Style::default(), &m);
        assert!(p
            .primitives()
            .iter()
            .any(|x| matches!(x, Primitive::Rect { style, .. } if style.fill == Color::WHITE)));

        // Garbage color also falls back to white (default style fill is transparent).
        let p1 = props(&[("fill", PropValue::text("not-a-color"))]);
        let mut p = Painter::new();
        r.draw(&mut p, bounds, &p1, &Style::default(), &m);
        assert!(p
            .primitives()
            .iter()
            .any(|x| matches!(x, Primitive::Rect { style, .. } if style.fill == Color::WHITE)));
    }

    #[test]
    fn circle_grows_with_label_and_draws_circle() {
        let c = Circle;
        let m = BasicMeasurer::default();
        let small = c.measure(&props(&[("label", PropValue::text("x"))]), &m);
        let big = c.measure(
            &props(&[("label", PropValue::text("a long circle label"))]),
            &m,
        );
        assert!(big.width > small.width);
        assert_eq!(small.width, small.height, "circle is square");

        let bounds = Rect::from_origin_size(Point::ZERO, small);
        let mut p = Painter::new();
        c.draw(&mut p, bounds, &Props::new(), &Style::default(), &m);
        assert!(p
            .primitives()
            .iter()
            .any(|x| matches!(x, Primitive::Circle { .. })));
    }

    #[test]
    fn oval_draws_closed_curved_path() {
        let o = Oval;
        let m = BasicMeasurer::default();
        let bounds = Rect::new(0.0, 0.0, 120.0, 60.0);
        let mut p = Painter::new();
        o.draw(
            &mut p,
            bounds,
            &props(&[("label", PropValue::text("oval"))]),
            &Style::default(),
            &m,
        );
        // An ellipse is a path with cubic curves, closed.
        let path = p
            .primitives()
            .iter()
            .find_map(|x| match x {
                Primitive::Path { cmds, .. } => Some(cmds.clone()),
                _ => None,
            })
            .expect("oval should emit a path");
        assert!(path.iter().any(|c| matches!(c, PathCmd::CurveTo(..))));
        assert!(matches!(path.last(), Some(PathCmd::Close)));
    }

    #[test]
    fn explosion_is_a_starburst_polygon() {
        let e = Explosion;
        let m = BasicMeasurer::default();
        let p0 = props(&[
            ("label", PropValue::text("NEW!")),
            ("spikes", PropValue::Number(10.0)),
        ]);
        let bounds = Rect::from_origin_size(Point::ZERO, e.measure(&p0, &m));
        let mut p = Painter::new();
        e.draw(&mut p, bounds, &p0, &Style::default(), &m);
        let line_to = p
            .primitives()
            .iter()
            .find_map(|x| match x {
                Primitive::Path { cmds, .. } => Some(
                    cmds.iter()
                        .filter(|c| matches!(c, PathCmd::LineTo(..)))
                        .count(),
                ),
                _ => None,
            })
            .expect("explosion should emit a path");
        // 10 spikes -> 20 vertices -> 1 MoveTo + 19 LineTo.
        assert_eq!(line_to, 19);
        assert!(p
            .primitives()
            .iter()
            .any(|x| matches!(x, Primitive::Text { text, .. } if text == "NEW!")));
    }

    #[test]
    fn arrow_tail_and_tip_follow_direction() {
        let a = Arrow;
        let m = BasicMeasurer::default();

        // Right arrow: tail on the left edge, tip on the right edge.
        let right = props(&[("direction", PropValue::text("right"))]);
        let bounds = Rect::new(0.0, 0.0, 120.0, 64.0);
        let ports = a.ports(bounds, &right);
        let tail = ports.iter().find(|p| p.name == "a").unwrap();
        let tip = ports.iter().find(|p| p.name == "b").unwrap();
        assert!((tail.point.x - bounds.x).abs() < 1e-9);
        assert_eq!(tail.dir, Dir::Left);
        assert!((tip.point.x - bounds.right()).abs() < 1e-9);
        assert_eq!(tip.dir, Dir::Right);

        // Up arrow measures taller than wide and tips upward.
        let up = props(&[("direction", PropValue::text("up"))]);
        let size = a.measure(&up, &m);
        assert!(size.height > size.width);
        let ub = Rect::from_origin_size(Point::ZERO, size);
        let tip = a
            .ports(ub, &up)
            .into_iter()
            .find(|p| p.name == "b")
            .unwrap();
        assert_eq!(tip.dir, Dir::Up);
        assert!((tip.point.y - ub.y).abs() < 1e-9);

        // Drawing emits a filled polygon path.
        let mut p = Painter::new();
        a.draw(&mut p, bounds, &right, &Style::default(), &m);
        assert!(p
            .primitives()
            .iter()
            .any(|x| matches!(x, Primitive::Path { .. })));
    }
}
