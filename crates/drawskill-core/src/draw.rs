//! Drawing primitives and the [`Painter`] symbols use to emit them.
//!
//! Symbols and the renderer never produce SVG text directly; instead they emit a flat list
//! of [`Primitive`]s in user space. The `render` module turns primitives into SVG, which is
//! then converted to PNG/PDF. Keeping primitives backend-agnostic makes symbols easy to unit
//! test (assert on the emitted primitives) and keeps a single source of truth for geometry.

use crate::geom::{Point, Rect};
use crate::style::{Color, TextAnchor};

/// Stroke styling for a primitive.
#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    pub color: Color,
    pub width: f64,
}

impl Stroke {
    pub fn new(color: Color, width: f64) -> Self {
        Stroke { color, width }
    }
}

/// Fill + stroke styling for a closed shape.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeStyle {
    pub fill: Color,
    pub stroke: Color,
    pub stroke_width: f64,
}

impl ShapeStyle {
    pub fn new(fill: Color, stroke: Color, stroke_width: f64) -> Self {
        ShapeStyle { fill, stroke, stroke_width }
    }

    /// An outline-only shape (no fill).
    pub fn outline(stroke: Color, stroke_width: f64) -> Self {
        ShapeStyle { fill: Color::NONE, stroke, stroke_width }
    }
}

/// Text styling.
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    pub color: Color,
    pub font_family: String,
    pub font_size: f64,
    pub anchor: TextAnchor,
}

/// A single SVG path command, in absolute user-space coordinates.
#[derive(Debug, Clone, PartialEq)]
pub enum PathCmd {
    MoveTo(Point),
    LineTo(Point),
    /// Cubic Bézier curve: two control points then the endpoint.
    CurveTo(Point, Point, Point),
    /// Quadratic Bézier curve: one control point then the endpoint.
    QuadTo(Point, Point),
    Close,
}

/// A primitive drawing operation in user space. The `y` of a [`Primitive::Text`] is the
/// text baseline.
#[derive(Debug, Clone, PartialEq)]
pub enum Primitive {
    Line { a: Point, b: Point, stroke: Stroke },
    Polyline { points: Vec<Point>, stroke: Stroke },
    Rect { rect: Rect, rx: f64, style: ShapeStyle },
    Circle { center: Point, radius: f64, style: ShapeStyle },
    Path { cmds: Vec<PathCmd>, style: ShapeStyle },
    /// `pos.y` is the baseline. Width/measurement is handled by the layout engine.
    Text { pos: Point, text: String, style: TextStyle },
}

impl Primitive {
    /// Return a copy translated by `(dx, dy)`. Used to move a symbol's locally-drawn
    /// primitives into their final position on the canvas.
    pub fn translated(&self, dx: f64, dy: f64) -> Primitive {
        let p = |pt: &Point| pt.translate(dx, dy);
        match self {
            Primitive::Line { a, b, stroke } => {
                Primitive::Line { a: p(a), b: p(b), stroke: stroke.clone() }
            }
            Primitive::Polyline { points, stroke } => Primitive::Polyline {
                points: points.iter().map(p).collect(),
                stroke: stroke.clone(),
            },
            Primitive::Rect { rect, rx, style } => Primitive::Rect {
                rect: rect.translate(dx, dy),
                rx: *rx,
                style: style.clone(),
            },
            Primitive::Circle { center, radius, style } => Primitive::Circle {
                center: p(center),
                radius: *radius,
                style: style.clone(),
            },
            Primitive::Path { cmds, style } => Primitive::Path {
                cmds: cmds
                    .iter()
                    .map(|c| match c {
                        PathCmd::MoveTo(pt) => PathCmd::MoveTo(p(pt)),
                        PathCmd::LineTo(pt) => PathCmd::LineTo(p(pt)),
                        PathCmd::CurveTo(c1, c2, e) => PathCmd::CurveTo(p(c1), p(c2), p(e)),
                        PathCmd::QuadTo(c1, e) => PathCmd::QuadTo(p(c1), p(e)),
                        PathCmd::Close => PathCmd::Close,
                    })
                    .collect(),
                style: style.clone(),
            },
            Primitive::Text { pos, text, style } => Primitive::Text {
                pos: p(pos),
                text: text.clone(),
                style: style.clone(),
            },
        }
    }
}

/// Collects [`Primitive`]s emitted by a symbol's `draw` method (or by the renderer).
///
/// Coordinates passed to a `Painter` handed to a symbol are in the symbol's *local* space,
/// where the symbol's bounding box origin is whatever `bounds` the layout engine provides.
/// Symbols are encouraged to draw relative to that `bounds`.
#[derive(Debug, Default, Clone)]
pub struct Painter {
    primitives: Vec<Primitive>,
}

impl Painter {
    pub fn new() -> Self {
        Painter { primitives: Vec::new() }
    }

    pub fn line(&mut self, a: Point, b: Point, stroke: Stroke) {
        self.primitives.push(Primitive::Line { a, b, stroke });
    }

    pub fn polyline(&mut self, points: Vec<Point>, stroke: Stroke) {
        self.primitives.push(Primitive::Polyline { points, stroke });
    }

    pub fn rect(&mut self, rect: Rect, rx: f64, style: ShapeStyle) {
        self.primitives.push(Primitive::Rect { rect, rx, style });
    }

    pub fn circle(&mut self, center: Point, radius: f64, style: ShapeStyle) {
        self.primitives.push(Primitive::Circle { center, radius, style });
    }

    pub fn path(&mut self, cmds: Vec<PathCmd>, style: ShapeStyle) {
        self.primitives.push(Primitive::Path { cmds, style });
    }

    pub fn text(&mut self, pos: Point, text: impl Into<String>, style: TextStyle) {
        self.primitives.push(Primitive::Text { pos, text: text.into(), style });
    }

    /// Number of primitives emitted so far.
    pub fn len(&self) -> usize {
        self.primitives.len()
    }

    pub fn is_empty(&self) -> bool {
        self.primitives.is_empty()
    }

    /// Borrow the emitted primitives (useful in tests).
    pub fn primitives(&self) -> &[Primitive] {
        &self.primitives
    }

    /// Consume the painter, returning the collected primitives.
    pub fn into_primitives(self) -> Vec<Primitive> {
        self.primitives
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn painter_collects_primitives() {
        let mut p = Painter::new();
        assert!(p.is_empty());
        p.line(Point::ZERO, Point::new(10.0, 0.0), Stroke::new(Color::BLACK, 1.0));
        p.circle(Point::new(5.0, 5.0), 3.0, ShapeStyle::outline(Color::BLACK, 1.0));
        assert_eq!(p.len(), 2);
        assert!(!p.is_empty());
        assert_eq!(p.into_primitives().len(), 2);
    }

    #[test]
    fn translate_moves_all_geometry() {
        let line = Primitive::Line {
            a: Point::new(1.0, 1.0),
            b: Point::new(2.0, 2.0),
            stroke: Stroke::new(Color::BLACK, 1.0),
        };
        let moved = line.translated(10.0, 20.0);
        match moved {
            Primitive::Line { a, b, .. } => {
                assert_eq!(a, Point::new(11.0, 21.0));
                assert_eq!(b, Point::new(12.0, 22.0));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn translate_path_commands() {
        let path = Primitive::Path {
            cmds: vec![
                PathCmd::MoveTo(Point::new(0.0, 0.0)),
                PathCmd::CurveTo(
                    Point::new(1.0, 1.0),
                    Point::new(2.0, 2.0),
                    Point::new(3.0, 3.0),
                ),
                PathCmd::Close,
            ],
            style: ShapeStyle::outline(Color::BLACK, 1.0),
        };
        let moved = path.translated(5.0, 5.0);
        if let Primitive::Path { cmds, .. } = moved {
            assert_eq!(cmds[0], PathCmd::MoveTo(Point::new(5.0, 5.0)));
            assert_eq!(
                cmds[1],
                PathCmd::CurveTo(
                    Point::new(6.0, 6.0),
                    Point::new(7.0, 7.0),
                    Point::new(8.0, 8.0)
                )
            );
            assert_eq!(cmds[2], PathCmd::Close);
        } else {
            panic!("wrong variant");
        }
    }
}
