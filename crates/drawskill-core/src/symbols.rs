//! The symbol plugin system.
//!
//! A *symbol* is a reusable, parametric piece of diagram art (a resistor, a DFD process, an
//! IC, …). A *plugin* is a named collection of symbols. Plugins are compile-time Rust types
//! implementing [`SymbolPlugin`]; they are registered into a [`Registry`], which resolves the
//! qualified `"plugin.symbol"` names used in diagrams.
//!
//! Symbols declare a [`PropertySpec`] schema (so `drawskill symbols describe` can report it,
//! and so values can be validated/defaulted), compute their intrinsic [`Size`] (optionally
//! measuring text), expose connection [`Port`]s, and draw themselves via a [`Painter`].

use std::collections::{BTreeMap, HashMap};

use crate::draw::Painter;
use crate::geom::{Point, Rect, Size};
use crate::measure::TextMeasurer;
use crate::style::Style;

/// The kind of a symbol property, used for validation and documentation.
#[derive(Debug, Clone, PartialEq)]
pub enum PropKind {
    Number,
    Text,
    Bool,
    /// A list of values (e.g. a list of pin names).
    List,
    /// One of a fixed set of string values.
    Enum(&'static [&'static str]),
}

/// Declares one property a symbol accepts.
#[derive(Debug, Clone)]
pub struct PropertySpec {
    pub name: &'static str,
    pub kind: PropKind,
    pub required: bool,
    pub default: Option<PropValue>,
    pub description: &'static str,
}

impl PropertySpec {
    pub fn optional(
        name: &'static str,
        kind: PropKind,
        default: PropValue,
        description: &'static str,
    ) -> Self {
        PropertySpec {
            name,
            kind,
            required: false,
            default: Some(default),
            description,
        }
    }

    pub fn required(name: &'static str, kind: PropKind, description: &'static str) -> Self {
        PropertySpec {
            name,
            kind,
            required: true,
            default: None,
            description,
        }
    }
}

/// A resolved property value.
#[derive(Debug, Clone, PartialEq)]
pub enum PropValue {
    Number(f64),
    Text(String),
    Bool(bool),
    List(Vec<PropValue>),
}

impl PropValue {
    pub fn text(s: impl Into<String>) -> Self {
        PropValue::Text(s.into())
    }
}

/// A resolved bag of property values for one symbol instance.
#[derive(Debug, Clone, Default)]
pub struct Props {
    values: HashMap<String, PropValue>,
}

impl Props {
    pub fn new() -> Self {
        Props::default()
    }

    pub fn insert(&mut self, name: impl Into<String>, value: PropValue) {
        self.values.insert(name.into(), value);
    }

    pub fn get(&self, name: &str) -> Option<&PropValue> {
        self.values.get(name)
    }

    /// Get a number, falling back to `default` if absent or the wrong type.
    pub fn number_or(&self, name: &str, default: f64) -> f64 {
        match self.values.get(name) {
            Some(PropValue::Number(n)) => *n,
            _ => default,
        }
    }

    /// Get a string slice, falling back to `default` if absent or the wrong type.
    pub fn text_or<'a>(&'a self, name: &str, default: &'a str) -> &'a str {
        match self.values.get(name) {
            Some(PropValue::Text(s)) => s,
            _ => default,
        }
    }

    /// Get a bool, falling back to `default`.
    pub fn bool_or(&self, name: &str, default: bool) -> bool {
        match self.values.get(name) {
            Some(PropValue::Bool(b)) => *b,
            _ => default,
        }
    }

    /// Get a list of strings (non-string items are skipped). Empty if absent.
    pub fn text_list(&self, name: &str) -> Vec<String> {
        match self.values.get(name) {
            Some(PropValue::List(items)) => items
                .iter()
                .filter_map(|v| match v {
                    PropValue::Text(s) => Some(s.clone()),
                    PropValue::Number(n) => Some(format!("{n}")),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Apply schema defaults for any property not explicitly set.
    pub fn with_defaults(mut self, schema: &[PropertySpec]) -> Self {
        for spec in schema {
            if !self.values.contains_key(spec.name) {
                if let Some(def) = &spec.default {
                    self.values.insert(spec.name.to_string(), def.clone());
                }
            }
        }
        self
    }
}

/// The outward direction a port faces, used when routing connections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

/// A named connection point on a symbol, in absolute canvas coordinates.
#[derive(Debug, Clone)]
pub struct Port {
    pub name: String,
    pub point: Point,
    pub dir: Dir,
}

impl Port {
    pub fn new(name: impl Into<String>, point: Point, dir: Dir) -> Self {
        Port {
            name: name.into(),
            point,
            dir,
        }
    }
}

/// A drawable, parametric symbol. Implemented by plugin authors.
pub trait Symbol: Send + Sync {
    /// The symbol's local name within its plugin (e.g. `"resistor"`).
    fn name(&self) -> &str;

    /// Human-readable one-line description for `symbols describe`.
    fn description(&self) -> &str {
        ""
    }

    /// The properties this symbol accepts.
    fn property_schema(&self) -> Vec<PropertySpec> {
        Vec::new()
    }

    /// The symbol's intrinsic size given its resolved properties. May measure text (e.g. an
    /// IC sizes itself to fit its pin labels). The layout engine uses this when the diagram
    /// doesn't pin an explicit size.
    fn measure(&self, props: &Props, measurer: &dyn TextMeasurer) -> Size;

    /// Connection ports, in absolute coordinates, given the final on-canvas `bounds`.
    fn ports(&self, bounds: Rect, props: &Props) -> Vec<Port>;

    /// Draw the symbol within `bounds`. `base_style` provides inherited colors/fonts the
    /// symbol may honor (most symbols draw in the stroke/text color).
    fn draw(
        &self,
        painter: &mut Painter,
        bounds: Rect,
        props: &Props,
        base_style: &Style,
        measurer: &dyn TextMeasurer,
    );
}

/// A named collection of symbols.
pub trait SymbolPlugin {
    /// The plugin id used as the qualifier in `"id.symbol"` names (e.g. `"schematic"`).
    fn id(&self) -> &str;

    /// One-line description of the plugin.
    fn description(&self) -> &str {
        ""
    }

    /// Construct the plugin's symbols.
    fn symbols(&self) -> Vec<Box<dyn Symbol>>;
}

/// Resolves `"plugin.symbol"` names to symbols and supports listing/describing them.
#[derive(Default)]
pub struct Registry {
    /// Qualified name -> symbol.
    symbols: HashMap<String, Box<dyn Symbol>>,
    /// Plugin id -> (description, sorted symbol names), for listing.
    plugins: BTreeMap<String, PluginInfo>,
}

struct PluginInfo {
    description: String,
    symbol_names: Vec<String>,
}

impl Registry {
    pub fn new() -> Self {
        Registry::default()
    }

    /// Register every symbol provided by `plugin` under `"<id>.<name>"`.
    pub fn register(&mut self, plugin: &dyn SymbolPlugin) {
        let id = plugin.id().to_string();
        let mut names = Vec::new();
        for sym in plugin.symbols() {
            let qualified = format!("{id}.{}", sym.name());
            names.push(sym.name().to_string());
            self.symbols.insert(qualified, sym);
        }
        names.sort();
        self.plugins.insert(
            id,
            PluginInfo {
                description: plugin.description().to_string(),
                symbol_names: names,
            },
        );
    }

    /// Look up a symbol by its qualified `"plugin.symbol"` name.
    pub fn get(&self, qualified: &str) -> Option<&dyn Symbol> {
        self.symbols.get(qualified).map(|b| b.as_ref())
    }

    /// All qualified symbol names, sorted.
    pub fn qualified_names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.symbols.keys().cloned().collect();
        v.sort();
        v
    }

    /// Plugins with their descriptions and symbol names, for `symbols list`.
    pub fn plugins(&self) -> Vec<(String, String, Vec<String>)> {
        self.plugins
            .iter()
            .map(|(id, info)| {
                (
                    id.clone(),
                    info.description.clone(),
                    info.symbol_names.clone(),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::Painter;
    use crate::measure::BasicMeasurer;
    use crate::style::Color;

    struct Dot;
    impl Symbol for Dot {
        fn name(&self) -> &str {
            "dot"
        }
        fn description(&self) -> &str {
            "a dot"
        }
        fn property_schema(&self) -> Vec<PropertySpec> {
            vec![PropertySpec::optional(
                "r",
                PropKind::Number,
                PropValue::Number(3.0),
                "radius",
            )]
        }
        fn measure(&self, props: &Props, _m: &dyn TextMeasurer) -> Size {
            let r = props.number_or("r", 3.0);
            Size::new(2.0 * r, 2.0 * r)
        }
        fn ports(&self, bounds: Rect, _props: &Props) -> Vec<Port> {
            vec![Port::new("c", bounds.center(), Dir::Right)]
        }
        fn draw(
            &self,
            p: &mut Painter,
            bounds: Rect,
            props: &Props,
            _s: &Style,
            _m: &dyn TextMeasurer,
        ) {
            let r = props.number_or("r", 3.0);
            p.circle(
                bounds.center(),
                r,
                crate::draw::ShapeStyle::outline(Color::BLACK, 1.0),
            );
        }
    }

    struct TestPlugin;
    impl SymbolPlugin for TestPlugin {
        fn id(&self) -> &str {
            "test"
        }
        fn description(&self) -> &str {
            "test plugin"
        }
        fn symbols(&self) -> Vec<Box<dyn Symbol>> {
            vec![Box::new(Dot)]
        }
    }

    #[test]
    fn register_and_resolve() {
        let mut reg = Registry::new();
        reg.register(&TestPlugin);
        assert!(reg.get("test.dot").is_some());
        assert!(reg.get("test.missing").is_none());
        assert_eq!(reg.qualified_names(), vec!["test.dot".to_string()]);
        let plugins = reg.plugins();
        assert_eq!(plugins[0].0, "test");
        assert_eq!(plugins[0].2, vec!["dot".to_string()]);
    }

    #[test]
    fn props_defaults_and_measure() {
        let reg = {
            let mut r = Registry::new();
            r.register(&TestPlugin);
            r
        };
        let sym = reg.get("test.dot").unwrap();
        let props = Props::new().with_defaults(&sym.property_schema());
        let size = sym.measure(&props, &BasicMeasurer::default());
        assert_eq!(size, Size::new(6.0, 6.0)); // default r=3
    }

    #[test]
    fn props_accessors() {
        let mut p = Props::new();
        p.insert("n", PropValue::Number(5.0));
        p.insert("s", PropValue::text("hi"));
        p.insert("flag", PropValue::Bool(true));
        p.insert(
            "pins",
            PropValue::List(vec![PropValue::text("A"), PropValue::text("B")]),
        );
        assert_eq!(p.number_or("n", 0.0), 5.0);
        assert_eq!(p.number_or("missing", 9.0), 9.0);
        assert_eq!(p.text_or("s", "x"), "hi");
        assert!(p.bool_or("flag", false));
        assert_eq!(p.text_list("pins"), vec!["A".to_string(), "B".to_string()]);
    }
}
