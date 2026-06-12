# CLAUDE.md

Guidance for working in this repository.

## What this is

**drawskill** is a pure-Rust diagram renderer plus a companion Claude skill. You describe a
diagram in a small, commented YAML language; the engine lays it out (auto box model) and
renders it to **SVG, PNG, or PDF**. There are **no system dependencies** — `cargo build` is
all that's needed. Keep it that way: do not add crates that require system C libraries.

## Workspace layout

```
crates/
  drawskill-core/              # the engine (see Pipeline below)
  drawskill-cli/               # the `drawskill` binary (render/measure/fonts/symbols)
    tests/cli.rs               # integration tests that drive the built binary
  drawskill-symbols-schematic/ # `schematic` plugin (resistor, ..., generic IC)
  drawskill-symbols-dataflow/  # `dataflow` (DFD) plugin
skill/                         # the Claude skill (name: diagram)
  SKILL.md
  reference/{language,symbols}.md
examples/                      # *.yaml (commented) + rendered *.svg (kept in sync)
```

## Pipeline (drawskill-core modules)

YAML → `parse` (saphyr load → owned `Yv` tree → `vars` + `${expr}` interpolation) → typed
`model` → `layout` (two-pass box model: intrinsic sizing, then placement with
grow/align/justify, port resolution, connection routing) → `render` (canonical **SVG**) →
`output` (PNG via `resvg`/`tiny-skia`, PDF via `svg2pdf`). Text sizing goes through the
`measure::TextMeasurer` trait; the real impl is `text::FontContext` (fontdb + cosmic-text),
and `measure::BasicMeasurer` is a font-free version used in unit tests.

Symbols are a compile-time plugin system: `symbols::{Symbol, SymbolPlugin, Registry}`. The
CLI registers both plugins in `build_registry()`.

## Common commands

```sh
cargo build                              # debug build
cargo build --release                    # -> target/release/drawskill
cargo test --workspace                   # all unit + integration tests
cargo fmt                                # format
cargo clippy --workspace --all-targets   # lint

# Render / inspect while iterating:
./target/debug/drawskill render examples/rc_filter.yaml -o /tmp/out.png --scale 2
./target/debug/drawskill symbols describe schematic.ic
```

## Definition of done — do these for EVERY change

Before considering any change complete:

1. **Tests pass:** `cargo test --workspace` is green. Add or update tests for the behavior
   you changed — unit tests live next to the code (`#[cfg(test)]`); end-to-end/CLI tests live
   in `crates/drawskill-cli/tests/cli.rs`. New surface area (a symbol, a language feature, an
   expression function, a CLI flag) **must** come with tests.
2. **Lint + format clean:** `cargo clippy --workspace --all-targets` has no warnings, and
   `cargo fmt` has been run.
3. **Examples stay in sync:** if you changed layout, rendering, a symbol's drawing, or the
   language, **re-render the examples** and commit the updated SVGs, and eyeball at least one
   PNG to confirm it still looks right:
   ```sh
   for f in rc_filter ic_atmega dfd_login note_card; do
     ./target/debug/drawskill render examples/$f.yaml -o examples/$f.svg
   done
   ```
   If you add a feature worth showing, add a new commented `examples/<name>.yaml` (+ its
   `.svg`) and include it in the `renders_every_example_to_all_formats` test list.
4. **Docs stay in sync:**
   - Changed the **language** (new node type, attribute, expression function, syntax rule)?
     Update `skill/reference/language.md` (and `SKILL.md` if it affects the workflow/rules).
   - Added or changed a **symbol** or its properties? Update `skill/reference/symbols.md`.
     Property schemas are also surfaced by `drawskill symbols describe`, so keep the in-code
     `property_schema()` accurate.
   - Changed the **CLI** or setup? Update `README.md`.
5. **Comment as the language asks:** example YAML must be liberally commented (the skill tells
   Claude to do this — the examples are the model to imitate).
6. **Commit** at a sensible milestone with a descriptive message. Only commit/push when asked
   per the harness rules; branch first if on `main` and the change is substantial.

## Adding a symbol

Implement `Symbol` in the relevant plugin crate and add it to that plugin's `symbols()` list.
A symbol must: declare `property_schema()`, `measure()` its intrinsic size (measuring text via
the passed `TextMeasurer` when size depends on labels — see the IC), expose connection
`ports()` in absolute coords, and `draw()` via the `Painter`. Then: add unit tests (ports,
measure, that `draw` emits the expected primitives), update `skill/reference/symbols.md`, and
add/extend an example if it's a notable capability.

## Gotchas (learned the hard way)

- **Flow-mapping `${}` quoting:** in YAML flow style `{ ... }`, an unquoted `${expr}` breaks
  the parser (its `}` closes the mapping). Use `{ width: "${w}" }` or block style. This is a
  documented authoring rule, not a bug — preserve it in examples/docs.
- **Fonts:** `cosmic-text` re-exports its own `fontdb` (0.16). Use `cosmic_text::fontdb`; do
  not add a separate `fontdb` dependency.
- **PNG/PDF crates:** `resvg` (0.45) and `svg2pdf` (0.13) share `usvg` 0.45. PNG and PDF each
  re-parse the generated SVG string independently in `output.rs` — keep that decoupling.
- **`measure` reads from a file** (`--text-file`, or `-` for stdin), never a shell argument,
  to avoid shell-injection/quoting issues. Keep it that way.
- **No system deps.** If a crate pulls in a C library, find another approach.
