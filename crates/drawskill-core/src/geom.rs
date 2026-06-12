//! Basic 2D geometry primitives used throughout the engine.
//!
//! All coordinates are in user-space units (which map 1:1 to SVG/PDF points and to
//! device pixels at the default scale). The origin is the top-left corner, with `x`
//! increasing to the right and `y` increasing downward, matching SVG conventions.

/// A point in user space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    pub fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }

    /// Translate the point by `(dx, dy)`.
    pub fn translate(self, dx: f64, dy: f64) -> Point {
        Point {
            x: self.x + dx,
            y: self.y + dy,
        }
    }
}

/// A width/height pair.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

impl Size {
    pub const ZERO: Size = Size {
        width: 0.0,
        height: 0.0,
    };

    pub fn new(width: f64, height: f64) -> Self {
        Size { width, height }
    }
}

/// An axis-aligned rectangle, described by its top-left corner and size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    pub fn from_origin_size(origin: Point, size: Size) -> Self {
        Rect {
            x: origin.x,
            y: origin.y,
            width: size.width,
            height: size.height,
        }
    }

    pub fn origin(&self) -> Point {
        Point::new(self.x, self.y)
    }

    pub fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }

    pub fn right(&self) -> f64 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.height
    }

    pub fn center(&self) -> Point {
        Point::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// Return a copy shrunk on every side by `pad`.
    pub fn inset(&self, pad: f64) -> Rect {
        Rect::new(
            self.x + pad,
            self.y + pad,
            (self.width - 2.0 * pad).max(0.0),
            (self.height - 2.0 * pad).max(0.0),
        )
    }

    /// Return a copy translated by `(dx, dy)`.
    pub fn translate(&self, dx: f64, dy: f64) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.width, self.height)
    }

    /// True if `p` lies inside (or on the edge of) this rectangle.
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x <= self.right() && p.y >= self.y && p.y <= self.bottom()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_derived_edges() {
        let r = Rect::new(10.0, 20.0, 30.0, 40.0);
        assert_eq!(r.right(), 40.0);
        assert_eq!(r.bottom(), 60.0);
        assert_eq!(r.center(), Point::new(25.0, 40.0));
        assert_eq!(r.origin(), Point::new(10.0, 20.0));
        assert_eq!(r.size(), Size::new(30.0, 40.0));
    }

    #[test]
    fn rect_inset_clamps_to_zero() {
        let r = Rect::new(0.0, 0.0, 4.0, 4.0);
        let i = r.inset(3.0); // would be negative without clamping
        assert_eq!(i.width, 0.0);
        assert_eq!(i.height, 0.0);
    }

    #[test]
    fn rect_contains() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(r.contains(Point::new(5.0, 5.0)));
        assert!(r.contains(Point::new(0.0, 0.0)));
        assert!(!r.contains(Point::new(11.0, 5.0)));
    }

    #[test]
    fn point_translate() {
        assert_eq!(
            Point::new(1.0, 2.0).translate(3.0, 4.0),
            Point::new(4.0, 6.0)
        );
    }
}
