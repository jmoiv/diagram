//! Render a laid-out [`Scene`] to an SVG document string.
//!
//! SVG is the canonical output: PNG and PDF are produced by converting this SVG (see
//! [`crate::output`]). Keeping a single SVG generator means one source of truth for geometry.

use std::fmt::Write;

use crate::draw::{PathCmd, Primitive, ShapeStyle, Stroke, TextStyle};
use crate::geom::Point;
use crate::layout::Scene;
use crate::style::{Color, TextAnchor};

/// Render a scene to a standalone SVG document.
pub fn to_svg(scene: &Scene) -> String {
    let mut s = String::new();
    let _ = write!(
        s,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">",
        w = num(scene.size.width),
        h = num(scene.size.height),
    );
    s.push('\n');

    if let Some(bg) = scene.background {
        if !bg.is_transparent() {
            let _ = write!(
                s,
                "  <rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"{}\"{}/>\n",
                num(scene.size.width),
                num(scene.size.height),
                bg.to_hex(),
                opacity_attr("fill-opacity", bg),
            );
        }
    }

    for p in &scene.primitives {
        render_primitive(&mut s, p);
    }

    s.push_str("</svg>\n");
    s
}

fn render_primitive(s: &mut String, p: &Primitive) {
    match p {
        Primitive::Line { a, b, stroke } => {
            let _ = write!(
                s,
                "  <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"{}/>\n",
                num(a.x),
                num(a.y),
                num(b.x),
                num(b.y),
                stroke_attrs(stroke),
            );
        }
        Primitive::Polyline { points, stroke } => {
            let _ = write!(
                s,
                "  <polyline points=\"{}\" fill=\"none\"{}/>\n",
                points_attr(points),
                stroke_attrs(stroke),
            );
        }
        Primitive::Rect { rect, rx, style } => {
            let rx_attr = if *rx > 0.0 {
                format!(" rx=\"{}\"", num(*rx))
            } else {
                String::new()
            };
            let _ = write!(
                s,
                "  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{}{}/>\n",
                num(rect.x),
                num(rect.y),
                num(rect.width),
                num(rect.height),
                rx_attr,
                shape_attrs(style),
            );
        }
        Primitive::Circle { center, radius, style } => {
            let _ = write!(
                s,
                "  <circle cx=\"{}\" cy=\"{}\" r=\"{}\"{}/>\n",
                num(center.x),
                num(center.y),
                num(*radius),
                shape_attrs(style),
            );
        }
        Primitive::Path { cmds, style } => {
            let _ = write!(s, "  <path d=\"{}\"{}/>\n", path_data(cmds), shape_attrs(style));
        }
        Primitive::Text { pos, text, style } => {
            let _ = write!(
                s,
                "  <text x=\"{}\" y=\"{}\"{}>{}</text>\n",
                num(pos.x),
                num(pos.y),
                text_attrs(style),
                escape_xml(text),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Attribute formatting
// ---------------------------------------------------------------------------

fn stroke_attrs(stroke: &Stroke) -> String {
    if stroke.color.is_transparent() {
        return " stroke=\"none\"".to_string();
    }
    format!(
        " stroke=\"{}\" stroke-width=\"{}\"{}",
        stroke.color.to_hex(),
        num(stroke.width),
        opacity_attr("stroke-opacity", stroke.color),
    )
}

fn shape_attrs(style: &ShapeStyle) -> String {
    let mut out = String::new();
    if style.fill.is_transparent() {
        out.push_str(" fill=\"none\"");
    } else {
        out.push_str(&format!(
            " fill=\"{}\"{}",
            style.fill.to_hex(),
            opacity_attr("fill-opacity", style.fill)
        ));
    }
    if style.stroke.is_transparent() || style.stroke_width <= 0.0 {
        out.push_str(" stroke=\"none\"");
    } else {
        out.push_str(&format!(
            " stroke=\"{}\" stroke-width=\"{}\"{}",
            style.stroke.to_hex(),
            num(style.stroke_width),
            opacity_attr("stroke-opacity", style.stroke),
        ));
    }
    out
}

fn text_attrs(style: &TextStyle) -> String {
    let anchor = match style.anchor {
        TextAnchor::Start => "start",
        TextAnchor::Middle => "middle",
        TextAnchor::End => "end",
    };
    let mut out = format!(
        " font-family=\"{}\" font-size=\"{}\" fill=\"{}\"",
        escape_xml(&style.font_family),
        num(style.font_size),
        style.color.to_hex(),
    );
    if anchor != "start" {
        out.push_str(&format!(" text-anchor=\"{anchor}\""));
    }
    out.push_str(&opacity_attr("fill-opacity", style.color));
    out
}

fn opacity_attr(name: &str, color: Color) -> String {
    if color.a == 255 {
        String::new()
    } else {
        format!(" {name}=\"{}\"", num(color.opacity()))
    }
}

fn points_attr(points: &[Point]) -> String {
    points
        .iter()
        .map(|p| format!("{},{}", num(p.x), num(p.y)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn path_data(cmds: &[PathCmd]) -> String {
    let mut d = String::new();
    for c in cmds {
        match c {
            PathCmd::MoveTo(p) => {
                let _ = write!(d, "M{} {} ", num(p.x), num(p.y));
            }
            PathCmd::LineTo(p) => {
                let _ = write!(d, "L{} {} ", num(p.x), num(p.y));
            }
            PathCmd::QuadTo(c1, e) => {
                let _ = write!(d, "Q{} {} {} {} ", num(c1.x), num(c1.y), num(e.x), num(e.y));
            }
            PathCmd::CurveTo(c1, c2, e) => {
                let _ = write!(
                    d,
                    "C{} {} {} {} {} {} ",
                    num(c1.x),
                    num(c1.y),
                    num(c2.x),
                    num(c2.y),
                    num(e.x),
                    num(e.y)
                );
            }
            PathCmd::Close => d.push_str("Z "),
        }
    }
    d.trim_end().to_string()
}

/// Format a number with up to 3 decimal places, trimming trailing zeros, so the SVG stays
/// compact and stable (good for snapshot tests).
fn num(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        return format!("{}", v as i64);
    }
    let mut s = format!("{v:.3}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    s
}

fn escape_xml(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::Primitive;
    use crate::geom::{Point, Rect, Size};
    use crate::style::Color;

    fn scene(prims: Vec<Primitive>) -> Scene {
        Scene { size: Size::new(100.0, 50.0), background: None, primitives: prims }
    }

    #[test]
    fn svg_has_header_and_dimensions() {
        let svg = to_svg(&scene(vec![]));
        assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
        assert!(svg.contains("width=\"100\""));
        assert!(svg.contains("height=\"50\""));
        assert!(svg.contains("viewBox=\"0 0 100 50\""));
        assert!(svg.trim_end().ends_with("</svg>"));
    }

    #[test]
    fn renders_rect_with_fill_and_stroke() {
        let svg = to_svg(&scene(vec![Primitive::Rect {
            rect: Rect::new(1.0, 2.0, 3.0, 4.0),
            rx: 1.5,
            style: ShapeStyle::new(Color::WHITE, Color::BLACK, 2.0),
        }]));
        assert!(svg.contains("<rect x=\"1\" y=\"2\" width=\"3\" height=\"4\" rx=\"1.5\""));
        assert!(svg.contains("fill=\"#ffffff\""));
        assert!(svg.contains("stroke=\"#000000\""));
        assert!(svg.contains("stroke-width=\"2\""));
    }

    #[test]
    fn transparent_fill_becomes_none() {
        let svg = to_svg(&scene(vec![Primitive::Circle {
            center: Point::new(5.0, 5.0),
            radius: 3.0,
            style: ShapeStyle::outline(Color::BLACK, 1.0),
        }]));
        assert!(svg.contains("fill=\"none\""));
        assert!(svg.contains("<circle cx=\"5\" cy=\"5\" r=\"3\""));
    }

    #[test]
    fn text_is_escaped_and_anchored() {
        let svg = to_svg(&scene(vec![Primitive::Text {
            pos: Point::new(10.0, 20.0),
            text: "a < b & \"c\"".to_string(),
            style: TextStyle {
                color: Color::BLACK,
                font_family: "sans-serif".into(),
                font_size: 14.0,
                anchor: TextAnchor::Middle,
            },
        }]));
        assert!(svg.contains("a &lt; b &amp; &quot;c&quot;"));
        assert!(svg.contains("text-anchor=\"middle\""));
        assert!(svg.contains("font-size=\"14\""));
    }

    #[test]
    fn alpha_emits_opacity() {
        let svg = to_svg(&scene(vec![Primitive::Rect {
            rect: Rect::new(0.0, 0.0, 1.0, 1.0),
            rx: 0.0,
            style: ShapeStyle::new(Color::rgba(0, 0, 0, 128), Color::NONE, 0.0),
        }]));
        assert!(svg.contains("fill-opacity="));
    }

    #[test]
    fn num_formats_compactly() {
        assert_eq!(num(10.0), "10");
        assert_eq!(num(10.5), "10.5");
        assert_eq!(num(10.250), "10.25");
        assert_eq!(num(-3.0), "-3");
    }

    #[test]
    fn path_data_roundtrip() {
        let d = path_data(&[
            PathCmd::MoveTo(Point::new(0.0, 0.0)),
            PathCmd::LineTo(Point::new(10.0, 0.0)),
            PathCmd::Close,
        ]);
        assert_eq!(d, "M0 0 L10 0 Z");
    }
}
