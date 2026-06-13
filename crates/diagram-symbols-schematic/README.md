# diagram-symbols-schematic

The `schematic` symbol plugin for [diagram](../../README.md): common electronic parts plus a
generic IC with named pins. Everything lives in [`src/lib.rs`](src/lib.rs).

## Symbols

| Qualified name        | Ports                | Orientation              | Key properties              |
|-----------------------|----------------------|--------------------------|-----------------------------|
| `schematic.resistor`  | `a`, `b`             | `horizontal \| vertical` | `ohms`, `label`             |
| `schematic.capacitor` | `a`, `b`             | `horizontal \| vertical` | `farads`, `label`           |
| `schematic.inductor`  | `a`, `b`             | `horizontal \| vertical` | `henries`, `label`          |
| `schematic.switch`    | `a`, `b`             | `horizontal \| vertical` | `label`                     |
| `schematic.diode`     | `a` (anode), `b`     | `right\|left\|up\|down`  | `label`                     |
| `schematic.vsource`   | `a` (+), `b` (−)     | `right\|left\|up\|down`  | `volts`, `label`            |
| `schematic.ground`    | `a`                  | `right\|left\|up\|down`  | —                           |
| `schematic.junction`  | `c` (center)         | none                     | —                           |
| `schematic.ic`        | one per named pin    | none                     | `name`, `*_pins`, `*_spacing` |

`diagram symbols describe schematic.<name>` prints the authoritative property schema
(generated from each symbol's `property_schema()`).

## Orientation convention

Parts carry an `orientation` property *where it makes sense*. The vocabulary is per-symbol and
deliberately differs by symmetry:

- **Axis-symmetric two-terminal parts** (`resistor`, `capacitor`, `inductor`, `switch`) take
  **`horizontal | vertical`** (default `horizontal`). They are electrically symmetric, so only
  the axis matters — there is no left/right or up/down distinction.
- **Polarized / single-terminal parts** (`diode`, `vsource`, `ground`) take the full
  **`right | left | up | down`** (default `right`, except `ground` defaults to `up`). Facing is
  meaningful here: the diode's anode→cathode direction, the source's +/− polarity, the ground
  lead's direction.
- **`junction` and `ic`** are symmetric in all dimensions and have **no** orientation.

For two-terminal parts, `orientation` is the direction of travel from the start lead `a` to
the end lead `b`; `ground`'s `orientation` is the direction its single lead points (toward the
circuit).

## IC pin spacing

`schematic.ic` auto-sizes to fit its `name` and the pin labels, distributing each side's pins
evenly. Four optional properties — `left_spacing`, `right_spacing`, `top_spacing`,
`bottom_spacing` — override that for a side: when **> 0** they set the *exact* center-to-center
distance in px between adjacent pins on that side (the IC grows that dimension and centers the
pin block). The default `0` means auto.

## Implementation notes (for adding / changing symbols)

- **Two-terminal parts share a natural-frame model.** Each part's geometry is written once, as
  if it were horizontal and facing right, in *natural coordinates*: `a` along the lead-to-lead
  (long) axis from the start lead, `c` across the short axis. The [`Tt`](src/lib.rs) context
  (`Tt::at`, `Tt::leads`, `Tt::ports`, `Tt::label`) maps those points into absolute canvas
  coordinates for the requested [`Facing`], so a single drawing serves all orientations. Add a
  new two-terminal part by building a `Tt` and emitting its body via `ctx.at(a, c)`.
- **Default orientations reproduce the pre-orientation geometry exactly** — `Facing::Right`
  (and `ground`'s default `up`) are the identity mappings, so the rendered examples don't drift.
- **Label strip.** A horizontal part reserves `LABEL_H` above its body for the value label; a
  vertical part reserves a fixed `VLABEL_W`-wide strip beside it. The strip width is a constant
  (not measured) so `ports()`, which has no `TextMeasurer`, can place the centerline without one.
- **Ports** are named `a`/`b` (two-terminal, start→end), `c` (junction center), or by pin name
  (IC). Keep these stable — examples and docs reference them in `connect:`.
- **Value labels** come from `value_label(props, key, unit)`: an explicit `label` wins,
  otherwise the value key (e.g. `ohms`) is formatted via `diagram_core::expr::format_num` with
  the unit suffix.
- **The IC is its own thing** (not a `Tt`): pins are placed by `side_positions(count, spacing,
  start, extent)`, which honors `*_spacing` or falls back to even distribution.

### Definition of done for a symbol change

1. `cargo test -p diagram-symbols-schematic` (ports / measure / draw-primitive unit tests).
2. Update [`skill/reference/symbols.md`](../../skill/reference/symbols.md) and keep
   `property_schema()` accurate (it feeds `diagram symbols describe`).
3. If drawing/layout changed, re-render the examples (`examples/*.svg`, `docs/images/*.png`) —
   default orientations must stay byte-identical unless the change is intentional.
