//! Colors and visual styling.
//!
//! [`Style`] holds the resolved visual attributes (stroke, fill, font, …) for an element.
//! Containers pass their style down to children, which override only the fields they set;
//! see [`Style::inherit`].

/// An sRGB color with an alpha channel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0, a: 255 };
    pub const WHITE: Color = Color { r: 255, g: 255, b: 255, a: 255 };
    pub const NONE: Color = Color { r: 0, g: 0, b: 0, a: 0 };

    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b, a: 255 }
    }

    pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color { r, g, b, a }
    }

    /// True if the color is fully transparent.
    pub fn is_transparent(&self) -> bool {
        self.a == 0
    }

    /// Parse a color from a CSS-like string: `#rgb`, `#rrggbb`, `#rrggbbaa`, `none`, or one of
    /// a small set of named colors. Returns `None` if it can't be parsed.
    pub fn parse(s: &str) -> Option<Color> {
        let s = s.trim();
        if s.eq_ignore_ascii_case("none") || s.eq_ignore_ascii_case("transparent") {
            return Some(Color::NONE);
        }
        if let Some(c) = named_color(s) {
            return Some(c);
        }
        let hex = s.strip_prefix('#')?;
        let parse2 = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        match hex.len() {
            3 => {
                // #rgb -> #rrggbb
                let nibble = |i: usize| {
                    let v = u8::from_str_radix(&hex[i..i + 1], 16).ok()?;
                    Some(v * 17) // 0xF -> 0xFF
                };
                Some(Color::rgb(nibble(0)?, nibble(1)?, nibble(2)?))
            }
            6 => Some(Color::rgb(parse2(0)?, parse2(2)?, parse2(4)?)),
            8 => Some(Color::rgba(parse2(0)?, parse2(2)?, parse2(4)?, parse2(6)?)),
            _ => None,
        }
    }

    /// Format as `#rrggbb` (used when alpha is full) — alpha is emitted separately via
    /// the SVG `*-opacity` attributes.
    pub fn to_hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// Alpha as a 0.0–1.0 opacity value.
    pub fn opacity(&self) -> f64 {
        self.a as f64 / 255.0
    }
}

fn named_color(name: &str) -> Option<Color> {
    let c = match name.to_ascii_lowercase().as_str() {
        "black" => Color::BLACK,
        "white" => Color::WHITE,
        "red" => Color::rgb(0xff, 0x00, 0x00),
        "green" => Color::rgb(0x00, 0x80, 0x00),
        "blue" => Color::rgb(0x00, 0x00, 0xff),
        "yellow" => Color::rgb(0xff, 0xff, 0x00),
        "orange" => Color::rgb(0xff, 0xa5, 0x00),
        "gray" | "grey" => Color::rgb(0x80, 0x80, 0x80),
        "lightgray" | "lightgrey" => Color::rgb(0xd3, 0xd3, 0xd3),
        "darkgray" | "darkgrey" => Color::rgb(0xa9, 0xa9, 0xa9),
        _ => return None,
    };
    Some(c)
}

/// Horizontal text anchoring, mirroring SVG's `text-anchor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAnchor {
    #[default]
    Start,
    Middle,
    End,
}

/// The fully-resolved style for an element. Every field is concrete after inheritance
/// resolution (no `Option`s remain), which keeps the renderer simple.
#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    pub stroke: Color,
    pub stroke_width: f64,
    pub fill: Color,
    pub text_color: Color,
    pub font_family: String,
    pub font_size: f64,
    pub text_anchor: TextAnchor,
    pub opacity: f64,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            stroke: Color::BLACK,
            stroke_width: 1.0,
            fill: Color::NONE,
            text_color: Color::BLACK,
            font_family: "sans-serif".to_string(),
            font_size: 14.0,
            text_anchor: TextAnchor::Start,
            opacity: 1.0,
        }
    }
}

/// A sparse set of style overrides, as parsed from a diagram element. Fields left `None`
/// are inherited from the parent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StylePatch {
    pub stroke: Option<Color>,
    pub stroke_width: Option<f64>,
    pub fill: Option<Color>,
    pub text_color: Option<Color>,
    pub font_family: Option<String>,
    pub font_size: Option<f64>,
    pub text_anchor: Option<TextAnchor>,
    pub opacity: Option<f64>,
}

impl Style {
    /// Produce a new style by applying `patch` on top of `self`. Unset patch fields are
    /// inherited unchanged.
    pub fn inherit(&self, patch: &StylePatch) -> Style {
        Style {
            stroke: patch.stroke.unwrap_or(self.stroke),
            stroke_width: patch.stroke_width.unwrap_or(self.stroke_width),
            fill: patch.fill.unwrap_or(self.fill),
            text_color: patch.text_color.unwrap_or(self.text_color),
            font_family: patch.font_family.clone().unwrap_or_else(|| self.font_family.clone()),
            font_size: patch.font_size.unwrap_or(self.font_size),
            text_anchor: patch.text_anchor.unwrap_or(self.text_anchor),
            opacity: patch.opacity.unwrap_or(self.opacity),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_forms() {
        assert_eq!(Color::parse("#000000"), Some(Color::BLACK));
        assert_eq!(Color::parse("#fff"), Some(Color::WHITE));
        assert_eq!(Color::parse("#ff0000"), Some(Color::rgb(255, 0, 0)));
        assert_eq!(Color::parse("#00000080"), Some(Color::rgba(0, 0, 0, 0x80)));
    }

    #[test]
    fn parse_named_and_none() {
        assert_eq!(Color::parse("black"), Some(Color::BLACK));
        assert_eq!(Color::parse("RED"), Some(Color::rgb(255, 0, 0)));
        assert_eq!(Color::parse("none"), Some(Color::NONE));
        assert!(Color::parse("none").unwrap().is_transparent());
    }

    #[test]
    fn parse_rejects_garbage() {
        assert_eq!(Color::parse("#12"), None);
        assert_eq!(Color::parse("not-a-color"), None);
    }

    #[test]
    fn hex_roundtrip_and_opacity() {
        let c = Color::rgba(0x12, 0x34, 0x56, 0x80);
        assert_eq!(c.to_hex(), "#123456");
        assert!((c.opacity() - 0.50196).abs() < 1e-3);
    }

    #[test]
    fn inherit_overrides_only_set_fields() {
        let base = Style::default();
        let patch = StylePatch { font_size: Some(20.0), fill: Some(Color::WHITE), ..Default::default() };
        let s = base.inherit(&patch);
        assert_eq!(s.font_size, 20.0);
        assert_eq!(s.fill, Color::WHITE);
        // unchanged:
        assert_eq!(s.stroke, base.stroke);
        assert_eq!(s.font_family, base.font_family);
    }
}
