# Symbol catalog

The symbol library is a plugin system. Rather than memorize it, **query the CLI** — it's the
source of truth and reflects any installed plugins:

```
diagram symbols list                      # every plugin and its symbols
diagram symbols describe schematic.ic     # one symbol's properties + defaults
```

`symbols describe` prints each property's name, kind (`number`/`text`/`bool`/`list`/`enum`),
whether it's required, its default, and a description.

## Built-in plugins (summary)

### `schematic` — electronic symbols

Two-terminal parts have ports `a` (start) and `b` (end); their value property is drawn as a
label beside the part.

| Symbol                 | Key property        | Ports            | `orientation`              |
|------------------------|---------------------|------------------|----------------------------|
| `schematic.resistor`   | `ohms`              | `a`, `b`         | `horizontal` \| `vertical` |
| `schematic.capacitor`  | `farads`            | `a`, `b`         | `horizontal` \| `vertical` |
| `schematic.inductor`   | `henries`           | `a`, `b`         | `horizontal` \| `vertical` |
| `schematic.switch`     | `label`             | `a`, `b`         | `horizontal` \| `vertical` |
| `schematic.diode`      | `label`             | `a`, `b`         | `right`/`left`/`up`/`down` |
| `schematic.vsource`    | `volts`             | `a`, `b`         | `right`/`left`/`up`/`down` |
| `schematic.ground`     | —                   | `a`              | `right`/`left`/`up`/`down` (default `up`) |
| `schematic.junction`   | —                   | `c` (center)     | — (symmetric)              |
| `schematic.ic`         | `name`, `*_pins`    | one per pin name | — (symmetric)              |

**Orientation.** Add `orientation:` to stand a part up or flip it. The vocabulary differs by
symmetry: the axis-symmetric parts (resistor, capacitor, inductor, switch) take `horizontal` or
`vertical` (default `horizontal`); the polarized / single-terminal parts (diode, vsource,
ground) take the four facings `right`/`left`/`up`/`down` (default `right`, except ground
defaults to `up`). For two-terminal parts the facing is the direction from lead `a` to lead `b`.

```yaml
- symbol: schematic.resistor
  props: { ohms: "10k", orientation: vertical }
- symbol: schematic.diode
  props: { orientation: down }     # anode a on top, cathode b at the bottom
```

The generic IC takes a `name` and four optional pin lists — `left_pins`, `right_pins`,
`top_pins`, `bottom_pins` — and auto-sizes to fit the chip name and pin labels. Each pin name
becomes a port: `connect: [{ from: U1.CLK, to: ... }]`. Per-side `*_spacing` properties
(`left_spacing`, `right_spacing`, `top_spacing`, `bottom_spacing`) spread that side's pins to
an *exact* center-to-center distance in pixels (default `0` = auto).

```yaml
symbol: schematic.ic
id: U1
props:
  name: ATmega
  left_pins:    [VCC, GND, RESET]
  left_spacing: 40            # force 40px between the left pins
  right_pins:   [PB0, PB1, PB2]
  top_pins:     [XTAL1, XTAL2]
```

### `dataflow` — data-flow diagram (DFD) symbols

All expose compass ports `n`, `e`, `s`, `w` (except `flow`, which uses `a`/`b`).

| Symbol               | Properties        | Notes                                  |
|----------------------|-------------------|----------------------------------------|
| `dataflow.process`   | `label`, `number` | Circle.                                |
| `dataflow.entity`    | `label`           | External entity (rectangle).           |
| `dataflow.store`     | `label`, `number` | Open-ended data store with a # cell.   |
| `dataflow.flow`      | `label`           | Standalone labeled arrow (`a` → `b`).  |
| `dataflow.boundary`  | `label`           | Dashed trust boundary; set width/height. |

Connections (`connect:`) draw the data flows between symbols; use the `flow` symbol only when
you want a standalone arrow element.

### `shapes` — generic shapes

Plain building blocks with colored backgrounds. Every shape accepts `label` (text drawn
inside), `fill` (background color — a name like `blue` or a `#hex`, default white),
`stroke_color` (border color — name or `#hex`, defaults to the inherited stroke color), and
`text_color` (label color, defaults to the inherited text color). The filled shapes expose
compass ports `n`, `e`, `s`, `w`; `arrow` exposes its tail `a` and tip `b`.

| Symbol             | Extra properties      | Ports            | Notes                                   |
|--------------------|-----------------------|------------------|-----------------------------------------|
| `shapes.rectangle` | `rx`                  | `n`, `e`, `s`, `w` | `rx` rounds the corners (0 = square).  |
| `shapes.circle`    | —                     | `n`, `e`, `s`, `w` | Auto-sizes to a square fitting `label`. |
| `shapes.oval`      | —                     | `n`, `e`, `s`, `w` | Ellipse.                                |
| `shapes.explosion` | `spikes`              | `n`, `e`, `s`, `w` | Starburst callout (`spikes`, default 12). |
| `shapes.arrow`     | `direction`           | `a` (tail), `b` (tip) | Block arrow; `direction` is right/left/up/down. |

```yaml
symbol: shapes.rectangle
id: step
props: { label: Process, fill: "#e8ffe8", text_color: "#14532d", rx: 8 }
```
