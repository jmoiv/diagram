# drawskill

A [Claude](https://claude.com/claude-code) **skill** plus a companion **Rust CLI** for
drawing diagrams. You (or Claude) describe a diagram in a small, commented **YAML** language;
`drawskill` lays it out automatically and renders it to **PNG**, **SVG**, or **PDF**.

Highlights:

- **Box-model auto-layout** (`vbox` / `hbox` / `box`) so you rarely place raw coordinates.
- **Variables + expressions**, including inline text measurement, so sizes aren't hand-computed.
- A **plugin system for symbol libraries**. Ships with **schematic** (electronic) and
  **dataflow / DFD** symbols, including a generic IC with named pins.
- Mixes **semantic symbols** with **raw draw primitives** for fine-grained control.
- **Pure Rust** — no system C libraries to install. `cargo build` is all you need.

---

## Setup

`drawskill` is written in Rust and built with Cargo. If you don't already have them, install
the Rust toolchain (this is the only prerequisite — there are **no system libraries** to
install).

### 1. Install Rust and Cargo

The recommended installer is [`rustup`](https://rustup.rs), which installs the Rust compiler
(`rustc`), the package manager / build tool (`cargo`), and keeps them updated.

**Linux / macOS**

Run the official installer and follow the on-screen prompts (accept the defaults):

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then load Cargo into your current shell (new shells pick this up automatically):

```sh
. "$HOME/.cargo/env"
```

> On macOS you may be prompted to install the Xcode Command Line Tools (`xcode-select
> --install`) the first time you compile, since Cargo uses the system linker.

**Windows**

Download and run [`rustup-init.exe`](https://rustup.rs) and follow the prompts. When asked,
allow it to install the Visual Studio C++ build tools (needed for the linker). After it
finishes, open a **new** terminal so `cargo` is on your `PATH`.

Alternatively, with `winget`:

```powershell
winget install Rustlang.Rustup
```

### 2. Verify the installation

Open a new terminal and confirm both tools are available:

```sh
rustc --version
cargo --version
```

You should see version numbers for each (Rust **1.74** or newer is recommended). If the
commands aren't found, restart your terminal so the updated `PATH` takes effect.

### 3. Build drawskill

From the repository root:

```sh
cargo build --release
```

The first build downloads and compiles dependencies and may take a few minutes. When it
finishes, the binary is at:

```
target/release/drawskill
```

You can run it directly, or via Cargo during development:

```sh
./target/release/drawskill --help
# or
cargo run --release -- --help
```

To run the test suite:

```sh
cargo test
```

---

## Usage

Render a diagram (output format is inferred from the file extension, or set with `--format`):

```sh
drawskill render diagram.yaml -o diagram.svg
drawskill render diagram.yaml -o diagram.png
drawskill render diagram.yaml -o diagram.pdf
```

Discover the available symbols and their properties:

```sh
drawskill symbols list
drawskill symbols describe schematic.resistor
```

Measure text up front (handy when you want exact sizes). Text is read from a **file**, never
the command line, so quotes, newlines, and shell metacharacters are never a problem:

```sh
drawskill measure --text-file label.txt --size 14            # single line
drawskill measure --text-file paragraph.txt --size 14 --width 200   # wrapped paragraph
```

Query the fonts available on your system:

```sh
drawskill fonts list
drawskill fonts query --family "DejaVu Sans"
```

`drawskill` uses your **system fonts**; use `fonts list` to see what's installed.

## The diagram language

Diagrams are written in a small, commented YAML language: a tree of auto-laid-out nodes
(`vbox`/`hbox`/`box` containers, `symbol`s, `text`, raw shapes) plus `connect`ions, with a
`vars`/`${expression}` system that includes inline text measurement. For example:

```yaml
# A box whose width fits its title.
vars:
  title: "Hello"
  pad: 12
canvas: { padding: 10, background: white }
root:
  box:
    - rect: { rx: 6 }
      width: ${text_width(title, 16) + pad * 2}
      height: 40
      fill: "#eef"
      stroke: "#88a"
    - text: ${title}
      font_size: 16
```

See [`examples/`](examples/) for complete diagrams (a schematic, an IC, a data-flow diagram,
and an expression-driven note card), each rendered to `.svg` alongside its `.yaml`. The full
language reference and symbol catalog live in [`skill/reference/`](skill/reference/).

> Tip: inside YAML **flow** mappings (`{ ... }`), quote expressions — `{ width: "${w}" }` —
> because the `}` would otherwise close the mapping. Block style needs no quoting.

The two built-in symbol plugins are **schematic** (resistor, capacitor, inductor, diode,
voltage source, switch, ground, junction, and a generic IC with named pins) and **dataflow**
(process, external entity, data store, flow, trust boundary). Run `drawskill symbols list` to
see them all and `drawskill symbols describe <plugin.name>` for a symbol's properties.

---

## Using it as a Claude skill

The `skill/` directory contains the Claude skill (named `diagram`), with `SKILL.md` and a
`reference/` folder documenting the language and symbols. Once the `drawskill` binary is built
and on your `PATH`, install the skill by copying `skill/` into your Claude skills directory
(e.g. `~/.claude/skills/diagram/`). Claude will then discover symbols and fonts via the CLI,
author the commented YAML, measure text when needed, and render diagrams for you on request.

---

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
