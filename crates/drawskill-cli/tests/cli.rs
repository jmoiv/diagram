//! Integration tests that drive the built `drawskill` binary end-to-end.

use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_drawskill")
}

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

fn tmp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("drawskill-it-{name}"))
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(bin())
        .args(args)
        .output()
        .expect("failed to spawn drawskill")
}

#[test]
fn renders_every_example_to_all_formats() {
    for example in [
        "rc_filter",
        "ic_atmega",
        "dfd_login",
        "note_card",
        "shapes_flowchart",
    ] {
        let input = examples_dir().join(format!("{example}.yaml"));
        assert!(input.exists(), "missing example {input:?}");

        for ext in ["svg", "png", "pdf"] {
            let out = tmp(&format!("{example}.{ext}"));
            let _ = std::fs::remove_file(&out);
            let output = run(&[
                "render",
                input.to_str().unwrap(),
                "-o",
                out.to_str().unwrap(),
            ]);
            assert!(
                output.status.success(),
                "render {example} -> {ext} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let bytes = std::fs::read(&out).expect("output file should exist");
            assert!(!bytes.is_empty(), "{example}.{ext} is empty");
            match ext {
                "svg" => {
                    let s = String::from_utf8_lossy(&bytes);
                    assert!(s.contains("<svg"), "{example}.svg missing <svg");
                    assert!(s.trim_end().ends_with("</svg>"));
                }
                "png" => assert_eq!(
                    &bytes[0..8],
                    &[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n']
                ),
                "pdf" => assert_eq!(&bytes[0..5], b"%PDF-"),
                _ => unreachable!(),
            }
            let _ = std::fs::remove_file(&out);
        }
    }
}

/// Render an example to SVG (to stdout) and return the document text.
fn render_svg(example: &str) -> String {
    let input = examples_dir().join(format!("{example}.yaml"));
    let output = run(&[
        "render",
        input.to_str().unwrap(),
        "-o",
        "-",
        "--format",
        "svg",
    ]);
    assert!(
        output.status.success(),
        "render {example} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn schematic_svg_snapshot_has_expected_elements() {
    let svg = render_svg("rc_filter");
    // Resistor zig-zag (polyline), voltage-source circle, and the value labels.
    assert!(
        svg.contains("<polyline"),
        "expected resistor zig-zag polyline"
    );
    assert!(svg.contains("<circle"), "expected voltage-source circle");
    assert!(svg.contains("1k\u{2126}"), "expected '1kΩ' resistor label");
    assert!(svg.contains("100nF"), "expected '100nF' capacitor label");
    assert!(svg.contains("5V"), "expected '5V' source label");
}

#[test]
fn ic_svg_snapshot_has_name_and_pins() {
    let svg = render_svg("ic_atmega");
    assert!(svg.contains(">ATmega<"), "expected chip name");
    for pin in ["VCC", "GND", "RESET", "PB0", "XTAL1"] {
        assert!(
            svg.contains(&format!(">{pin}<")),
            "expected pin label {pin}"
        );
    }
}

#[test]
fn dfd_svg_snapshot_has_flows_and_shapes() {
    let svg = render_svg("dfd_login");
    assert!(svg.contains(">User<"));
    assert!(svg.contains(">Validate<"));
    assert!(svg.contains(">credentials<"), "expected flow label");
    assert!(svg.contains("<polyline"), "expected connection polyline");
}

#[test]
fn render_to_stdout_with_explicit_format() {
    let input = examples_dir().join("rc_filter.yaml");
    let output = run(&[
        "render",
        input.to_str().unwrap(),
        "-o",
        "-",
        "--format",
        "svg",
    ]);
    assert!(output.status.success());
    let s = String::from_utf8_lossy(&output.stdout);
    assert!(s.contains("<svg"));
    assert!(s.contains("</svg>"));
}

#[test]
fn render_unknown_symbol_fails_cleanly() {
    let bad = tmp("bad.yaml");
    std::fs::write(&bad, "root:\n  symbol: nope.nothing\n").unwrap();
    let out = tmp("bad.svg");
    let output = run(&["render", bad.to_str().unwrap(), "-o", out.to_str().unwrap()]);
    assert!(
        !output.status.success(),
        "expected failure for unknown symbol"
    );
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(err.contains("unknown symbol"), "stderr was: {err}");
    let _ = std::fs::remove_file(&bad);
}

#[test]
fn measure_single_line_json() {
    let f = tmp("measure-line.txt");
    std::fs::write(&f, "Hello").unwrap();
    let output = run(&[
        "measure",
        "--text-file",
        f.to_str().unwrap(),
        "--size",
        "14",
    ]);
    assert!(output.status.success());
    let s = String::from_utf8_lossy(&output.stdout);
    assert!(s.contains("\"mode\": \"line\""));
    assert!(s.contains("\"width\""));
    assert!(s.contains("\"ascent\""));
    let _ = std::fs::remove_file(&f);
}

#[test]
fn measure_paragraph_handles_shell_metacharacters() {
    // The whole point of reading from a file: quotes, $, backticks, newlines are inert.
    let f = tmp("measure-para.txt");
    std::fs::write(&f, "the quick \"brown\" $fox `jumps`\nover the lazy dog").unwrap();
    let output = run(&[
        "measure",
        "--text-file",
        f.to_str().unwrap(),
        "--size",
        "14",
        "--width",
        "120",
    ]);
    assert!(output.status.success());
    let s = String::from_utf8_lossy(&output.stdout);
    assert!(s.contains("\"mode\": \"paragraph\""));
    assert!(s.contains("\"lines\""));
    let _ = std::fs::remove_file(&f);
}

#[test]
fn measure_reads_stdin_with_dash() {
    use std::io::Write;
    let mut child = Command::new(bin())
        .args(["measure", "--text-file", "-", "--size", "12"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"piped text")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("\"mode\": \"line\""));
}

#[test]
fn fonts_list_returns_families() {
    let output = run(&["fonts", "list"]);
    assert!(output.status.success());
    let s = String::from_utf8_lossy(&output.stdout);
    assert!(s.contains("\"families\""));
    assert!(s.contains("\"count\""));
}

#[test]
fn symbols_list_includes_both_plugins() {
    let output = run(&["symbols", "list"]);
    assert!(output.status.success());
    let s = String::from_utf8_lossy(&output.stdout);
    assert!(s.contains("schematic"));
    assert!(s.contains("dataflow"));
    assert!(s.contains("schematic.ic"));
}

#[test]
fn symbols_describe_ic_reports_pins() {
    let output = run(&["symbols", "describe", "schematic.ic"]);
    assert!(output.status.success());
    let s = String::from_utf8_lossy(&output.stdout);
    assert!(s.contains("left_pins"));
    assert!(s.contains("right_pins"));
    assert!(s.contains("\"kind\": \"list\""));
}

#[test]
fn symbols_describe_unknown_fails() {
    let output = run(&["symbols", "describe", "schematic.nope"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown symbol"));
}
