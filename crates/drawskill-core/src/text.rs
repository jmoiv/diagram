//! Font discovery and text measurement, backed by `fontdb` + `cosmic-text`.
//!
//! [`FontContext`] owns a `cosmic-text` `FontSystem` (which loads the system fonts once) and
//! implements [`TextMeasurer`]. The same system fonts are used by the SVG renderer (`usvg`
//! also reads `fontdb`), so measured sizes match the rendered glyphs. It also powers the
//! `drawskill fonts` command via [`FontContext::families`].

use std::cell::RefCell;

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping};

use crate::measure::{LineMetrics, ParagraphLayout, TextMeasurer, WrappedLine};

/// Default ratio of line height to font size used when laying out text.
const LINE_HEIGHT_RATIO: f32 = 1.2;

/// Information about one font family available on the system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontFamily {
    pub name: String,
    /// Distinct styles available (e.g. `"Regular"`, `"Bold"`, `"Italic"`).
    pub styles: Vec<String>,
    pub monospaced: bool,
}

/// Owns the font system and provides measurement + discovery. Construction loads system
/// fonts and is therefore relatively expensive; create one and reuse it.
pub struct FontContext {
    font_system: RefCell<FontSystem>,
}

impl Default for FontContext {
    fn default() -> Self {
        Self::new()
    }
}

impl FontContext {
    /// Create a context with the system fonts loaded.
    pub fn new() -> Self {
        FontContext {
            font_system: RefCell::new(FontSystem::new()),
        }
    }

    /// List the available font families, sorted by name, each with its distinct styles.
    pub fn families(&self) -> Vec<FontFamily> {
        use std::collections::BTreeMap;
        let fs = self.font_system.borrow();
        let db = fs.db();
        let mut map: BTreeMap<String, (std::collections::BTreeSet<String>, bool)> = BTreeMap::new();
        for face in db.faces() {
            let family = face
                .families
                .first()
                .map(|(name, _)| name.clone())
                .unwrap_or_else(|| face.post_script_name.clone());
            let style = describe_style(face);
            let entry = map.entry(family).or_default();
            entry.0.insert(style);
            entry.1 |= face.monospaced;
        }
        map.into_iter()
            .map(|(name, (styles, monospaced))| FontFamily {
                name,
                styles: styles.into_iter().collect(),
                monospaced,
            })
            .collect()
    }

    /// True if a family with the given name is available (case-insensitive).
    pub fn has_family(&self, name: &str) -> bool {
        let fs = self.font_system.borrow();
        let found = fs.db().faces().any(|face| {
            face.families
                .iter()
                .any(|(fam, _)| fam.eq_ignore_ascii_case(name))
        });
        found
    }

    fn build_buffer(&self, text: &str, wrap_width: Option<f32>, family: &str, size: f64) -> Buffer {
        let mut fs = self.font_system.borrow_mut();
        let metrics = Metrics::new(size as f32, size as f32 * LINE_HEIGHT_RATIO);
        let mut buffer = Buffer::new(&mut fs, metrics);
        buffer.set_size(&mut fs, wrap_width, None);
        let attrs = Attrs::new().family(family_of(family));
        buffer.set_text(&mut fs, text, attrs, Shaping::Advanced);
        buffer.shape_until_scroll(&mut fs, false);
        buffer
    }
}

/// Map a CSS-ish family string to a cosmic-text `Family`. Generic names map to the generic
/// families; anything else is treated as a specific family name.
fn family_of(name: &str) -> Family<'_> {
    match name.trim().to_ascii_lowercase().as_str() {
        "sans-serif" | "sans" => Family::SansSerif,
        "serif" => Family::Serif,
        "monospace" | "mono" => Family::Monospace,
        "cursive" => Family::Cursive,
        "fantasy" => Family::Fantasy,
        _ => Family::Name(name),
    }
}

fn describe_style(face: &cosmic_text::fontdb::FaceInfo) -> String {
    use cosmic_text::fontdb::Style;
    let style = match face.style {
        Style::Normal => "Regular",
        Style::Italic => "Italic",
        Style::Oblique => "Oblique",
    };
    let weight = face.weight.0;
    if weight == 400 {
        style.to_string()
    } else if weight == 700 && style == "Regular" {
        "Bold".to_string()
    } else if weight == 700 {
        format!("Bold {style}")
    } else {
        format!("{style} ({weight})")
    }
}

impl TextMeasurer for FontContext {
    fn measure_line(&self, text: &str, font_family: &str, font_size: f64) -> LineMetrics {
        let line = text.replace(['\n', '\r'], " ");
        let buffer = self.build_buffer(&line, None, font_family, font_size);
        let line_height = font_size as f32 * LINE_HEIGHT_RATIO;
        let mut width = 0.0f32;
        let mut ascent = font_size as f32 * 0.8;
        let mut found = false;
        for run in buffer.layout_runs() {
            width = width.max(run.line_w);
            if !found {
                // First run's baseline (line_y) measured from the line top approximates ascent.
                ascent = run.line_y - run.line_top;
                found = true;
            }
        }
        let descent = (line_height - ascent).max(0.0);
        LineMetrics {
            width: width as f64,
            ascent: ascent as f64,
            descent: descent as f64,
        }
    }

    fn layout_paragraph(
        &self,
        text: &str,
        max_width: f64,
        font_family: &str,
        font_size: f64,
    ) -> ParagraphLayout {
        let buffer = self.build_buffer(text, Some(max_width as f32), font_family, font_size);
        let line_height = (font_size as f32 * LINE_HEIGHT_RATIO) as f64;
        let mut lines: Vec<WrappedLine> = Vec::new();
        for run in buffer.layout_runs() {
            // Reconstruct this visual line's text from its glyph byte ranges into run.text.
            let line_text = match (run.glyphs.first(), run.glyphs.last()) {
                (Some(first), Some(last)) => {
                    let start = first.start.min(last.start);
                    let end = first.end.max(last.end);
                    run.text.get(start..end).unwrap_or("").trim().to_string()
                }
                _ => String::new(),
            };
            lines.push(WrappedLine {
                text: line_text,
                width: run.line_w as f64,
            });
        }
        if lines.is_empty() {
            lines.push(WrappedLine {
                text: String::new(),
                width: 0.0,
            });
        }
        let width = lines
            .iter()
            .map(|l| l.width)
            .fold(0.0, f64::max)
            .min(max_width);
        ParagraphLayout {
            height: line_height * lines.len() as f64,
            width,
            line_height,
            lines,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests load system fonts. They assert structural invariants rather than exact
    // pixel values, so they pass regardless of which fonts a machine happens to have.

    #[test]
    fn measures_a_line() {
        let ctx = FontContext::new();
        let short = ctx.measure_line("i", "sans-serif", 16.0);
        let long = ctx.measure_line("wwwwwwwwww", "sans-serif", 16.0);
        assert!(long.width > short.width, "longer text should be wider");
        assert!(short.height() > 0.0);
        assert!(short.ascent > 0.0);
    }

    #[test]
    fn empty_line_has_height() {
        let ctx = FontContext::new();
        let m = ctx.measure_line("", "sans-serif", 16.0);
        assert!(m.height() > 0.0);
        assert_eq!(m.width, 0.0);
    }

    #[test]
    fn paragraph_wraps() {
        let ctx = FontContext::new();
        let narrow = ctx.measure_paragraph("the quick brown fox jumps", 40.0, "sans-serif", 14.0);
        let wide = ctx.measure_paragraph("the quick brown fox jumps", 1000.0, "sans-serif", 14.0);
        assert!(
            narrow.lines > wide.lines,
            "narrower width should wrap to more lines"
        );
        assert_eq!(wide.lines, 1);
        assert!(narrow.width <= 40.0);
    }

    #[test]
    fn layout_paragraph_recovers_words() {
        let ctx = FontContext::new();
        let p = ctx.layout_paragraph("the quick brown fox", 60.0, "sans-serif", 14.0);
        let joined = p
            .lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        for word in ["the", "quick", "brown", "fox"] {
            assert!(joined.contains(word), "missing {word} in {joined:?}");
        }
    }

    #[test]
    fn lists_some_families() {
        let ctx = FontContext::new();
        let fams = ctx.families();
        // CI/headless machines still ship at least one font via fontconfig/embedded fallback.
        assert!(!fams.is_empty(), "expected at least one system font family");
        assert!(fams.iter().all(|f| !f.name.is_empty()));
    }
}
