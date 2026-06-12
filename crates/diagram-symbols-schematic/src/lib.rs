//! Schematic (electronic) symbols for diagram.
//!
//! Provides the `schematic` plugin with common two-terminal components (resistor, capacitor,
//! inductor, diode, voltage source, switch), a ground symbol, a wire junction, and a generic
//! [`ic`](Ic) with named pins. Two-terminal parts are drawn horizontally with connection
//! ports `a` (left) and `b` (right); the IC exposes one port per named pin.

use diagram_core::draw::{Painter, PathCmd, ShapeStyle, Stroke};
use diagram_core::geom::{Point, Rect, Size};
use diagram_core::measure::TextMeasurer;
use diagram_core::style::{Style, TextAnchor};
use diagram_core::symbols::{
    Dir, Port, PropKind, PropValue, PropertySpec, Props, Symbol, SymbolPlugin,
};

/// The schematic symbol plugin.
pub struct Schematic;

impl SymbolPlugin for Schematic {
    fn id(&self) -> &str {
        "schematic"
    }

    fn description(&self) -> &str {
        "Common electronic schematic symbols (resistor, capacitor, IC, ...)."
    }

    fn symbols(&self) -> Vec<Box<dyn Symbol>> {
        vec![
            Box::new(Resistor),
            Box::new(Capacitor),
            Box::new(Inductor),
            Box::new(Diode),
            Box::new(VoltageSource),
            Box::new(Switch),
            Box::new(Ground),
            Box::new(Junction),
            Box::new(Ic),
        ]
    }
}

// ---------------------------------------------------------------------------
// Shared geometry / helpers for two-terminal components
// ---------------------------------------------------------------------------

const LEAD: f64 = 10.0;
const BODY_H: f64 = 24.0;
const LABEL_H: f64 = 14.0;
const DEFAULT_W: f64 = 64.0;

fn stroke(style: &Style) -> Stroke {
    Stroke::new(style.stroke, style.stroke_width.max(1.0))
}

fn outline(style: &Style) -> ShapeStyle {
    ShapeStyle::outline(style.stroke, style.stroke_width.max(1.0))
}

fn filled(style: &Style) -> ShapeStyle {
    ShapeStyle::new(style.stroke, style.stroke, style.stroke_width.max(1.0))
}

/// Whether a two-terminal component has a value/label to display above it.
fn value_label(props: &Props, key: &str, unit: &str) -> Option<String> {
    let explicit = props.text_or("label", "");
    if !explicit.is_empty() {
        return Some(explicit.to_string());
    }
    match props.get(key) {
        Some(PropValue::Number(n)) => {
            Some(format!("{}{}", diagram_core::expr::format_num(*n), unit))
        }
        Some(PropValue::Text(s)) if !s.is_empty() => Some(format!("{s}{unit}")),
        _ => None,
    }
}

/// Default intrinsic size of a two-terminal component, given whether it shows a label.
fn tt_size(has_label: bool) -> Size {
    Size::new(DEFAULT_W, BODY_H + if has_label { LABEL_H } else { 0.0 })
}

/// The vertical centerline (lead height) for a two-terminal component within `bounds`.
fn tt_centerline(bounds: Rect, has_label: bool) -> f64 {
    let label_area = if has_label { LABEL_H } else { 0.0 };
    bounds.y + label_area + (bounds.height - label_area) / 2.0
}

fn tt_ports(bounds: Rect, has_label: bool) -> Vec<Port> {
    let cy = tt_centerline(bounds, has_label);
    vec![
        Port::new("a", Point::new(bounds.x, cy), Dir::Left),
        Port::new("b", Point::new(bounds.right(), cy), Dir::Right),
    ]
}

/// Draw the lead wires from the bounds edges to the body's left/right extents at `cy`.
fn draw_leads(p: &mut Painter, bounds: Rect, body_l: f64, body_r: f64, cy: f64, style: &Style) {
    p.line(
        Point::new(bounds.x, cy),
        Point::new(body_l, cy),
        stroke(style),
    );
    p.line(
        Point::new(body_r, cy),
        Point::new(bounds.right(), cy),
        stroke(style),
    );
}

/// Draw a centered label across the top of the component (if present).
fn draw_label(p: &mut Painter, bounds: Rect, text: &str, style: &Style) {
    p.text(
        Point::new(bounds.center().x, bounds.y + LABEL_H - 3.0),
        text,
        diagram_core::draw::TextStyle {
            color: style.text_color,
            font_family: style.font_family.clone(),
            font_size: style.font_size.min(LABEL_H),
            anchor: TextAnchor::Middle,
        },
    );
}

/// The horizontal body extent (inset from the leads) for a two-terminal component.
fn body_extent(bounds: Rect) -> (f64, f64) {
    (bounds.x + LEAD, bounds.right() - LEAD)
}

// ---------------------------------------------------------------------------
// Resistor
// ---------------------------------------------------------------------------

pub struct Resistor;

impl Symbol for Resistor {
    fn name(&self) -> &str {
        "resistor"
    }
    fn description(&self) -> &str {
        "Resistor (ANSI zig-zag). `ohms` is drawn as the value label."
    }
    fn property_schema(&self) -> Vec<PropertySpec> {
        vec![
            PropertySpec::optional(
                "ohms",
                PropKind::Text,
                PropValue::text(""),
                "Resistance value, shown as the label (e.g. 220, 4.7k).",
            ),
            PropertySpec::optional(
                "label",
                PropKind::Text,
                PropValue::text(""),
                "Override label text (used instead of ohms).",
            ),
        ]
    }
    fn measure(&self, props: &Props, _m: &dyn TextMeasurer) -> Size {
        tt_size(value_label(props, "ohms", "\u{2126}").is_some())
    }
    fn ports(&self, bounds: Rect, props: &Props) -> Vec<Port> {
        tt_ports(bounds, value_label(props, "ohms", "\u{2126}").is_some())
    }
    fn draw(
        &self,
        p: &mut Painter,
        bounds: Rect,
        props: &Props,
        style: &Style,
        _m: &dyn TextMeasurer,
    ) {
        let label = value_label(props, "ohms", "\u{2126}");
        let cy = tt_centerline(bounds, label.is_some());
        let (bl, br) = body_extent(bounds);
        draw_leads(p, bounds, bl, br, cy, style);
        // Zig-zag across [bl, br].
        let segments = 6;
        let amp = 8.0;
        let mut pts = vec![Point::new(bl, cy)];
        for i in 0..segments {
            let x = bl + (br - bl) * (i as f64 + 0.5) / segments as f64;
            let y = if i % 2 == 0 { cy - amp } else { cy + amp };
            pts.push(Point::new(x, y));
        }
        pts.push(Point::new(br, cy));
        p.polyline(pts, stroke(style));
        if let Some(l) = label {
            draw_label(p, bounds, &l, style);
        }
    }
}

// ---------------------------------------------------------------------------
// Capacitor
// ---------------------------------------------------------------------------

pub struct Capacitor;

impl Symbol for Capacitor {
    fn name(&self) -> &str {
        "capacitor"
    }
    fn description(&self) -> &str {
        "Capacitor (two parallel plates). `farads` is drawn as the value label."
    }
    fn property_schema(&self) -> Vec<PropertySpec> {
        vec![
            PropertySpec::optional(
                "farads",
                PropKind::Text,
                PropValue::text(""),
                "Capacitance value, shown as the label (e.g. 100n).",
            ),
            PropertySpec::optional(
                "label",
                PropKind::Text,
                PropValue::text(""),
                "Override label text.",
            ),
        ]
    }
    fn measure(&self, props: &Props, _m: &dyn TextMeasurer) -> Size {
        tt_size(value_label(props, "farads", "F").is_some())
    }
    fn ports(&self, bounds: Rect, props: &Props) -> Vec<Port> {
        tt_ports(bounds, value_label(props, "farads", "F").is_some())
    }
    fn draw(
        &self,
        p: &mut Painter,
        bounds: Rect,
        props: &Props,
        style: &Style,
        _m: &dyn TextMeasurer,
    ) {
        let label = value_label(props, "farads", "F");
        let cy = tt_centerline(bounds, label.is_some());
        let gap = 6.0;
        let mid = bounds.center().x;
        let plate_l = mid - gap / 2.0;
        let plate_r = mid + gap / 2.0;
        draw_leads(p, bounds, plate_l, plate_r, cy, style);
        let plate_h = 16.0;
        p.line(
            Point::new(plate_l, cy - plate_h / 2.0),
            Point::new(plate_l, cy + plate_h / 2.0),
            stroke(style),
        );
        p.line(
            Point::new(plate_r, cy - plate_h / 2.0),
            Point::new(plate_r, cy + plate_h / 2.0),
            stroke(style),
        );
        if let Some(l) = label {
            draw_label(p, bounds, &l, style);
        }
    }
}

// ---------------------------------------------------------------------------
// Inductor
// ---------------------------------------------------------------------------

pub struct Inductor;

impl Symbol for Inductor {
    fn name(&self) -> &str {
        "inductor"
    }
    fn description(&self) -> &str {
        "Inductor (series of humps). `henries` is drawn as the value label."
    }
    fn property_schema(&self) -> Vec<PropertySpec> {
        vec![
            PropertySpec::optional(
                "henries",
                PropKind::Text,
                PropValue::text(""),
                "Inductance value, shown as the label.",
            ),
            PropertySpec::optional(
                "label",
                PropKind::Text,
                PropValue::text(""),
                "Override label text.",
            ),
        ]
    }
    fn measure(&self, props: &Props, _m: &dyn TextMeasurer) -> Size {
        tt_size(value_label(props, "henries", "H").is_some())
    }
    fn ports(&self, bounds: Rect, props: &Props) -> Vec<Port> {
        tt_ports(bounds, value_label(props, "henries", "H").is_some())
    }
    fn draw(
        &self,
        p: &mut Painter,
        bounds: Rect,
        props: &Props,
        style: &Style,
        _m: &dyn TextMeasurer,
    ) {
        let label = value_label(props, "henries", "H");
        let cy = tt_centerline(bounds, label.is_some());
        let (bl, br) = body_extent(bounds);
        draw_leads(p, bounds, bl, br, cy, style);
        // Four semicircular humps as quadratic-ish cubic arcs.
        let humps = 4;
        let w = (br - bl) / humps as f64;
        let r = w / 2.0;
        let mut cmds = vec![PathCmd::MoveTo(Point::new(bl, cy))];
        for i in 0..humps {
            let x0 = bl + i as f64 * w;
            let x1 = x0 + w;
            // Cubic approximating a semicircle bump upward.
            let k = r * 1.333;
            cmds.push(PathCmd::CurveTo(
                Point::new(x0, cy - k),
                Point::new(x1, cy - k),
                Point::new(x1, cy),
            ));
        }
        p.path(
            cmds,
            ShapeStyle::outline(style.stroke, style.stroke_width.max(1.0)),
        );
        if let Some(l) = label {
            draw_label(p, bounds, &l, style);
        }
    }
}

// ---------------------------------------------------------------------------
// Diode
// ---------------------------------------------------------------------------

pub struct Diode;

impl Symbol for Diode {
    fn name(&self) -> &str {
        "diode"
    }
    fn description(&self) -> &str {
        "Diode (triangle + cathode bar), anode `a` to cathode `b`."
    }
    fn property_schema(&self) -> Vec<PropertySpec> {
        vec![PropertySpec::optional(
            "label",
            PropKind::Text,
            PropValue::text(""),
            "Optional label.",
        )]
    }
    fn measure(&self, props: &Props, _m: &dyn TextMeasurer) -> Size {
        tt_size(!props.text_or("label", "").is_empty())
    }
    fn ports(&self, bounds: Rect, props: &Props) -> Vec<Port> {
        tt_ports(bounds, !props.text_or("label", "").is_empty())
    }
    fn draw(
        &self,
        p: &mut Painter,
        bounds: Rect,
        props: &Props,
        style: &Style,
        _m: &dyn TextMeasurer,
    ) {
        let has_label = !props.text_or("label", "").is_empty();
        let cy = tt_centerline(bounds, has_label);
        let mid = bounds.center().x;
        let half = 9.0;
        let tri_l = mid - half;
        let tri_r = mid + half;
        draw_leads(p, bounds, tri_l, tri_r, cy, style);
        // Triangle pointing right.
        p.path(
            vec![
                PathCmd::MoveTo(Point::new(tri_l, cy - half)),
                PathCmd::LineTo(Point::new(tri_l, cy + half)),
                PathCmd::LineTo(Point::new(tri_r, cy)),
                PathCmd::Close,
            ],
            filled(style),
        );
        // Cathode bar.
        p.line(
            Point::new(tri_r, cy - half),
            Point::new(tri_r, cy + half),
            stroke(style),
        );
        if has_label {
            draw_label(p, bounds, props.text_or("label", ""), style);
        }
    }
}

// ---------------------------------------------------------------------------
// Voltage source
// ---------------------------------------------------------------------------

pub struct VoltageSource;

impl Symbol for VoltageSource {
    fn name(&self) -> &str {
        "vsource"
    }
    fn description(&self) -> &str {
        "Voltage source (circle with + / -). `volts` is drawn as the value label."
    }
    fn property_schema(&self) -> Vec<PropertySpec> {
        vec![
            PropertySpec::optional(
                "volts",
                PropKind::Text,
                PropValue::text(""),
                "Voltage value, shown as the label.",
            ),
            PropertySpec::optional(
                "label",
                PropKind::Text,
                PropValue::text(""),
                "Override label text.",
            ),
        ]
    }
    fn measure(&self, props: &Props, _m: &dyn TextMeasurer) -> Size {
        let has_label = value_label(props, "volts", "V").is_some();
        Size::new(DEFAULT_W, 34.0 + if has_label { LABEL_H } else { 0.0 })
    }
    fn ports(&self, bounds: Rect, props: &Props) -> Vec<Port> {
        tt_ports(bounds, value_label(props, "volts", "V").is_some())
    }
    fn draw(
        &self,
        p: &mut Painter,
        bounds: Rect,
        props: &Props,
        style: &Style,
        _m: &dyn TextMeasurer,
    ) {
        let label = value_label(props, "volts", "V");
        let cy = tt_centerline(bounds, label.is_some());
        let mid = bounds.center().x;
        let r = 15.0;
        draw_leads(p, bounds, mid - r, mid + r, cy, style);
        p.circle(Point::new(mid, cy), r, outline(style));
        // + on the left half, - on the right half.
        let s = 4.0;
        p.line(
            Point::new(mid - r * 0.45 - s, cy),
            Point::new(mid - r * 0.45 + s, cy),
            stroke(style),
        );
        p.line(
            Point::new(mid - r * 0.45, cy - s),
            Point::new(mid - r * 0.45, cy + s),
            stroke(style),
        );
        p.line(
            Point::new(mid + r * 0.45 - s, cy),
            Point::new(mid + r * 0.45 + s, cy),
            stroke(style),
        );
        if let Some(l) = label {
            draw_label(p, bounds, &l, style);
        }
    }
}

// ---------------------------------------------------------------------------
// Switch
// ---------------------------------------------------------------------------

pub struct Switch;

impl Symbol for Switch {
    fn name(&self) -> &str {
        "switch"
    }
    fn description(&self) -> &str {
        "SPST switch (open). `label` is drawn above."
    }
    fn property_schema(&self) -> Vec<PropertySpec> {
        vec![PropertySpec::optional(
            "label",
            PropKind::Text,
            PropValue::text(""),
            "Optional label.",
        )]
    }
    fn measure(&self, props: &Props, _m: &dyn TextMeasurer) -> Size {
        tt_size(!props.text_or("label", "").is_empty())
    }
    fn ports(&self, bounds: Rect, props: &Props) -> Vec<Port> {
        tt_ports(bounds, !props.text_or("label", "").is_empty())
    }
    fn draw(
        &self,
        p: &mut Painter,
        bounds: Rect,
        props: &Props,
        style: &Style,
        _m: &dyn TextMeasurer,
    ) {
        let has_label = !props.text_or("label", "").is_empty();
        let cy = tt_centerline(bounds, has_label);
        let (bl, br) = body_extent(bounds);
        // Leads to the two contact dots.
        p.line(Point::new(bounds.x, cy), Point::new(bl, cy), stroke(style));
        p.line(
            Point::new(br, cy),
            Point::new(bounds.right(), cy),
            stroke(style),
        );
        p.circle(Point::new(bl, cy), 1.6, filled(style));
        p.circle(Point::new(br, cy), 1.6, filled(style));
        // Open lever from the left dot, angled up to near the right dot.
        p.line(
            Point::new(bl, cy),
            Point::new(br - 2.0, cy - 10.0),
            stroke(style),
        );
        if has_label {
            draw_label(p, bounds, props.text_or("label", ""), style);
        }
    }
}

// ---------------------------------------------------------------------------
// Ground
// ---------------------------------------------------------------------------

pub struct Ground;

impl Symbol for Ground {
    fn name(&self) -> &str {
        "ground"
    }
    fn description(&self) -> &str {
        "Ground symbol; single port `a` at the top."
    }
    fn measure(&self, _props: &Props, _m: &dyn TextMeasurer) -> Size {
        Size::new(28.0, 26.0)
    }
    fn ports(&self, bounds: Rect, _props: &Props) -> Vec<Port> {
        vec![Port::new(
            "a",
            Point::new(bounds.center().x, bounds.y),
            Dir::Up,
        )]
    }
    fn draw(
        &self,
        p: &mut Painter,
        bounds: Rect,
        _props: &Props,
        style: &Style,
        _m: &dyn TextMeasurer,
    ) {
        let cx = bounds.center().x;
        let top = bounds.y;
        // Vertical lead then three shrinking horizontal bars.
        let bar_y = top + 10.0;
        p.line(Point::new(cx, top), Point::new(cx, bar_y), stroke(style));
        let widths = [14.0, 9.0, 4.0];
        for (i, w) in widths.iter().enumerate() {
            let y = bar_y + i as f64 * 5.0;
            p.line(Point::new(cx - w, y), Point::new(cx + w, y), stroke(style));
        }
    }
}

// ---------------------------------------------------------------------------
// Junction (wire dot)
// ---------------------------------------------------------------------------

pub struct Junction;

impl Symbol for Junction {
    fn name(&self) -> &str {
        "junction"
    }
    fn description(&self) -> &str {
        "A filled wire junction dot; port `c` at its center."
    }
    fn measure(&self, _props: &Props, _m: &dyn TextMeasurer) -> Size {
        Size::new(8.0, 8.0)
    }
    fn ports(&self, bounds: Rect, _props: &Props) -> Vec<Port> {
        vec![Port::new("c", bounds.center(), Dir::Right)]
    }
    fn draw(
        &self,
        p: &mut Painter,
        bounds: Rect,
        _props: &Props,
        style: &Style,
        _m: &dyn TextMeasurer,
    ) {
        p.circle(bounds.center(), 3.0, filled(style));
    }
}

// ---------------------------------------------------------------------------
// Generic IC with named pins
// ---------------------------------------------------------------------------

const IC_LEAD: f64 = 12.0;
const IC_PIN_SPACING: f64 = 22.0;
const IC_LABEL_PAD: f64 = 8.0;

pub struct Ic;

struct PinPlacement {
    name: String,
    tip: Point,
    label_pos: Point,
    anchor: TextAnchor,
    dir: Dir,
    body_anchor: Point,
}

impl Ic {
    fn pins(props: &Props, side: &str) -> Vec<String> {
        props.text_list(side)
    }

    /// Compute the body rectangle within `bounds`, reserving lead space on sides that have
    /// pins (left/right always reserve, since horizontal pins are the common case).
    fn body_rect(bounds: Rect, has_top: bool, has_bottom: bool) -> Rect {
        let top = if has_top { IC_LEAD } else { 0.0 };
        let bottom = if has_bottom { IC_LEAD } else { 0.0 };
        Rect::new(
            bounds.x + IC_LEAD,
            bounds.y + top,
            (bounds.width - 2.0 * IC_LEAD).max(1.0),
            (bounds.height - top - bottom).max(1.0),
        )
    }

    fn placements(bounds: Rect, props: &Props) -> Vec<PinPlacement> {
        let left = Self::pins(props, "left_pins");
        let right = Self::pins(props, "right_pins");
        let top = Self::pins(props, "top_pins");
        let bottom = Self::pins(props, "bottom_pins");
        let body = Self::body_rect(bounds, !top.is_empty(), !bottom.is_empty());
        let mut out = Vec::new();

        for (i, name) in left.iter().enumerate() {
            let y = body.y + body.height * (i as f64 + 1.0) / (left.len() as f64 + 1.0);
            out.push(PinPlacement {
                name: name.clone(),
                tip: Point::new(bounds.x, y),
                label_pos: Point::new(body.x + IC_LABEL_PAD, y + 4.0),
                anchor: TextAnchor::Start,
                dir: Dir::Left,
                body_anchor: Point::new(body.x, y),
            });
        }
        for (i, name) in right.iter().enumerate() {
            let y = body.y + body.height * (i as f64 + 1.0) / (right.len() as f64 + 1.0);
            out.push(PinPlacement {
                name: name.clone(),
                tip: Point::new(bounds.right(), y),
                label_pos: Point::new(body.right() - IC_LABEL_PAD, y + 4.0),
                anchor: TextAnchor::End,
                dir: Dir::Right,
                body_anchor: Point::new(body.right(), y),
            });
        }
        for (i, name) in top.iter().enumerate() {
            let x = body.x + body.width * (i as f64 + 1.0) / (top.len() as f64 + 1.0);
            out.push(PinPlacement {
                name: name.clone(),
                tip: Point::new(x, bounds.y),
                label_pos: Point::new(x, body.y + IC_LABEL_PAD + 4.0),
                anchor: TextAnchor::Middle,
                dir: Dir::Up,
                body_anchor: Point::new(x, body.y),
            });
        }
        for (i, name) in bottom.iter().enumerate() {
            let x = body.x + body.width * (i as f64 + 1.0) / (bottom.len() as f64 + 1.0);
            out.push(PinPlacement {
                name: name.clone(),
                tip: Point::new(x, bounds.bottom()),
                label_pos: Point::new(x, body.bottom() - IC_LABEL_PAD),
                anchor: TextAnchor::Middle,
                dir: Dir::Down,
                body_anchor: Point::new(x, body.bottom()),
            });
        }
        out
    }
}

impl Symbol for Ic {
    fn name(&self) -> &str {
        "ic"
    }
    fn description(&self) -> &str {
        "Generic IC / chip with a `name` and named pins on each side (left_pins, right_pins, top_pins, bottom_pins)."
    }
    fn property_schema(&self) -> Vec<PropertySpec> {
        vec![
            PropertySpec::optional(
                "name",
                PropKind::Text,
                PropValue::text("IC"),
                "Chip / module name shown in the center.",
            ),
            PropertySpec::optional(
                "left_pins",
                PropKind::List,
                PropValue::List(vec![]),
                "Pin names down the left side.",
            ),
            PropertySpec::optional(
                "right_pins",
                PropKind::List,
                PropValue::List(vec![]),
                "Pin names down the right side.",
            ),
            PropertySpec::optional(
                "top_pins",
                PropKind::List,
                PropValue::List(vec![]),
                "Pin names along the top.",
            ),
            PropertySpec::optional(
                "bottom_pins",
                PropKind::List,
                PropValue::List(vec![]),
                "Pin names along the bottom.",
            ),
        ]
    }

    fn measure(&self, props: &Props, m: &dyn TextMeasurer) -> Size {
        let left = Self::pins(props, "left_pins");
        let right = Self::pins(props, "right_pins");
        let top = Self::pins(props, "top_pins");
        let bottom = Self::pins(props, "bottom_pins");
        let name = props.text_or("name", "IC");

        let max_label_w = |names: &[String]| {
            names
                .iter()
                .map(|n| m.measure_line(n, "sans-serif", 11.0).width)
                .fold(0.0_f64, f64::max)
        };
        let left_w = max_label_w(&left);
        let right_w = max_label_w(&right);
        let top_w = max_label_w(&top);
        let bottom_w = max_label_w(&bottom);
        let name_w = m.measure_line(name, "sans-serif", 13.0).width;
        let rows = left.len().max(right.len()).max(1);

        // Width must fit: left labels + centered name + right labels (with clearance), and
        // enough column spacing that top/bottom pin labels don't overlap.
        let horizontal_need = left_w + right_w + name_w + 36.0;
        let top_need = (top.len() as f64 + 1.0) * (top_w + 10.0);
        let bottom_need = (bottom.len() as f64 + 1.0) * (bottom_w + 10.0);
        let inner_w = horizontal_need
            .max(top_need)
            .max(bottom_need)
            .max(name_w + 16.0)
            .max(48.0);
        let inner_h = (rows as f64 * IC_PIN_SPACING).max(40.0);

        let top_lead = if top.is_empty() { 0.0 } else { IC_LEAD };
        let bottom_lead = if bottom.is_empty() { 0.0 } else { IC_LEAD };
        Size::new(inner_w + 2.0 * IC_LEAD, inner_h + top_lead + bottom_lead)
    }

    fn ports(&self, bounds: Rect, props: &Props) -> Vec<Port> {
        Self::placements(bounds, props)
            .into_iter()
            .map(|p| Port::new(p.name, p.tip, p.dir))
            .collect()
    }

    fn draw(
        &self,
        p: &mut Painter,
        bounds: Rect,
        props: &Props,
        style: &Style,
        _m: &dyn TextMeasurer,
    ) {
        let has_top = !Self::pins(props, "top_pins").is_empty();
        let has_bottom = !Self::pins(props, "bottom_pins").is_empty();
        let body = Self::body_rect(bounds, has_top, has_bottom);
        p.rect(body, 3.0, outline(style));

        // Chip name centered.
        p.text(
            body.center(),
            props.text_or("name", "IC"),
            diagram_core::draw::TextStyle {
                color: style.text_color,
                font_family: style.font_family.clone(),
                font_size: 13.0,
                anchor: TextAnchor::Middle,
            },
        );

        for pin in Self::placements(bounds, props) {
            // Lead from the body edge to the pin tip.
            p.line(pin.body_anchor, pin.tip, stroke(style));
            p.text(
                pin.label_pos,
                &pin.name,
                diagram_core::draw::TextStyle {
                    color: style.text_color,
                    font_family: style.font_family.clone(),
                    font_size: 11.0,
                    anchor: pin.anchor,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diagram_core::draw::Primitive;
    use diagram_core::measure::BasicMeasurer;
    use diagram_core::symbols::Registry;

    fn registry() -> Registry {
        let mut r = Registry::new();
        r.register(&Schematic);
        r
    }

    #[test]
    fn plugin_registers_all_symbols() {
        let reg = registry();
        for name in [
            "schematic.resistor",
            "schematic.capacitor",
            "schematic.inductor",
            "schematic.diode",
            "schematic.vsource",
            "schematic.switch",
            "schematic.ground",
            "schematic.junction",
            "schematic.ic",
        ] {
            assert!(reg.get(name).is_some(), "missing {name}");
        }
    }

    #[test]
    fn resistor_has_two_ports_and_draws() {
        let reg = registry();
        let r = reg.get("schematic.resistor").unwrap();
        let mut props = Props::new();
        props.insert("ohms", PropValue::Number(220.0));
        let props = props.with_defaults(&r.property_schema());
        let bounds = Rect::new(0.0, 0.0, 64.0, 38.0);
        let ports = r.ports(bounds, &props);
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].name, "a");
        assert_eq!(ports[1].name, "b");

        let m = BasicMeasurer::default();
        let mut p = Painter::new();
        r.draw(&mut p, bounds, &props, &Style::default(), &m);
        // Expect a zig-zag polyline and the value label.
        assert!(p
            .primitives()
            .iter()
            .any(|x| matches!(x, Primitive::Polyline { .. })));
        assert!(p
            .primitives()
            .iter()
            .any(|x| matches!(x, Primitive::Text { text, .. } if text.contains("220"))));
    }

    #[test]
    fn resistor_label_changes_height() {
        let r = Resistor;
        let m = BasicMeasurer::default();
        let with = r.measure(
            &{
                let mut p = Props::new();
                p.insert("ohms", PropValue::Number(1.0));
                p
            },
            &m,
        );
        let without = r.measure(&Props::new(), &m);
        assert!(with.height > without.height);
    }

    #[test]
    fn ic_pins_become_ports_with_names() {
        let ic = Ic;
        let mut props = Props::new();
        props.insert("name", PropValue::text("MCU"));
        props.insert(
            "left_pins",
            PropValue::List(vec![PropValue::text("VCC"), PropValue::text("GND")]),
        );
        props.insert("right_pins", PropValue::List(vec![PropValue::text("CLK")]));
        let m = BasicMeasurer::default();
        let size = ic.measure(&props, &m);
        let bounds = Rect::from_origin_size(Point::ZERO, size);
        let ports = ic.ports(bounds, &props);
        let names: Vec<&str> = ports.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"VCC"));
        assert!(names.contains(&"GND"));
        assert!(names.contains(&"CLK"));
        assert_eq!(ports.len(), 3);

        // VCC (left) faces left; CLK (right) faces right.
        let vcc = ports.iter().find(|p| p.name == "VCC").unwrap();
        assert_eq!(vcc.dir, Dir::Left);
        assert!((vcc.point.x - bounds.x).abs() < 1e-9);

        // Drawing emits the chip name and each pin label.
        let mut p = Painter::new();
        ic.draw(&mut p, bounds, &props, &Style::default(), &m);
        let texts: Vec<&str> = p
            .primitives()
            .iter()
            .filter_map(|x| match x {
                Primitive::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(texts.contains(&"MCU"));
        assert!(texts.contains(&"VCC"));
        assert!(texts.contains(&"CLK"));
    }

    #[test]
    fn ic_autosizes_to_more_pins() {
        let ic = Ic;
        let m = BasicMeasurer::default();
        let few = {
            let mut p = Props::new();
            p.insert("left_pins", PropValue::List(vec![PropValue::text("A")]));
            ic.measure(&p, &m)
        };
        let many = {
            let mut p = Props::new();
            p.insert(
                "left_pins",
                PropValue::List((0..6).map(|i| PropValue::text(format!("P{i}"))).collect()),
            );
            ic.measure(&p, &m)
        };
        assert!(many.height > few.height, "more pins should be taller");
    }

    #[test]
    fn ground_has_single_top_port() {
        let g = Ground;
        let bounds = Rect::new(0.0, 0.0, 28.0, 26.0);
        let ports = g.ports(bounds, &Props::new());
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].name, "a");
        assert_eq!(ports[0].dir, Dir::Up);
    }
}
