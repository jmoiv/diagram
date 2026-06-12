//! Text-measurement abstraction.
//!
//! The layout engine, the expression functions, and symbols all need to know how big a
//! string renders, and the renderer needs the actual wrapped lines (SVG does not wrap text).
//! They depend only on the [`TextMeasurer`] trait defined here, not on the concrete font
//! backend. The real implementation lives in the [`crate::text`] module (backed by `fontdb` +
//! `cosmic-text`); [`BasicMeasurer`] is a font-free approximation used as a fallback and in
//! unit tests so layout can be tested without loading fonts.

/// Metrics for a single line of text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineMetrics {
    pub width: f64,
    pub ascent: f64,
    pub descent: f64,
}

impl LineMetrics {
    /// Total line height (ascent + descent).
    pub fn height(&self) -> f64 {
        self.ascent + self.descent
    }
}

/// One visual line produced by wrapping a paragraph.
#[derive(Debug, Clone, PartialEq)]
pub struct WrappedLine {
    pub text: String,
    pub width: f64,
}

/// The result of wrapping a paragraph to a maximum width.
#[derive(Debug, Clone, PartialEq)]
pub struct ParagraphLayout {
    pub lines: Vec<WrappedLine>,
    /// Width of the widest line (<= the requested max width).
    pub width: f64,
    /// Total height of all lines.
    pub height: f64,
    /// Height of a single line (line advance).
    pub line_height: f64,
}

/// Compact paragraph metrics (no per-line text).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParagraphMetrics {
    pub width: f64,
    pub height: f64,
    pub lines: usize,
}

/// Something that can measure and wrap text. Implementors should be consistent with how the
/// same text is ultimately rendered, so reserved layout space matches the drawn glyphs.
pub trait TextMeasurer {
    /// Measure a single line (no wrapping; newlines are treated as spaces).
    fn measure_line(&self, text: &str, font_family: &str, font_size: f64) -> LineMetrics;

    /// Wrap a paragraph to at most `max_width`, returning each visual line and its width.
    fn layout_paragraph(
        &self,
        text: &str,
        max_width: f64,
        font_family: &str,
        font_size: f64,
    ) -> ParagraphLayout;

    /// Compact metrics for a wrapped paragraph (derived from [`Self::layout_paragraph`]).
    fn measure_paragraph(
        &self,
        text: &str,
        max_width: f64,
        font_family: &str,
        font_size: f64,
    ) -> ParagraphMetrics {
        let p = self.layout_paragraph(text, max_width, font_family, font_size);
        ParagraphMetrics { width: p.width, height: p.height, lines: p.lines.len().max(1) }
    }
}

/// A font-free approximation: every glyph is treated as a fixed fraction of the font size
/// wide. Deterministic and dependency-free, so it's ideal for unit tests and as a fallback
/// when no fonts are available. Not accurate enough for production rendering.
#[derive(Debug, Clone, Copy)]
pub struct BasicMeasurer {
    /// Average glyph advance as a fraction of font size.
    pub advance_ratio: f64,
    /// Ascent as a fraction of font size.
    pub ascent_ratio: f64,
    /// Descent as a fraction of font size.
    pub descent_ratio: f64,
    /// Line height as a fraction of font size.
    pub line_height_ratio: f64,
}

impl Default for BasicMeasurer {
    fn default() -> Self {
        BasicMeasurer {
            advance_ratio: 0.6,
            ascent_ratio: 0.8,
            descent_ratio: 0.2,
            line_height_ratio: 1.2,
        }
    }
}

impl BasicMeasurer {
    fn line_width(&self, text: &str, font_size: f64) -> f64 {
        text.chars().count() as f64 * self.advance_ratio * font_size
    }
}

impl TextMeasurer for BasicMeasurer {
    fn measure_line(&self, text: &str, _font_family: &str, font_size: f64) -> LineMetrics {
        let line = text.replace(['\n', '\r'], " ");
        LineMetrics {
            width: self.line_width(&line, font_size),
            ascent: self.ascent_ratio * font_size,
            descent: self.descent_ratio * font_size,
        }
    }

    fn layout_paragraph(
        &self,
        text: &str,
        max_width: f64,
        _font_family: &str,
        font_size: f64,
    ) -> ParagraphLayout {
        let line_height = self.line_height_ratio * font_size;
        let space_w = self.advance_ratio * font_size;
        let mut lines: Vec<WrappedLine> = Vec::new();

        for hard_line in text.split('\n') {
            let mut cur = String::new();
            let mut cur_w = 0.0f64;
            for word in hard_line.split_whitespace() {
                let w = self.line_width(word, font_size);
                if cur.is_empty() {
                    cur.push_str(word);
                    cur_w = w;
                } else if cur_w + space_w + w > max_width {
                    lines.push(WrappedLine { text: std::mem::take(&mut cur), width: cur_w });
                    cur.push_str(word);
                    cur_w = w;
                } else {
                    cur.push(' ');
                    cur.push_str(word);
                    cur_w += space_w + w;
                }
            }
            lines.push(WrappedLine { text: cur, width: cur_w });
        }
        if lines.is_empty() {
            lines.push(WrappedLine { text: String::new(), width: 0.0 });
        }
        let width = lines.iter().map(|l| l.width).fold(0.0, f64::max).min(max_width);
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

    #[test]
    fn line_width_scales_with_length_and_size() {
        let m = BasicMeasurer::default();
        let a = m.measure_line("ab", "sans", 10.0);
        let b = m.measure_line("abcd", "sans", 10.0);
        assert!((b.width - 2.0 * a.width).abs() < 1e-9);
        assert!((a.height() - 10.0).abs() < 1e-9);
    }

    #[test]
    fn paragraph_wraps_and_preserves_words() {
        let m = BasicMeasurer::default();
        let p = m.layout_paragraph("hello world hello world", 70.0, "sans", 10.0);
        assert!(p.lines.len() >= 2, "expected wrapping, got {} lines", p.lines.len());
        assert!(p.width <= 70.0);
        // Every word survives wrapping.
        let joined = p.lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join(" ");
        assert_eq!(joined.split_whitespace().count(), 4);
    }

    #[test]
    fn paragraph_single_line_when_fits() {
        let m = BasicMeasurer::default();
        let p = m.layout_paragraph("hi there", 1000.0, "sans", 10.0);
        assert_eq!(p.lines.len(), 1);
        assert_eq!(p.lines[0].text, "hi there");
    }

    #[test]
    fn hard_newlines_force_breaks() {
        let m = BasicMeasurer::default();
        let p = m.layout_paragraph("a\nb\nc", 1000.0, "sans", 10.0);
        assert_eq!(p.lines.len(), 3);
    }

    #[test]
    fn metrics_derive_from_layout() {
        let m = BasicMeasurer::default();
        let pm = m.measure_paragraph("a\nb", 1000.0, "sans", 10.0);
        assert_eq!(pm.lines, 2);
        assert!((pm.height - 24.0).abs() < 1e-9); // 2 lines * 1.2 * 10
    }
}
