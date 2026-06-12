//! drawskill core engine.
//!
//! Pipeline: YAML text -> [`parse`] (with [`expr`] variable/expression resolution) ->
//! [`model`] document -> [`layout`] (two-pass box model, using [`text`] measurement and
//! [`symbols`]) -> positioned scene -> [`render`] (SVG) -> [`output`] (SVG/PNG/PDF).

pub mod draw;
pub mod error;
pub mod geom;
pub mod layout;
pub mod measure;
pub mod model;
pub mod output;
pub mod render;
pub mod style;
pub mod symbols;
pub mod text;

pub use error::{Error, Result};
