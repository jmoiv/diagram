//! Convert a [`Scene`] to the supported output formats.
//!
//! SVG is generated directly by [`crate::render`]. PNG and PDF are produced by parsing that
//! SVG with `usvg` (using system fonts, consistent with measurement) and converting it:
//! PNG via `resvg`/`tiny-skia`, PDF via `svg2pdf` (vector, no rasterization).

use resvg::tiny_skia;
use resvg::usvg;

use crate::error::{Error, Result};
use crate::layout::Scene;
use crate::render::to_svg;

/// A supported output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Svg,
    Png,
    Pdf,
}

impl Format {
    /// Infer the format from a file extension (case-insensitive).
    pub fn from_extension(ext: &str) -> Option<Format> {
        match ext.trim_start_matches('.').to_ascii_lowercase().as_str() {
            "svg" => Some(Format::Svg),
            "png" => Some(Format::Png),
            "pdf" => Some(Format::Pdf),
            _ => None,
        }
    }

    /// Infer the format from a file path's extension.
    pub fn from_path(path: &std::path::Path) -> Option<Format> {
        path.extension()
            .and_then(|e| e.to_str())
            .and_then(Format::from_extension)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Format::Svg => "svg",
            Format::Png => "png",
            Format::Pdf => "pdf",
        }
    }
}

/// Render a scene to the requested format, returning the encoded bytes.
pub fn render(scene: &Scene, format: Format, png_scale: f32) -> Result<Vec<u8>> {
    let svg = to_svg(scene);
    match format {
        Format::Svg => Ok(svg.into_bytes()),
        Format::Png => svg_to_png(&svg, png_scale),
        Format::Pdf => svg_to_pdf(&svg),
    }
}

/// Just the SVG string (no parsing/rasterization).
pub fn render_svg(scene: &Scene) -> String {
    to_svg(scene)
}

/// Parse an SVG document into a usvg tree using system fonts.
fn parse_tree(svg: &str) -> Result<usvg::Tree> {
    let mut options = usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    usvg::Tree::from_str(svg, &options).map_err(|e| Error::Render(format!("SVG parse failed: {e}")))
}

/// Rasterize an SVG string to PNG at the given scale.
pub fn svg_to_png(svg: &str, scale: f32) -> Result<Vec<u8>> {
    let tree = parse_tree(svg)?;
    let size = tree.size();
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let width = (size.width() * scale).ceil().max(1.0) as u32;
    let height = (size.height() * scale).ceil().max(1.0) as u32;

    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| Error::Render(format!("invalid pixmap size {width}x{height}")))?;
    let transform = tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    pixmap
        .encode_png()
        .map_err(|e| Error::Render(format!("PNG encoding failed: {e}")))
}

/// Convert an SVG string to a single-page PDF.
pub fn svg_to_pdf(svg: &str) -> Result<Vec<u8>> {
    // svg2pdf re-exports the same usvg version, so parse with its options for type parity.
    let mut options = svg2pdf::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = svg2pdf::usvg::Tree::from_str(svg, &options)
        .map_err(|e| Error::Render(format!("SVG parse failed: {e}")))?;
    svg2pdf::to_pdf(
        &tree,
        svg2pdf::ConversionOptions::default(),
        svg2pdf::PageOptions::default(),
    )
    .map_err(|e| Error::Render(format!("PDF conversion failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::{Primitive, ShapeStyle};
    use crate::geom::{Point, Size};
    use crate::style::Color;

    fn sample_scene() -> Scene {
        Scene {
            size: Size::new(80.0, 40.0),
            background: Some(Color::WHITE),
            primitives: vec![Primitive::Circle {
                center: Point::new(40.0, 20.0),
                radius: 15.0,
                style: ShapeStyle::new(Color::rgb(200, 30, 30), Color::BLACK, 2.0),
            }],
        }
    }

    #[test]
    fn format_inference() {
        assert_eq!(Format::from_extension("PNG"), Some(Format::Png));
        assert_eq!(Format::from_extension(".pdf"), Some(Format::Pdf));
        assert_eq!(Format::from_extension("svg"), Some(Format::Svg));
        assert_eq!(Format::from_extension("txt"), None);
        assert_eq!(
            Format::from_path(std::path::Path::new("/tmp/a.svg")),
            Some(Format::Svg)
        );
    }

    #[test]
    fn renders_svg_bytes() {
        let bytes = render(&sample_scene(), Format::Svg, 1.0).unwrap();
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("<svg"));
        assert!(s.contains("<circle"));
    }

    #[test]
    fn renders_png_with_header_and_size() {
        let bytes = render(&sample_scene(), Format::Png, 2.0).unwrap();
        // PNG magic number.
        assert_eq!(
            &bytes[0..8],
            &[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n']
        );
        // IHDR width/height are big-endian u32 at offsets 16 and 20. Scale 2 => 160x80.
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        assert_eq!((w, h), (160, 80));
    }

    #[test]
    fn renders_pdf_with_header() {
        let bytes = render(&sample_scene(), Format::Pdf, 1.0).unwrap();
        assert_eq!(&bytes[0..5], b"%PDF-");
        assert!(bytes.len() > 100, "PDF unexpectedly tiny");
    }
}
