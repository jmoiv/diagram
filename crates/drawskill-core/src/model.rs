//! The typed diagram document.
//!
//! [`parse`](crate::parse) produces a [`Document`] from YAML (after variable/expression
//! resolution); [`layout`](crate::layout) consumes it. The model is a tree of [`Node`]s laid
//! out by a simple box model, plus a list of [`Connect`]ions that reference node ports by id.

use crate::draw::PathCmd;
use crate::geom::{Point, Size};
use crate::style::{Color, StylePatch};
use crate::symbols::Props;

/// A complete diagram.
#[derive(Debug, Clone)]
pub struct Document {
    pub canvas: Canvas,
    pub root: Node,
    pub connections: Vec<Connect>,
}

/// Canvas-level configuration.
#[derive(Debug, Clone)]
pub struct Canvas {
    /// Fixed width, or `None` to size to content.
    pub width: Option<f64>,
    /// Fixed height, or `None` to size to content.
    pub height: Option<f64>,
    /// Padding around the root content (applied when auto-sizing, and as an inset otherwise).
    pub padding: f64,
    /// Optional background fill.
    pub background: Option<Color>,
    /// Document-level style defaults inherited by the whole tree.
    pub base_style: StylePatch,
}

impl Default for Canvas {
    fn default() -> Self {
        Canvas {
            width: None,
            height: None,
            padding: 10.0,
            background: None,
            base_style: StylePatch::default(),
        }
    }
}

/// How a node is sized along one axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SizeSpec {
    /// Size to intrinsic content.
    Auto,
    /// A fixed size in user units.
    Fixed(f64),
    /// Grow to fill leftover space, sharing it by this weight (flex grow).
    Grow(f64),
}

impl Default for SizeSpec {
    fn default() -> Self {
        SizeSpec::Auto
    }
}

/// Per-side spacing.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Edges {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

impl Edges {
    pub fn all(v: f64) -> Self {
        Edges { top: v, right: v, bottom: v, left: v }
    }

    pub fn horizontal(&self) -> f64 {
        self.left + self.right
    }

    pub fn vertical(&self) -> f64 {
        self.top + self.bottom
    }
}

/// Cross-axis alignment of children within a container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
    /// Stretch children to fill the cross axis.
    Stretch,
}

/// Main-axis distribution of children within a container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Justify {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
}

/// Layout direction of a container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Lay children out left-to-right (`hbox`).
    Row,
    /// Lay children out top-to-bottom (`vbox`).
    Column,
    /// Stack children at the same origin (`box`), sized to the largest.
    Stack,
}

/// A node in the diagram tree.
#[derive(Debug, Clone)]
pub struct Node {
    /// Optional id, used as a connection target.
    pub id: Option<String>,
    /// Style overrides applied to this node and inherited by descendants.
    pub style: StylePatch,
    /// Sizing along the main/cross axes (interpreted by the parent container).
    pub width: SizeSpec,
    pub height: SizeSpec,
    /// Margin around the node.
    pub margin: Edges,
    /// Per-node cross-axis alignment override (falls back to the container's `align`).
    pub align_self: Option<Align>,
    pub kind: NodeKind,
}

impl Node {
    /// Construct a node with default layout attributes wrapping `kind`.
    pub fn new(kind: NodeKind) -> Self {
        Node {
            id: None,
            style: StylePatch::default(),
            width: SizeSpec::Auto,
            height: SizeSpec::Auto,
            margin: Edges::default(),
            align_self: None,
            kind,
        }
    }
}

/// The variant-specific content of a node.
#[derive(Debug, Clone)]
pub enum NodeKind {
    Container(Container),
    Symbol(SymbolNode),
    Text(TextNode),
    Line(LineShape),
    Rect(RectShape),
    Circle(CircleShape),
    Path(PathShape),
    /// Empty, optionally flexible, space.
    Spacer,
}

/// A layout container.
#[derive(Debug, Clone)]
pub struct Container {
    pub direction: Direction,
    pub children: Vec<Node>,
    /// Gap between adjacent children along the main axis.
    pub gap: f64,
    pub padding: Edges,
    pub align: Align,
    pub justify: Justify,
}

/// An instance of a plugin symbol.
#[derive(Debug, Clone)]
pub struct SymbolNode {
    /// Qualified `"plugin.name"`.
    pub name: String,
    pub props: Props,
}

/// A text leaf. If `wrap` is set, the text is laid out as a wrapped paragraph at that width;
/// otherwise it's a single line.
#[derive(Debug, Clone)]
pub struct TextNode {
    pub text: String,
    pub wrap: Option<f64>,
}

/// A raw line between two local-space points.
#[derive(Debug, Clone)]
pub struct LineShape {
    pub a: Point,
    pub b: Point,
}

/// A raw rectangle drawn at the node's origin.
#[derive(Debug, Clone)]
pub struct RectShape {
    pub size: Size,
    pub rx: f64,
}

/// A raw circle. Its bounding box is `2*radius` square.
#[derive(Debug, Clone)]
pub struct CircleShape {
    pub radius: f64,
}

/// A raw path in local-space coordinates.
#[derive(Debug, Clone)]
pub struct PathShape {
    pub cmds: Vec<PathCmd>,
}

/// Connection routing style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Routing {
    #[default]
    Straight,
    /// Right-angle (Manhattan) routing.
    Orthogonal,
}

/// Which endpoints get arrowheads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Arrow {
    None,
    #[default]
    End,
    Both,
}

/// A reference to a connection endpoint: a node id and optional named port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortRef {
    pub node: String,
    pub port: Option<String>,
}

/// A connection between two ports/nodes.
#[derive(Debug, Clone)]
pub struct Connect {
    pub from: PortRef,
    pub to: PortRef,
    pub style: StylePatch,
    pub routing: Routing,
    pub arrow: Arrow,
    /// Optional label drawn at the midpoint.
    pub label: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_helpers() {
        let e = Edges::all(4.0);
        assert_eq!(e.horizontal(), 8.0);
        assert_eq!(e.vertical(), 8.0);
    }

    #[test]
    fn node_defaults() {
        let n = Node::new(NodeKind::Spacer);
        assert_eq!(n.width, SizeSpec::Auto);
        assert!(n.id.is_none());
        assert!(n.align_self.is_none());
    }

    #[test]
    fn sizespec_default_is_auto() {
        assert_eq!(SizeSpec::default(), SizeSpec::Auto);
    }
}
