fn main() {
    let base = std::env::var("CARGO_PKG_VERSION").unwrap();
    let is_tag = std::env::var("GITHUB_REF")
        .map(|r| r.starts_with("refs/tags/v"))
        .unwrap_or(false);
    let version = if is_tag { base } else { format!("{base}-dev") };
    println!("cargo:rustc-env=DIAGRAM_VERSION={version}");
    println!("cargo:rerun-if-env-changed=GITHUB_REF");
}
