# Development

How to build diagram from source and work on it. For an architectural overview and the
"definition of done" checklist applied to every change, see [CLAUDE.md](CLAUDE.md).

## Prerequisites — install Rust and Cargo

diagram builds with Cargo and has **no system-library dependencies** — the Rust toolchain is
the only prerequisite. The recommended installer is [`rustup`](https://rustup.rs), which
installs the compiler (`rustc`), the build tool (`cargo`), and keeps them updated.

**Linux / macOS**

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
. "$HOME/.cargo/env"     # load Cargo into the current shell (new shells do this automatically)
```

> On macOS you may be prompted to install the Xcode Command Line Tools (`xcode-select
> --install`) the first time you compile, since Cargo uses the system linker.

**Windows**

Download and run [`rustup-init.exe`](https://rustup.rs) and follow the prompts, allowing it to
install the Visual Studio C++ build tools (needed for the linker). Then open a **new** terminal
so `cargo` is on your `PATH`. Or with winget: `winget install Rustlang.Rustup`.

**Verify** (Rust 1.74 or newer is recommended):

```sh
rustc --version
cargo --version
```

## Build

```sh
cargo build              # debug build
cargo build --release    # optimized -> target/release/diagram
```

The first build downloads and compiles dependencies and may take a few minutes. Run the binary
directly (`./target/release/diagram --help`) or via Cargo (`cargo run -- --help`).

## Test, lint, format

```sh
cargo test --workspace                   # unit + CLI integration tests
cargo clippy --workspace --all-targets   # lint (keep warning-free)
cargo fmt                                # format
```

## Project layout

```
crates/
  diagram-core/              # engine: parse -> model -> layout -> render -> output
  diagram-cli/               # the `diagram` binary; tests/cli.rs drives it end-to-end
  diagram-symbols-schematic/ # `schematic` plugin
  diagram-symbols-dataflow/  # `dataflow` (DFD) plugin
skill/                         # the Claude skill (SKILL.md + reference/)
examples/                      # commented *.yaml + rendered *.svg (kept in sync)
docs/images/                   # rendered PNGs used by README.md
```

The render pipeline, how to add a symbol, and the gotchas (YAML flow-mapping `${}` quoting,
crate-version coupling, etc.) are documented in [CLAUDE.md](CLAUDE.md).

## Before you commit — the short version

1. `cargo test --workspace` is green (add/adjust tests for what you changed).
2. `cargo clippy --workspace --all-targets` is clean and `cargo fmt` has been run.
3. If you changed layout, rendering, a symbol, or the language: **re-render the examples** and
   commit the updated artifacts, and eyeball a PNG.
   ```sh
   for f in rc_filter ic_atmega dfd_login note_card; do
     ./target/release/diagram render examples/$f.yaml -o examples/$f.svg
     ./target/release/diagram render examples/$f.yaml -o docs/images/$f.png --scale 2
   done
   ```
4. Update docs that went stale: `skill/reference/language.md`, `skill/reference/symbols.md`,
   `README.md`.

The full checklist (with rationale) is in [CLAUDE.md](CLAUDE.md).

## Releasing

Releases are built automatically by GitHub Actions when a version tag is pushed.
Tag the commit you want to release on `main`:

```sh
git tag v0.x.y
git push origin v0.x.y
```

The workflow builds binaries for all five platforms and publishes them to
<https://github.com/jmoiv/diagram/releases>. CI (tests + lint) also runs on every push.
