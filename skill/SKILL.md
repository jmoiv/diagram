---
name: diagram
description: Draw diagrams (schematics, data-flow diagrams, block diagrams, flowcharts) and render them to PNG, SVG, or PDF. Use when the user asks to draw, diagram, sketch, or visualize a circuit, system, architecture, data flow, or similar — anything built from labeled boxes, symbols, and connections. Authors a small commented YAML language and renders it with the `diagram` CLI.
---

# Drawing diagrams with diagram

`diagram` renders diagrams from a small YAML language. You write a `.yaml` file describing
a tree of laid-out nodes (boxes, symbols, text, shapes) plus connections, then run the CLI to
render it to **PNG**, **SVG**, or **PDF**.

## Workflow

1. **Discover what's available** before authoring:
   - `diagram symbols list` — all symbol plugins and their symbols.
   - `diagram symbols describe <plugin.name>` — a symbol's properties (e.g.
     `diagram symbols describe schematic.ic`).
   - `diagram fonts list` / `diagram fonts query --family "Name"` — available system fonts.
2. **Author** a commented `.yaml` file (see `reference/language.md` for the full spec).
3. **Render** to the format the user asked for:
   ```
   diagram render diagram.yaml -o diagram.png        # or .svg / .pdf
   diagram render diagram.yaml -o out.png --scale 2  # higher-res PNG
   ```
4. **Look at the result** (open/inspect the PNG) and iterate.

## Authoring rules — follow these

- **Comment liberally.** YAML supports `#` comments. Explain what each section is and why.
  These files are read back later; comments and clear structure matter.
- **Use `vars:` for anything repeated or meaningful** — sizes, colors, gaps, repeated text.
  Reference them with `${name}`. This keeps diagrams readable and easy to adjust.
- **Prefer auto-layout over manual coordinates.** Use `vbox` / `hbox` / `box` containers with
  `gap`, `padding`, `align`, and `justify`. Only reach for raw `line`/`rect`/`path` and manual
  sizes when you need fine control.
- **Quote expressions inside flow mappings.** In YAML flow style `{ ... }`, an unquoted
  `${expr}` breaks parsing because of the `}`. Write `{ width: "${w}" }` or use block style:
  ```yaml
  rect:
    width: ${w}      # fine in block style
  ```
- **Let the tool measure text.** To size something to text, use the inline measurement
  functions in expressions (`text_width`, `para_height`, …) instead of guessing. For example:
  `width: ${text_width(title, 16) + 24}`. If you need exact numbers up front, write the text to
  a temp file and run `diagram measure --text-file /tmp/t.txt --size 16` (and `--width N` for
  a wrapped paragraph). Always pass text via a file, never as a shell argument.

## Minimal example

```yaml
# A box with a centered label, sized to its text.
vars:
  title: "Hello"
  pad: 12
canvas:
  padding: 10
  background: white
root:
  box:
    - rect:
        rx: 6
      width: ${text_width(title, 16) + pad * 2}
      height: 40
      fill: "#eef"
      stroke: "#88a"
    - text: ${title}
      font_size: 16
```

## What you can draw

- **schematic** plugin — electronic symbols: resistor, capacitor, inductor, diode, voltage
  source, switch, ground, junction, and a generic **ic** with named pins.
- **dataflow** plugin — DFD symbols: process, external entity, data store, flow arrow, trust
  boundary.
- **shapes** plugin — generic shapes with colored backgrounds: rectangle, circle, oval,
  explosion callout, and block arrow (each takes `fill` and `text_color`).
- **Generic** — nested boxes, text (single line or wrapped), raw lines/rects/circles/paths,
  and labeled connections with arrows. Great for block diagrams, flowcharts, and architecture
  sketches even without a dedicated symbol set.

See `reference/language.md` for the complete language and `reference/symbols.md` for how to
explore the symbol catalog.
