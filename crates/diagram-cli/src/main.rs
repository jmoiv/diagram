//! diagram command-line interface.
//!
//! Subcommands:
//! - `render`  — render a YAML diagram to SVG/PNG/PDF.
//! - `measure` — measure text (single line or width-constrained paragraph) from a file.
//! - `fonts`   — list/query available system fonts.
//! - `symbols` — list symbols or describe one symbol's properties.
//! - `install` — install the companion Claude skill into a `.claude/skills` directory.
//! - `update`  — download and install the latest release binary, then refresh the skill.

mod update;

/// The Claude skill files, embedded so `diagram install` is self-contained.
const SKILL_MD: &str = include_str!("../../../skill/SKILL.md");
const SKILL_LANGUAGE: &str = include_str!("../../../skill/reference/language.md");
const SKILL_SYMBOLS: &str = include_str!("../../../skill/reference/symbols.md");

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use diagram_core::measure::TextMeasurer;
use diagram_core::output::Format;
use diagram_core::symbols::{PropKind, PropValue, Registry};
use diagram_core::text::FontContext;

/// Build a registry with the built-in symbol plugins registered.
fn build_registry() -> Registry {
    let mut reg = Registry::new();
    reg.register(&diagram_symbols_schematic::Schematic);
    reg.register(&diagram_symbols_dataflow::Dataflow);
    reg.register(&diagram_symbols_shapes::Shapes);
    reg
}

#[derive(Parser)]
#[command(
    name = "diagram",
    version = env!("DIAGRAM_VERSION"),
    about = "Render diagrams from a YAML language to SVG/PNG/PDF."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Render a YAML diagram to SVG, PNG, or PDF.
    Render {
        /// Input YAML file.
        input: PathBuf,
        /// Output file ("-" for stdout). Format is inferred from the extension.
        #[arg(short, long)]
        output: String,
        /// Force the output format (svg|png|pdf), overriding the extension.
        #[arg(short, long)]
        format: Option<String>,
        /// PNG scale factor (pixels per user unit).
        #[arg(long, default_value_t = 1.0)]
        scale: f32,
    },
    /// Measure text. Reads the text from a file (never a shell argument).
    Measure {
        /// File containing the text to measure ("-" reads stdin).
        #[arg(long)]
        text_file: String,
        /// Font family (defaults to sans-serif).
        #[arg(long, default_value = "sans-serif")]
        font: String,
        /// Font size in user units.
        #[arg(long, default_value_t = 14.0)]
        size: f64,
        /// If set, wrap the text to this width and report paragraph metrics.
        #[arg(long)]
        width: Option<f64>,
    },
    /// List or query available system fonts.
    Fonts {
        #[command(subcommand)]
        what: FontsCmd,
    },
    /// List available symbols or describe one.
    Symbols {
        #[command(subcommand)]
        what: SymbolsCmd,
    },
    /// Install the companion Claude skill (`diagram`) into a `.claude/skills` directory.
    ///
    /// Chooses the target by searching upward from the current directory: an existing
    /// `.claude` directory wins; otherwise a new `.claude` is created beside the nearest
    /// `.git`; otherwise it lands in the current directory.
    Install,
    /// Check for a newer release, replace this binary if one is found, and refresh the skill.
    Update,
}

#[derive(Subcommand)]
enum FontsCmd {
    /// List all available font families.
    List,
    /// Check whether a family is available and show its styles.
    Query {
        #[arg(long)]
        family: String,
    },
}

#[derive(Subcommand)]
enum SymbolsCmd {
    /// List all plugins and their symbols.
    List,
    /// Describe a symbol's properties, given its qualified "plugin.name".
    Describe { name: String },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Render {
            input,
            output,
            format,
            scale,
        } => cmd_render(input, output, format, scale),
        Command::Measure {
            text_file,
            font,
            size,
            width,
        } => cmd_measure(text_file, font, size, width),
        Command::Fonts { what } => cmd_fonts(what),
        Command::Symbols { what } => cmd_symbols(what),
        Command::Install => cmd_install(),
        Command::Update => update::run(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("diagram: error: {e}");
            ExitCode::FAILURE
        }
    }
}

type CmdResult = Result<(), Box<dyn std::error::Error>>;

/// Install the embedded `diagram` skill into the resolved `.claude/skills/diagram` directory.
fn cmd_install() -> CmdResult {
    let claude_dir = resolve_claude_dir()?;
    let dest = claude_dir.join("skills").join("diagram");
    std::fs::create_dir_all(dest.join("reference"))
        .map_err(|e| format!("cannot create {}: {e}", dest.display()))?;

    std::fs::write(dest.join("SKILL.md"), SKILL_MD)?;
    std::fs::write(dest.join("reference").join("language.md"), SKILL_LANGUAGE)?;
    std::fs::write(dest.join("reference").join("symbols.md"), SKILL_SYMBOLS)?;

    eprintln!("installed the 'diagram' skill to {}", dest.display());
    Ok(())
}

/// Decide which `.claude` directory the skill should be installed into:
/// 1. an existing `.claude` in the current directory or any ancestor;
/// 2. otherwise, a new `.claude` beside the nearest ancestor `.git` (the repo root);
/// 3. otherwise, `.claude` in the current directory.
fn resolve_claude_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let cwd = std::env::current_dir()?;

    // 1. An existing `.claude` directory takes precedence.
    for dir in cwd.ancestors() {
        let candidate = dir.join(".claude");
        if candidate.is_dir() {
            return Ok(candidate);
        }
    }
    // 2. Otherwise, sit beside the nearest `.git` (file or directory, for worktrees).
    for dir in cwd.ancestors() {
        if dir.join(".git").exists() {
            return Ok(dir.join(".claude"));
        }
    }
    // 3. Fall back to the current directory.
    Ok(cwd.join(".claude"))
}

fn cmd_render(input: PathBuf, output: String, format: Option<String>, scale: f32) -> CmdResult {
    let source = std::fs::read_to_string(&input)
        .map_err(|e| format!("cannot read input {}: {e}", input.display()))?;

    let fmt = match &format {
        Some(f) => Format::from_extension(f)
            .ok_or_else(|| format!("unknown format {f:?} (expected svg, png, or pdf)"))?,
        None => {
            if output == "-" {
                return Err("when writing to stdout, pass --format svg|png|pdf".into());
            }
            Format::from_path(std::path::Path::new(&output)).ok_or_else(|| {
                format!("cannot infer format from output {output:?}; pass --format")
            })?
        }
    };

    let registry = build_registry();
    let measurer = FontContext::new();
    let bytes = diagram_core::render_to_bytes(&source, &registry, &measurer, fmt, scale)?;

    if output == "-" {
        std::io::stdout().write_all(&bytes)?;
    } else {
        std::fs::write(&output, &bytes).map_err(|e| format!("cannot write {output}: {e}"))?;
        eprintln!("wrote {} ({} bytes)", output, bytes.len());
    }
    Ok(())
}

fn cmd_measure(text_file: String, font: String, size: f64, width: Option<f64>) -> CmdResult {
    let text = read_text_source(&text_file)?;
    let ctx = FontContext::new();

    let json = match width {
        Some(w) => {
            let p = ctx.measure_paragraph(&text, w, &font, size);
            serde_json::json!({
                "mode": "paragraph",
                "width": round3(p.width),
                "height": round3(p.height),
                "lines": p.lines,
                "max_width": w,
                "font": font,
                "size": size,
            })
        }
        None => {
            let m = ctx.measure_line(&text, &font, size);
            serde_json::json!({
                "mode": "line",
                "width": round3(m.width),
                "height": round3(m.height()),
                "ascent": round3(m.ascent),
                "descent": round3(m.descent),
                "font": font,
                "size": size,
            })
        }
    };
    println!("{}", serde_json::to_string_pretty(&json)?);
    Ok(())
}

fn cmd_fonts(what: FontsCmd) -> CmdResult {
    let ctx = FontContext::new();
    let families = ctx.families();
    let json = match what {
        FontsCmd::List => serde_json::json!({
            "count": families.len(),
            "families": families.iter().map(|f| serde_json::json!({
                "name": f.name,
                "styles": f.styles,
                "monospaced": f.monospaced,
            })).collect::<Vec<_>>(),
        }),
        FontsCmd::Query { family } => {
            let found = families
                .iter()
                .find(|f| f.name.eq_ignore_ascii_case(&family));
            serde_json::json!({
                "query": family,
                "available": found.is_some(),
                "match": found.map(|f| serde_json::json!({
                    "name": f.name,
                    "styles": f.styles,
                    "monospaced": f.monospaced,
                })),
            })
        }
    };
    println!("{}", serde_json::to_string_pretty(&json)?);
    Ok(())
}

fn cmd_symbols(what: SymbolsCmd) -> CmdResult {
    let registry = build_registry();
    let json = match what {
        SymbolsCmd::List => {
            let plugins = registry
                .plugins()
                .into_iter()
                .map(|(id, desc, symbols)| {
                    serde_json::json!({
                        "plugin": id,
                        "description": desc,
                        "symbols": symbols.iter().map(|s| format!("{id}.{s}")).collect::<Vec<_>>(),
                    })
                })
                .collect::<Vec<_>>();
            serde_json::json!({ "plugins": plugins })
        }
        SymbolsCmd::Describe { name } => {
            let sym = registry
                .get(&name)
                .ok_or_else(|| format!("unknown symbol {name:?} (try `diagram symbols list`)"))?;
            let props = sym
                .property_schema()
                .into_iter()
                .map(|spec| {
                    serde_json::json!({
                        "name": spec.name,
                        "kind": prop_kind_str(&spec.kind),
                        "required": spec.required,
                        "default": spec.default.as_ref().map(prop_value_json),
                        "description": spec.description,
                    })
                })
                .collect::<Vec<_>>();
            serde_json::json!({
                "name": name,
                "description": sym.description(),
                "properties": props,
            })
        }
    };
    println!("{}", serde_json::to_string_pretty(&json)?);
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Read the measure text from a file path, or stdin when the path is "-".
fn read_text_source(path: &str) -> Result<String, Box<dyn std::error::Error>> {
    if path == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        Ok(s)
    } else {
        std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read text file {path}: {e}").into())
    }
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn prop_kind_str(kind: &PropKind) -> String {
    match kind {
        PropKind::Number => "number".into(),
        PropKind::Text => "text".into(),
        PropKind::Bool => "bool".into(),
        PropKind::List => "list".into(),
        PropKind::Enum(values) => format!("enum({})", values.join("|")),
    }
}

fn prop_value_json(v: &PropValue) -> serde_json::Value {
    match v {
        PropValue::Number(n) => serde_json::json!(n),
        PropValue::Text(s) => serde_json::json!(s),
        PropValue::Bool(b) => serde_json::json!(b),
        PropValue::List(items) => {
            serde_json::Value::Array(items.iter().map(prop_value_json).collect())
        }
    }
}
