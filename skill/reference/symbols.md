# Symbol catalog

The symbol library is a plugin system. Rather than memorize it, **query the CLI** — it's the
source of truth and reflects any installed plugins:

```
drawskill symbols list                      # every plugin and its symbols
drawskill symbols describe schematic.ic     # one symbol's properties + defaults
```

`symbols describe` prints each property's name, kind (`number`/`text`/`bool`/`list`/`enum`),
whether it's required, its default, and a description.

## Built-in plugins (summary)

### `schematic` — electronic symbols

Two-terminal parts have ports `a` (left) and `b` (right); their value property is drawn as a
label above the part.

| Symbol                 | Key property        | Ports            |
|------------------------|---------------------|------------------|
| `schematic.resistor`   | `ohms`              | `a`, `b`         |
| `schematic.capacitor`  | `farads`            | `a`, `b`         |
| `schematic.inductor`   | `henries`           | `a`, `b`         |
| `schematic.diode`      | `label`             | `a`, `b`         |
| `schematic.vsource`    | `volts`             | `a`, `b`         |
| `schematic.switch`     | `label`             | `a`, `b`         |
| `schematic.ground`     | —                   | `a` (top)        |
| `schematic.junction`   | —                   | `c` (center)     |
| `schematic.ic`         | `name`, `*_pins`    | one per pin name |

The generic IC takes a `name` and four optional pin lists — `left_pins`, `right_pins`,
`top_pins`, `bottom_pins` — and auto-sizes to fit the chip name and pin labels. Each pin name
becomes a port: `connect: [{ from: U1.CLK, to: ... }]`.

```yaml
symbol: schematic.ic
id: U1
props:
  name: ATmega
  left_pins:  [VCC, GND, RESET]
  right_pins: [PB0, PB1, PB2]
  top_pins:   [XTAL1, XTAL2]
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
