//! drawskill core engine.
//!
//! Pipeline: YAML text -> [`parse`] (with [`expr`] variable/expression resolution) ->
//! [`model`] document -> [`layout`] (two-pass box model, using [`text`] measurement and
//! [`symbols`]) -> positioned scene -> [`render`] (SVG) -> [`output`] (SVG/PNG/PDF).

pub mod draw;
pub mod error;
pub mod expr;
pub mod geom;
pub mod layout;
pub mod measure;
pub mod model;
pub mod output;
pub mod parse;
pub mod render;
pub mod style;
pub mod symbols;
pub mod text;

pub use error::{Error, Result};

use measure::TextMeasurer;
use output::Format;
use symbols::Registry;

/// End-to-end: parse a YAML diagram, lay it out against `registry`, and render it to `format`.
///
/// `png_scale` is only used for PNG output (1.0 = one user unit per pixel). This is the
/// one-call path the CLI uses; each stage is also available individually.
pub fn render_to_bytes(
    source: &str,
    registry: &Registry,
    measurer: &dyn TextMeasurer,
    format: Format,
    png_scale: f32,
) -> Result<Vec<u8>> {
    let document = parse::parse_document(source, measurer)?;
    let scene = layout::layout_document(&document, registry, measurer)?;
    output::render(&scene, format, png_scale)
}
