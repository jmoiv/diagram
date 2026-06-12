//! drawskill command-line interface.
//!
//! Subcommands:
//! - `render`  — render a YAML diagram to SVG/PNG/PDF.
//! - `measure` — measure text (single line or width-constrained paragraph) from a file.
//! - `fonts`   — list/query available system fonts.
//! - `symbols` — list symbols or describe one symbol's properties.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use drawskill_core::measure::TextMeasurer;
use drawskill_core::output::Format;
use drawskill_core::symbols::{PropKind, PropValue, Registry};
use drawskill_core::text::FontContext;

/// Build a registry with the built-in symbol plugins registered.
fn build_registry() -> Registry {
    let mut reg = Registry::new();
    reg.register(&drawskill_symbols_schematic::Schematic);
    reg.register(&drawskill_symbols_dataflow::Dataflow);
    reg
}

#[derive(Parser)]
#[command(name = "drawskill", version, about = "Render diagrams from a YAML language to SVG/PNG/PDF.")]
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
        Command::Render { input, output, format, scale } => cmd_render(input, output, format, scale),
        Command::Measure { text_file, font, size, width } => cmd_measure(text_file, font, size, width),
        Command::Fonts { what } => cmd_fonts(what),
        Command::Symbols { what } => cmd_symbols(what),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("drawskill: error: {e}");
            ExitCode::FAILURE
        }
    }
}

type CmdResult = Result<(), Box<dyn std::error::Error>>;

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
            Format::from_path(std::path::Path::new(&output))
                .ok_or_else(|| format!("cannot infer format from output {output:?}; pass --format"))?
        }
    };

    let registry = build_registry();
    let measurer = FontContext::new();
    let bytes = drawskill_core::render_to_bytes(&source, &registry, &measurer, fmt, scale)?;

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
            let found = families.iter().find(|f| f.name.eq_ignore_ascii_case(&family));
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
                .ok_or_else(|| format!("unknown symbol {name:?} (try `drawskill symbols list`)"))?;
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
        std::fs::read_to_string(path).map_err(|e| format!("cannot read text file {path}: {e}").into())
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
        PropValue::List(items) => serde_json::Value::Array(items.iter().map(prop_value_json).collect()),
    }
}
