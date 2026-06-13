# The diagram YAML language

A document is a YAML mapping with these top-level keys:

| Key       | Required | Purpose                                             |
|-----------|----------|-----------------------------------------------------|
| `vars`    | no       | Named values referenced as `${name}` (see below).   |
| `canvas`  | no       | Canvas size, padding, background, base text style.  |
| `root`    | **yes**  | The single root node of the diagram tree.           |
| `connect` | no       | A list of connections between node ports.           |

## Variables and expressions

`vars:` is a mapping resolved in order (a later var may use an earlier one). Reference a
variable anywhere with `${ ... }`, which may contain a full expression:

```yaml
vars:
  pad: 8
  cols: 3
  cell: 40
  width: ${cols * cell + (cols + 1) * pad}   # arithmetic + references
  accent: "#3366cc"
```

A value that is exactly `${expr}` keeps the expression's type (a number stays a number). An
embedded `"R${n}"` is substituted as a string.

**Expression language:** numbers, `'strings'`/`"strings"`, `+ - * / %`, parentheses, unary
minus, and functions:

- Math: `min`, `max`, `round`, `floor`, `ceil`, `abs`, `sqrt`.
- Text measurement (use these to size to content):
  - `text_width(text, size [, font])`, `text_height(text, size [, font])`
  - `para_width(text, max_width, size [, font])`, `para_height(text, max_width, size [, font])`

> In YAML **flow** mappings (`{ a: ${x} }`), quote expressions: `{ a: "${x}" }`. In block
> style (`a: ${x}` on its own line) no quoting is needed.

## canvas

```yaml
canvas:
  width: 400          # optional; omit (or `auto`) to size to content
  height: auto        # optional
  padding: 12         # space around the root content (default 10)
  background: white   # optional fill; a color or "none"
  font: sans-serif    # base font family (inherited)
  font_size: 14       # base font size
  color: black        # base text color
```

## Nodes

Every node is a mapping with exactly one **type key**, plus optional common attributes.

Type keys: `vbox`, `hbox`, `box`, `symbol`, `text`, `line`, `rect`, `circle`, `path`, `spacer`.

### Common attributes (any node)

| Attribute    | Values                                                      |
|--------------|-------------------------------------------------------------|
| `id`         | string — needed to target the node in `connect`             |
| `width`      | number (fixed), `auto`, or `grow` / `grow N` (flex weight)  |
| `height`     | same as `width`                                             |
| `margin`     | number, `[v, h]`, `[t, r, b, l]`, or `{top, right, ...}`    |
| `align_self` | `start` \| `center` \| `end` \| `stretch`                   |
| Style keys   | `stroke`, `stroke_width`, `fill`, `color`, `font`, `font_size`, `opacity`, `text_anchor` |

Styles inherit to descendants. Colors: `#rgb`, `#rrggbb`, `#rrggbbaa`, `none`, or names like
`black`, `red`, `blue`, `gray`.

### Containers: `vbox`, `hbox`, `box`

- `vbox` stacks children vertically, `hbox` horizontally, `box` overlays them (a stack).
- The value is either the **list of children**, or a **map** with `children:` plus layout
  props. Layout props: `gap` (number), `padding` (edges), `align` (cross-axis:
  `start|center|end|stretch`), `justify` (main-axis:
  `start|center|end|space-between|space-around`).

```yaml
hbox:                  # value is the children list; layout props are siblings
  - text: A
  - text: B
gap: 10
align: center
```
```yaml
vbox:                  # equivalent map form
  gap: 10
  align: center
  children:
    - text: A
    - text: B
```

### `symbol`

```yaml
symbol: schematic.resistor   # "plugin.name"
id: R1
props:                       # symbol-specific (see `diagram symbols describe`)
  ohms: "1k"
```

### `text`

```yaml
text: "Hello"      # single line
```
```yaml
text: ${body}      # wrapped paragraph
wrap: 220          # wrap width in user units
font_size: 12
```

### Raw shapes (fine control)

```yaml
line:   { from: [0, 0], to: [40, 0] }   # local coords, relative to the node's box
rect:   { width: 40, height: 20, rx: 4 }
circle: { r: 10 }
path:   { d: "M0 0 L20 0 L10 18 Z" }     # SVG path: M L H V C Q Z (abs + rel)
spacer:                                    # empty; use `width: grow` for flexible space
```

## connect

A list of connections. Endpoints reference a node `id`, optionally a named port
(`id.port`). Symbols expose ports (e.g. a resistor has `a`/`b`; an IC has one per pin; DFD
shapes have `n`/`e`/`s`/`w`). A bare `id` connects from the node's center.

```yaml
connect:
  - from: R1.b           # node R1, port b
    to: C1.a
    arrow: end           # none | end | both   (default end)
    routing: orthogonal  # straight | orthogonal
    ortho_style: auto    # auto | hvh | vhv  (default auto; see below)
    spread: true         # false = opt this net out of parallel-spread grouping
    label: "signal"      # optional, drawn at the midpoint
    stroke: "#333"       # style overrides allowed
```

### Orthogonal routing and parallel lines

Orthogonal routing draws Z-shaped right-angle paths with an elbow segment at the midpoint.
Two shapes are used:

- **H-V-H** (default for horizontal ports): horizontal → vertical → horizontal; elbows at
  the sides. Used automatically when neither endpoint port faces Up or Down.
- **V-H-V** (default for vertical ports): vertical → horizontal → vertical; elbows at
  the top/bottom. Used automatically when both endpoint ports face Up or Down (e.g. MCU
  bottom pins connecting to a chip's top pins).

Override the auto choice with `ortho_style: hvh` or `ortho_style: vhv`.

**Parallel-line spread.** When multiple connections share the same source and destination
columns their natural shift points would coincide, producing overlapping lines. The engine
detects this automatically and fans the shift points apart by **8 px × rank** (centered on
the natural midpoint) with the fan direction chosen to avoid crossings. No configuration
needed — parallel lines are visibly separated by default.

Set `spread: false` on a connection to opt it out of the spread computation. That net renders
at the natural shift midpoint (offset 0); the remaining nets in the same column still fan
among themselves.

**Leave room.** The fan works best when there is enough space between nodes that `n × 8 px`
of spread fits comfortably. Six parallel lines need roughly 50 px of clearance. Use `margin`
or a fixed canvas `width` to provide it. The router does not detect collisions with node
bodies — avoid placing unrelated nodes inside the spread corridor.
