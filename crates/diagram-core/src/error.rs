//! Error type for the engine.

use std::fmt;

/// Errors produced while parsing, laying out, or rendering a diagram.
#[derive(Debug)]
pub enum Error {
    /// The YAML document could not be parsed.
    Yaml(String),
    /// A `${...}` expression failed to evaluate.
    Expr(String),
    /// The document structure was invalid (e.g. unknown node type, bad property).
    Parse(String),
    /// A referenced symbol (`plugin.name`) does not exist in the registry.
    UnknownSymbol(String),
    /// A `connect` referenced a port that doesn't exist.
    UnknownPort(String),
    /// Layout failed (e.g. an impossible constraint).
    Layout(String),
    /// Rendering or output conversion failed.
    Render(String),
    /// An I/O error occurred (reading input, writing output).
    Io(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Yaml(m) => write!(f, "YAML error: {m}"),
            Error::Expr(m) => write!(f, "expression error: {m}"),
            Error::Parse(m) => write!(f, "document error: {m}"),
            Error::UnknownSymbol(m) => write!(f, "unknown symbol: {m}"),
            Error::UnknownPort(m) => write!(f, "unknown port: {m}"),
            Error::Layout(m) => write!(f, "layout error: {m}"),
            Error::Render(m) => write!(f, "render error: {m}"),
            Error::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

/// Convenient result alias.
pub type Result<T> = std::result::Result<T, Error>;
