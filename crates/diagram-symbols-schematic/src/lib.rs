//! Schematic (electronic) symbols for diagram.
//!
//! Provides the `schematic` plugin with common two-terminal components (resistor, capacitor,
//! inductor, diode, voltage source, switch), a ground symbol, a wire junction, and a generic
//! [`ic`](Ic) with named pins.
//!
//! Most parts accept an `orientation` property so they can be stood up or flipped (see the
//! crate `README.md` for the full convention): the axis-symmetric two-terminal parts take
//! `horizontal | vertical`, while the polarized / single-terminal parts (diode, voltage
//! source, ground) take `right | left | up | down`. `junction` and `ic` are symmetric in all
//! dimensions and have no orientation; the IC instead exposes per-side pin-`*_spacing` props.
//! Two-terminal parts expose ports `a` (start) and `b` (end); the IC exposes one port per pin.

use diagram_core::draw::{Painter, PathCmd, ShapeStyle, Stroke, TextStyle};
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
// Orientation
// ---------------------------------------------------------------------------

/// Which way a part faces. For two-terminal parts this is the direction of travel from the
/// start lead (`a`) to the end lead (`b`); `Right` is the natural, unrotated layout.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Facing {
    Right,
    Left,
    Up,
    Down,
}

impl Facing {
    fn is_horizontal(self) -> bool {
        matches!(self, Facing::Right | Facing::Left)
    }

    /// Port directions for the start (`a`) and end (`b`) leads given this facing.
    fn port_dirs(self) -> (Dir, Dir) {
        match self {
            Facing::Right => (Dir::Left, Dir::Right),
            Facing::Left => (Dir::Right, Dir::Left),
            Facing::Down => (Dir::Up, Dir::Down),
            Facing::Up => (Dir::Down, Dir::Up),
        }
    }
}

/// `orientation` for axis-symmetric parts (`horizontal | vertical`, default horizontal).
fn axis_facing(props: &Props) -> Facing {
    match props.text_or("orientation", "horizontal") {
        "vertical" => Facing::Down,
        _ => Facing::Right,
    }
}

/// `orientation` for polarized parts (`right | left | up | down`).
fn dir_facing(props: &Props, default: Facing) -> Facing {
    match props.text_or("orientation", "") {
        "left" => Facing::Left,
        "up" => Facing::Up,
        "down" => Facing::Down,
        "right" => Facing::Right,
        _ => default,
    }
}

fn axis_orientation_prop() -> PropertySpec {
    PropertySpec::optional(
        "orientation",
        PropKind::Enum(&["horizontal", "vertical"]),
        PropValue::text("horizontal"),
        "Layout axis: `horizontal` (default) or `vertical`.",
    )
}

fn dir_orientation_prop(default: &'static str) -> PropertySpec {
    PropertySpec::optional(
        "orientation",
        PropKind::Enum(&["right", "left", "up", "down"]),
        PropValue::text(default),
        "Facing direction: right/left/up/down.",
    )
}

// ---------------------------------------------------------------------------
// Shared geometry / helpers for two-terminal components
// ---------------------------------------------------------------------------

const LEAD: f64 = 10.0;
const BODY_H: f64 = 24.0;
const LABEL_H: f64 = 14.0;
const DEFAULT_W: f64 = 64.0;
/// Width of the label strip placed beside a *vertical* part (horizontal parts use `LABEL_H`).
const VLABEL_W: f64 = 38.0;

fn stroke(style: &Style) -> Stroke {
    Stroke::new(style.stroke, style.stroke_width.max(1.0))
}

fn outline(style: &Style) -> ShapeStyle {
    ShapeStyle::outline(style.stroke, style.stroke_width.max(1.0))
}

fn filled(style: &Style) -> ShapeStyle {
    ShapeStyle::new(style.stroke, style.stroke, style.stroke_width.max(1.0))
}

/// Whether a two-terminal component has a value/label to display.
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

/// Reserved label strip on the *cross* axis: `LABEL_H` above a horizontal part, `VLABEL_W`
/// beside a vertical one, or nothing when there is no label.
fn label_strip(facing: Facing, has_label: bool) -> f64 {
    if !has_label {
        0.0
    } else if facing.is_horizontal() {
        LABEL_H
    } else {
        VLABEL_W
    }
}

/// Geometry context for a two-terminal part. Coordinates are expressed in the part's *natural
/// frame*: `a` runs along the lead-to-lead (long) axis from the start lead, `c` runs across
/// the short axis. [`Tt::at`] maps such a natural point into absolute canvas coordinates for
/// the requested [`Facing`], so each symbol's drawing is written once (as if horizontal).
struct Tt {
    facing: Facing,
    bounds: Rect,
    /// Lead-to-lead extent.
    long: f64,
    /// Cross-axis position of the centerline (where the leads/body sit).
    cc: f64,
    /// Reserved label strip on the cross axis.
    strip: f64,
}

impl Tt {
    fn new(bounds: Rect, facing: Facing, strip: f64) -> Self {
        let (long, short) = if facing.is_horizontal() {
            (bounds.width, bounds.height)
        } else {
            (bounds.height, bounds.width)
        };
        let cc = strip + (short - strip) / 2.0;
        Self {
            facing,
            bounds,
            long,
            cc,
            strip,
        }
    }

    /// Map a natural-frame offset to absolute coordinates.
    fn at(&self, a: f64, c: f64) -> Point {
        let b = self.bounds;
        match self.facing {
            Facing::Right => Point::new(b.x + a, b.y + c),
            Facing::Left => Point::new(b.right() - a, b.y + c),
            Facing::Down => Point::new(b.x + c, b.y + a),
            Facing::Up => Point::new(b.x + c, b.bottom() - a),
        }
    }

    /// The body extent along the long axis (inset from the leads).
    fn body(&self) -> (f64, f64) {
        (LEAD, self.long - LEAD)
    }

    fn ports(&self) -> Vec<Port> {
        let (start, end) = self.facing.port_dirs();
        vec![
            Port::new("a", self.at(0.0, self.cc), start),
            Port::new("b", self.at(self.long, self.cc), end),
        ]
    }

    /// Draw the lead wires from the bounds ends to the body extents `[b0, b1]`.
    fn leads(&self, p: &mut Painter, b0: f64, b1: f64, style: &Style) {
        p.line(self.at(0.0, self.cc), self.at(b0, self.cc), stroke(style));
        p.line(
            self.at(b1, self.cc),
            self.at(self.long, self.cc),
            stroke(style),
        );
    }

    /// Draw the value label in its reserved strip (centered over a horizontal part, beside a
    /// vertical one).
    fn label(&self, p: &mut Painter, text: &str, style: &Style) {
        let pos = if self.facing.is_horizontal() {
            self.at(self.long / 2.0, self.strip - 3.0)
        } else {
            // Vertically centered text reads nicest nudged down by ~half its cap height.
            self.at(self.long / 2.0, self.strip / 2.0)
                .translate(0.0, 4.0)
        };
        p.text(
            pos,
            text,
            TextStyle {
                color: style.text_color,
                font_family: style.font_family.clone(),
                font_size: style.font_size.min(LABEL_H),
                anchor: TextAnchor::Middle,
            },
        );
    }
}

/// Intrinsic size of a two-terminal part: `long` along its axis, `body_short` across, plus the
/// label strip, swapped for vertical orientation.
fn tt_size(facing: Facing, has_label: bool, long: f64, body_short: f64) -> Size {
    let short = body_short + label_strip(facing, has_label);
    if facing.is_horizontal() {
        Size::new(long, short)
    } else {
        Size::new(short, long)
    }
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
            axis_orientation_prop(),
        ]
    }
    fn measure(&self, props: &Props, _m: &dyn TextMeasurer) -> Size {
        let has = value_label(props, "ohms", "\u{2126}").is_some();
        tt_size(axis_facing(props), has, DEFAULT_W, BODY_H)
    }
    fn ports(&self, bounds: Rect, props: &Props) -> Vec<Port> {
        let facing = axis_facing(props);
        let has = value_label(props, "ohms", "\u{2126}").is_some();
        Tt::new(bounds, facing, label_strip(facing, has)).ports()
    }
    fn draw(
        &self,
        p: &mut Painter,
        bounds: Rect,
        props: &Props,
        style: &Style,
        _m: &dyn TextMeasurer,
    ) {
        let facing = axis_facing(props);
        let label = value_label(props, "ohms", "\u{2126}");
        let ctx = Tt::new(bounds, facing, label_strip(facing, label.is_some()));
        let (bl, br) = ctx.body();
        ctx.leads(p, bl, br, style);
        // Zig-zag across [bl, br].
        let segments = 6;
        let amp = 8.0;
        let mut pts = vec![ctx.at(bl, ctx.cc)];
        for i in 0..segments {
            let a = bl + (br - bl) * (i as f64 + 0.5) / segments as f64;
            let c = if i % 2 == 0 {
                ctx.cc - amp
            } else {
                ctx.cc + amp
            };
            pts.push(ctx.at(a, c));
        }
        pts.push(ctx.at(br, ctx.cc));
        p.polyline(pts, stroke(style));
        if let Some(l) = label {
            ctx.label(p, &l, style);
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
            axis_orientation_prop(),
        ]
    }
    fn measure(&self, props: &Props, _m: &dyn TextMeasurer) -> Size {
        let has = value_label(props, "farads", "F").is_some();
        tt_size(axis_facing(props), has, DEFAULT_W, BODY_H)
    }
    fn ports(&self, bounds: Rect, props: &Props) -> Vec<Port> {
        let facing = axis_facing(props);
        let has = value_label(props, "farads", "F").is_some();
        Tt::new(bounds, facing, label_strip(facing, has)).ports()
    }
    fn draw(
        &self,
        p: &mut Painter,
        bounds: Rect,
        props: &Props,
        style: &Style,
        _m: &dyn TextMeasurer,
    ) {
        let facing = axis_facing(props);
        let label = value_label(props, "farads", "F");
        let ctx = Tt::new(bounds, facing, label_strip(facing, label.is_some()));
        let gap = 6.0;
        let mid = ctx.long / 2.0;
        let plate_l = mid - gap / 2.0;
        let plate_r = mid + gap / 2.0;
        ctx.leads(p, plate_l, plate_r, style);
        let half = 8.0;
        p.line(
            ctx.at(plate_l, ctx.cc - half),
            ctx.at(plate_l, ctx.cc + half),
            stroke(style),
        );
        p.line(
            ctx.at(plate_r, ctx.cc - half),
            ctx.at(plate_r, ctx.cc + half),
            stroke(style),
        );
        if let Some(l) = label {
            ctx.label(p, &l, style);
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
            axis_orientation_prop(),
        ]
    }
    fn measure(&self, props: &Props, _m: &dyn TextMeasurer) -> Size {
        let has = value_label(props, "henries", "H").is_some();
        tt_size(axis_facing(props), has, DEFAULT_W, BODY_H)
    }
    fn ports(&self, bounds: Rect, props: &Props) -> Vec<Port> {
        let facing = axis_facing(props);
        let has = value_label(props, "henries", "H").is_some();
        Tt::new(bounds, facing, label_strip(facing, has)).ports()
    }
    fn draw(
        &self,
        p: &mut Painter,
        bounds: Rect,
        props: &Props,
        style: &Style,
        _m: &dyn TextMeasurer,
    ) {
        let facing = axis_facing(props);
        let label = value_label(props, "henries", "H");
        let ctx = Tt::new(bounds, facing, label_strip(facing, label.is_some()));
        let (bl, br) = ctx.body();
        ctx.leads(p, bl, br, style);
        // Four semicircular humps as cubic arcs, bumping toward -c.
        let humps = 4;
        let w = (br - bl) / humps as f64;
        let r = w / 2.0;
        let k = r * 1.333;
        let mut cmds = vec![PathCmd::MoveTo(ctx.at(bl, ctx.cc))];
        for i in 0..humps {
            let a0 = bl + i as f64 * w;
            let a1 = a0 + w;
            cmds.push(PathCmd::CurveTo(
                ctx.at(a0, ctx.cc - k),
                ctx.at(a1, ctx.cc - k),
                ctx.at(a1, ctx.cc),
            ));
        }
        p.path(
            cmds,
            ShapeStyle::outline(style.stroke, style.stroke_width.max(1.0)),
        );
        if let Some(l) = label {
            ctx.label(p, &l, style);
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
        vec![
            PropertySpec::optional(
                "label",
                PropKind::Text,
                PropValue::text(""),
                "Optional label.",
            ),
            dir_orientation_prop("right"),
        ]
    }
    fn measure(&self, props: &Props, _m: &dyn TextMeasurer) -> Size {
        let has = !props.text_or("label", "").is_empty();
        tt_size(dir_facing(props, Facing::Right), has, DEFAULT_W, BODY_H)
    }
    fn ports(&self, bounds: Rect, props: &Props) -> Vec<Port> {
        let facing = dir_facing(props, Facing::Right);
        let has = !props.text_or("label", "").is_empty();
        Tt::new(bounds, facing, label_strip(facing, has)).ports()
    }
    fn draw(
        &self,
        p: &mut Painter,
        bounds: Rect,
        props: &Props,
        style: &Style,
        _m: &dyn TextMeasurer,
    ) {
        let facing = dir_facing(props, Facing::Right);
        let has_label = !props.text_or("label", "").is_empty();
        let ctx = Tt::new(bounds, facing, label_strip(facing, has_label));
        let mid = ctx.long / 2.0;
        let half = 9.0;
        let tri_l = mid - half;
        let tri_r = mid + half;
        ctx.leads(p, tri_l, tri_r, style);
        // Triangle pointing toward the cathode (+a), then the cathode bar.
        p.path(
            vec![
                PathCmd::MoveTo(ctx.at(tri_l, ctx.cc - half)),
                PathCmd::LineTo(ctx.at(tri_l, ctx.cc + half)),
                PathCmd::LineTo(ctx.at(tri_r, ctx.cc)),
                PathCmd::Close,
            ],
            filled(style),
        );
        p.line(
            ctx.at(tri_r, ctx.cc - half),
            ctx.at(tri_r, ctx.cc + half),
            stroke(style),
        );
        if has_label {
            ctx.label(p, props.text_or("label", ""), style);
        }
    }
}

// ---------------------------------------------------------------------------
// Voltage source
// ---------------------------------------------------------------------------

pub struct VoltageSource;

const VSOURCE_BODY: f64 = 34.0;

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
            dir_orientation_prop("right"),
        ]
    }
    fn measure(&self, props: &Props, _m: &dyn TextMeasurer) -> Size {
        let has = value_label(props, "volts", "V").is_some();
        tt_size(
            dir_facing(props, Facing::Right),
            has,
            DEFAULT_W,
            VSOURCE_BODY,
        )
    }
    fn ports(&self, bounds: Rect, props: &Props) -> Vec<Port> {
        let facing = dir_facing(props, Facing::Right);
        let has = value_label(props, "volts", "V").is_some();
        Tt::new(bounds, facing, label_strip(facing, has)).ports()
    }
    fn draw(
        &self,
        p: &mut Painter,
        bounds: Rect,
        props: &Props,
        style: &Style,
        _m: &dyn TextMeasurer,
    ) {
        let facing = dir_facing(props, Facing::Right);
        let label = value_label(props, "volts", "V");
        let ctx = Tt::new(bounds, facing, label_strip(facing, label.is_some()));
        let mid = ctx.long / 2.0;
        let r = 15.0;
        ctx.leads(p, mid - r, mid + r, style);
        p.circle(ctx.at(mid, ctx.cc), r, outline(style));
        // + toward the start lead, - toward the end lead.
        let s = 4.0;
        let plus = mid - r * 0.45;
        let minus = mid + r * 0.45;
        p.line(
            ctx.at(plus - s, ctx.cc),
            ctx.at(plus + s, ctx.cc),
            stroke(style),
        );
        p.line(
            ctx.at(plus, ctx.cc - s),
            ctx.at(plus, ctx.cc + s),
            stroke(style),
        );
        p.line(
            ctx.at(minus - s, ctx.cc),
            ctx.at(minus + s, ctx.cc),
            stroke(style),
        );
        if let Some(l) = label {
            ctx.label(p, &l, style);
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
        "SPST switch (open). `label` is drawn alongside."
    }
    fn property_schema(&self) -> Vec<PropertySpec> {
        vec![
            PropertySpec::optional(
                "label",
                PropKind::Text,
                PropValue::text(""),
                "Optional label.",
            ),
            axis_orientation_prop(),
        ]
    }
    fn measure(&self, props: &Props, _m: &dyn TextMeasurer) -> Size {
        let has = !props.text_or("label", "").is_empty();
        tt_size(axis_facing(props), has, DEFAULT_W, BODY_H)
    }
    fn ports(&self, bounds: Rect, props: &Props) -> Vec<Port> {
        let facing = axis_facing(props);
        let has = !props.text_or("label", "").is_empty();
        Tt::new(bounds, facing, label_strip(facing, has)).ports()
    }
    fn draw(
        &self,
        p: &mut Painter,
        bounds: Rect,
        props: &Props,
        style: &Style,
        _m: &dyn TextMeasurer,
    ) {
        let facing = axis_facing(props);
        let has_label = !props.text_or("label", "").is_empty();
        let ctx = Tt::new(bounds, facing, label_strip(facing, has_label));
        let (bl, br) = ctx.body();
        ctx.leads(p, bl, br, style);
        p.circle(ctx.at(bl, ctx.cc), 1.6, filled(style));
        p.circle(ctx.at(br, ctx.cc), 1.6, filled(style));
        // Open lever from the start contact, angled off the centerline to near the end contact.
        p.line(
            ctx.at(bl, ctx.cc),
            ctx.at(br - 2.0, ctx.cc - 10.0),
            stroke(style),
        );
        if has_label {
            ctx.label(p, props.text_or("label", ""), style);
        }
    }
}

// ---------------------------------------------------------------------------
// Ground
// ---------------------------------------------------------------------------

pub struct Ground;

/// The direction the ground's single lead/port points (toward the circuit).
fn ground_dir(props: &Props) -> Dir {
    match props.text_or("orientation", "up") {
        "down" => Dir::Down,
        "left" => Dir::Left,
        "right" => Dir::Right,
        _ => Dir::Up,
    }
}

/// Map a ground natural-frame point — `a` from the port into the symbol, `c` across — for the
/// port direction `dir`. The natural frame points up (port at top, bars below).
fn ground_at(bounds: Rect, dir: Dir, a: f64, c: f64) -> Point {
    let b = bounds;
    match dir {
        Dir::Up => Point::new(b.x + c, b.y + a),
        Dir::Down => Point::new(b.x + c, b.bottom() - a),
        Dir::Right => Point::new(b.right() - a, b.y + c),
        Dir::Left => Point::new(b.x + a, b.y + c),
    }
}

/// `(long, short)` natural extents of the ground for the given facing.
fn ground_extents(bounds: Rect, dir: Dir) -> (f64, f64) {
    match dir {
        Dir::Up | Dir::Down => (bounds.height, bounds.width),
        Dir::Left | Dir::Right => (bounds.width, bounds.height),
    }
}

impl Symbol for Ground {
    fn name(&self) -> &str {
        "ground"
    }
    fn description(&self) -> &str {
        "Ground symbol; single port `a`. `orientation` points the lead up (default)/down/left/right."
    }
    fn property_schema(&self) -> Vec<PropertySpec> {
        vec![dir_orientation_prop("up")]
    }
    fn measure(&self, props: &Props, _m: &dyn TextMeasurer) -> Size {
        match ground_dir(props) {
            Dir::Up | Dir::Down => Size::new(28.0, 26.0),
            Dir::Left | Dir::Right => Size::new(26.0, 28.0),
        }
    }
    fn ports(&self, bounds: Rect, props: &Props) -> Vec<Port> {
        let dir = ground_dir(props);
        let (_, short) = ground_extents(bounds, dir);
        vec![Port::new(
            "a",
            ground_at(bounds, dir, 0.0, short / 2.0),
            dir,
        )]
    }
    fn draw(
        &self,
        p: &mut Painter,
        bounds: Rect,
        props: &Props,
        style: &Style,
        _m: &dyn TextMeasurer,
    ) {
        let dir = ground_dir(props);
        let (_, short) = ground_extents(bounds, dir);
        let cc = short / 2.0;
        let at = |a: f64, c: f64| ground_at(bounds, dir, a, c);
        // Lead from the port, then three shrinking bars.
        let bar_a = 10.0;
        p.line(at(0.0, cc), at(bar_a, cc), stroke(style));
        let widths = [14.0, 9.0, 4.0];
        for (i, w) in widths.iter().enumerate() {
            let a = bar_a + i as f64 * 5.0;
            p.line(at(a, cc - w), at(a, cc + w), stroke(style));
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

/// Positions of `count` pins along a side spanning `[start, start+extent]`. With `spacing > 0`
/// the pins are placed at exactly that center-to-center distance, as a block centered on the
/// side; otherwise they are distributed evenly (the historical default).
fn side_positions(count: usize, spacing: f64, start: f64, extent: f64) -> Vec<f64> {
    if count == 0 {
        return Vec::new();
    }
    if spacing > 0.0 {
        let span = (count as f64 - 1.0) * spacing;
        let first = start + (extent - span) / 2.0;
        (0..count).map(|i| first + i as f64 * spacing).collect()
    } else {
        (0..count)
            .map(|i| start + extent * (i as f64 + 1.0) / (count as f64 + 1.0))
            .collect()
    }
}

impl Ic {
    fn pins(props: &Props, side: &str) -> Vec<String> {
        props.text_list(side)
    }

    fn spacing(props: &Props, side: &str) -> f64 {
        props.number_or(side, 0.0).max(0.0)
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

        let ys_left = side_positions(
            left.len(),
            Self::spacing(props, "left_spacing"),
            body.y,
            body.height,
        );
        for (i, name) in left.iter().enumerate() {
            let y = ys_left[i];
            out.push(PinPlacement {
                name: name.clone(),
                tip: Point::new(bounds.x, y),
                label_pos: Point::new(body.x + IC_LABEL_PAD, y + 4.0),
                anchor: TextAnchor::Start,
                dir: Dir::Left,
                body_anchor: Point::new(body.x, y),
            });
        }
        let ys_right = side_positions(
            right.len(),
            Self::spacing(props, "right_spacing"),
            body.y,
            body.height,
        );
        for (i, name) in right.iter().enumerate() {
            let y = ys_right[i];
            out.push(PinPlacement {
                name: name.clone(),
                tip: Point::new(bounds.right(), y),
                label_pos: Point::new(body.right() - IC_LABEL_PAD, y + 4.0),
                anchor: TextAnchor::End,
                dir: Dir::Right,
                body_anchor: Point::new(body.right(), y),
            });
        }
        let xs_top = side_positions(
            top.len(),
            Self::spacing(props, "top_spacing"),
            body.x,
            body.width,
        );
        for (i, name) in top.iter().enumerate() {
            let x = xs_top[i];
            out.push(PinPlacement {
                name: name.clone(),
                tip: Point::new(x, bounds.y),
                label_pos: Point::new(x, body.y + IC_LABEL_PAD + 4.0),
                anchor: TextAnchor::Middle,
                dir: Dir::Up,
                body_anchor: Point::new(x, body.y),
            });
        }
        let xs_bottom = side_positions(
            bottom.len(),
            Self::spacing(props, "bottom_spacing"),
            body.x,
            body.width,
        );
        for (i, name) in bottom.iter().enumerate() {
            let x = xs_bottom[i];
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
        "Generic IC / chip with a `name` and named pins on each side (left_pins, right_pins, top_pins, bottom_pins). Per-side `*_spacing` (px) spreads the pins out."
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
            PropertySpec::optional(
                "left_spacing",
                PropKind::Number,
                PropValue::Number(0.0),
                "Exact px between adjacent left pins (0 = auto).",
            ),
            PropertySpec::optional(
                "right_spacing",
                PropKind::Number,
                PropValue::Number(0.0),
                "Exact px between adjacent right pins (0 = auto).",
            ),
            PropertySpec::optional(
                "top_spacing",
                PropKind::Number,
                PropValue::Number(0.0),
                "Exact px between adjacent top pins (0 = auto).",
            ),
            PropertySpec::optional(
                "bottom_spacing",
                PropKind::Number,
                PropValue::Number(0.0),
                "Exact px between adjacent bottom pins (0 = auto).",
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

        // A side with explicit spacing needs `(n+1) * spacing` so adjacent pins (and the end
        // margins) sit exactly that far apart once `side_positions` centers them.
        let spacing_need = |n: usize, sp: f64| {
            if sp > 0.0 {
                (n as f64 + 1.0) * sp
            } else {
                0.0
            }
        };

        // Width must fit: left labels + centered name + right labels (with clearance), and
        // enough column spacing that top/bottom pin labels don't overlap.
        let horizontal_need = left_w + right_w + name_w + 36.0;
        let top_need = (top.len() as f64 + 1.0) * (top_w + 10.0);
        let bottom_need = (bottom.len() as f64 + 1.0) * (bottom_w + 10.0);
        let inner_w = horizontal_need
            .max(top_need)
            .max(bottom_need)
            .max(name_w + 16.0)
            .max(spacing_need(top.len(), Self::spacing(props, "top_spacing")))
            .max(spacing_need(
                bottom.len(),
                Self::spacing(props, "bottom_spacing"),
            ))
            .max(48.0);
        let inner_h = (rows as f64 * IC_PIN_SPACING)
            .max(spacing_need(
                left.len(),
                Self::spacing(props, "left_spacing"),
            ))
            .max(spacing_need(
                right.len(),
                Self::spacing(props, "right_spacing"),
            ))
            .max(40.0);

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

    fn with_props(sym: &dyn Symbol, pairs: &[(&str, PropValue)]) -> Props {
        let mut props = Props::new();
        for (k, v) in pairs {
            props.insert(*k, v.clone());
        }
        props.with_defaults(&sym.property_schema())
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
    fn resistor_vertical_swaps_size_and_ports() {
        let r = Resistor;
        let m = BasicMeasurer::default();
        let horiz = with_props(&r, &[("ohms", PropValue::Number(220.0))]);
        let vert = with_props(
            &r,
            &[
                ("ohms", PropValue::Number(220.0)),
                ("orientation", PropValue::text("vertical")),
            ],
        );
        let hs = r.measure(&horiz, &m);
        let vs = r.measure(&vert, &m);
        // Horizontal is wider than tall; vertical is the transpose (taller than wide).
        assert!(hs.width > hs.height);
        assert!(vs.height > vs.width);
        assert!((vs.height - hs.width).abs() < 1e-9);

        // Vertical ports land on the top/bottom edges, facing up/down.
        let bounds = Rect::from_origin_size(Point::ZERO, vs);
        let ports = r.ports(bounds, &vert);
        let a = ports.iter().find(|p| p.name == "a").unwrap();
        let b = ports.iter().find(|p| p.name == "b").unwrap();
        assert_eq!(a.dir, Dir::Up);
        assert_eq!(b.dir, Dir::Down);
        assert!((a.point.y - bounds.y).abs() < 1e-9);
        assert!((b.point.y - bounds.bottom()).abs() < 1e-9);
    }

    #[test]
    fn diode_flips_with_orientation() {
        let d = Diode;
        let m = BasicMeasurer::default();
        let right = with_props(&d, &[]);
        let left = with_props(&d, &[("orientation", PropValue::text("left"))]);
        let bounds = Rect::from_origin_size(Point::ZERO, d.measure(&right, &m));

        // Anode `a` rides the left edge facing right, mirrored to the right edge facing left.
        let ar = d.ports(bounds, &right);
        let a_r = ar.iter().find(|p| p.name == "a").unwrap();
        assert_eq!(a_r.dir, Dir::Left);
        assert!((a_r.point.x - bounds.x).abs() < 1e-9);

        let al = d.ports(bounds, &left);
        let a_l = al.iter().find(|p| p.name == "a").unwrap();
        assert_eq!(a_l.dir, Dir::Right);
        assert!((a_l.point.x - bounds.right()).abs() < 1e-9);

        // `up`/`down` transpose the bounding box.
        let up = with_props(&d, &[("orientation", PropValue::text("up"))]);
        let us = d.measure(&up, &m);
        assert!(us.height > us.width);

        // Still draws its filled triangle.
        let mut p = Painter::new();
        d.draw(&mut p, bounds, &right, &Style::default(), &m);
        assert!(p
            .primitives()
            .iter()
            .any(|x| matches!(x, Primitive::Path { .. })));
    }

    #[test]
    fn ground_orientation_moves_the_port() {
        let g = Ground;
        // Default: port at the top, facing up.
        let up = with_props(&g, &[]);
        let bounds = Rect::new(0.0, 0.0, 28.0, 26.0);
        let ports = g.ports(bounds, &up);
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].dir, Dir::Up);
        assert!((ports[0].point.y - bounds.y).abs() < 1e-9);

        // Down: port at the bottom, facing down (size unchanged for the vertical axis).
        let down = with_props(&g, &[("orientation", PropValue::text("down"))]);
        let ports = g.ports(bounds, &down);
        assert_eq!(ports[0].dir, Dir::Down);
        assert!((ports[0].point.y - bounds.bottom()).abs() < 1e-9);

        // Right: the box transposes to 26x28.
        let right = with_props(&g, &[("orientation", PropValue::text("right"))]);
        let m = BasicMeasurer::default();
        let s = g.measure(&right, &m);
        assert!((s.width - 26.0).abs() < 1e-9 && (s.height - 28.0).abs() < 1e-9);
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
    fn ic_left_spacing_spreads_pins_exactly() {
        let ic = Ic;
        let m = BasicMeasurer::default();
        let pins = PropValue::List(vec![
            PropValue::text("A"),
            PropValue::text("B"),
            PropValue::text("C"),
        ]);

        let auto = {
            let mut p = Props::new();
            p.insert("left_pins", pins.clone());
            p
        };
        let spaced = {
            let mut p = Props::new();
            p.insert("left_pins", pins.clone());
            p.insert("left_spacing", PropValue::Number(40.0));
            p
        };

        // Explicit spacing grows the IC taller than the auto layout.
        assert!(ic.measure(&spaced, &m).height > ic.measure(&auto, &m).height);

        // Adjacent left ports end up exactly 40px apart.
        let bounds = Rect::from_origin_size(Point::ZERO, ic.measure(&spaced, &m));
        let mut ys: Vec<f64> = ic
            .ports(bounds, &spaced)
            .iter()
            .map(|p| p.point.y)
            .collect();
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!((ys[1] - ys[0] - 40.0).abs() < 1e-6);
        assert!((ys[2] - ys[1] - 40.0).abs() < 1e-6);
    }
}
