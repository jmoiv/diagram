use std::io::Read;
use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};

const CURRENT: &str = env!("DIAGRAM_VERSION");
const REPO: &str = "jmoiv/diagram";

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    if CURRENT.contains('-') {
        eprintln!("Development build ({CURRENT}) — skipping update check.");
        return Ok(());
    }

    let spinner = new_spinner("Checking for updates…");
    let latest_tag = fetch_latest_tag()?;
    spinner.finish_and_clear();

    let latest = latest_tag.trim_start_matches('v');

    if parse_version(latest) <= parse_version(CURRENT) {
        println!("Already up to date ({CURRENT}).");
        return Ok(());
    }

    println!("Updating {CURRENT} → {latest}");

    let archive = platform_archive()?;
    let url = format!("https://github.com/{REPO}/releases/download/v{latest}/{archive}");
    let data = download(archive, &url)?;

    let spinner = new_spinner("Installing…");
    let binary = extract(&data)?;
    spinner.finish_and_clear();

    install(&binary, latest)
}

fn fetch_latest_tag() -> Result<String, Box<dyn std::error::Error>> {
    let body = ureq::get(&format!(
        "https://api.github.com/repos/{REPO}/releases/latest"
    ))
    .set("User-Agent", &format!("diagram/{CURRENT}"))
    .set("Accept", "application/vnd.github+json")
    .call()
    .map_err(|e| format!("failed to check for updates: {e}"))?
    .into_string()?;

    let json: serde_json::Value = serde_json::from_str(&body)?;
    json["tag_name"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "missing tag_name in GitHub API response".into())
}

fn platform_archive() -> Result<&'static str, Box<dyn std::error::Error>> {
    // File names match `uname -s` / `uname -m` output so the shell install
    // one-liner needs no tr or sed. macOS aarch64 reports "arm64" from uname.
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Ok("Linux-x86_64.gz"),
        ("linux", "aarch64") => Ok("Linux-aarch64.gz"),
        ("macos", "x86_64") => Ok("Darwin-x86_64.gz"),
        ("macos", "aarch64") => Ok("Darwin-arm64.gz"),
        ("windows", "x86_64") => Ok("windows-x86_64.gz"),
        (os, arch) => Err(format!("unsupported platform: {os}/{arch}").into()),
    }
}

fn download(archive: &str, url: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let response = ureq::get(url)
        .set("User-Agent", &format!("diagram/{CURRENT}"))
        .call()
        .map_err(|e| format!("failed to download {archive}: {e}"))?;

    let total = response
        .header("content-length")
        .and_then(|s| s.parse::<u64>().ok());

    let pb = if let Some(len) = total {
        let pb = ProgressBar::new(len);
        pb.set_style(
            ProgressStyle::with_template(
                "Downloading {msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes}",
            )?
            .progress_chars("=> "),
        );
        pb.set_message(archive.to_string());
        pb
    } else {
        new_spinner(&format!("Downloading {archive}…"))
    };

    let mut reader = response.into_reader();
    let mut data = Vec::new();
    let mut buf = [0u8; 16384];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
        pb.inc(n as u64);
    }
    pb.finish_and_clear();

    Ok(data)
}

fn extract(data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use flate2::read::GzDecoder;
    let mut gz = GzDecoder::new(data);
    let mut bytes = Vec::new();
    gz.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn install(binary: &[u8], version: &str) -> Result<(), Box<dyn std::error::Error>> {
    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path
        .parent()
        .ok_or("cannot determine executable directory")?;
    let temp_path = exe_dir.join(".diagram-update.tmp");

    // Write to the same directory as the exe so rename stays on one device.
    match std::fs::write(&temp_path, binary) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            return permission_denied(&exe_path);
        }
        Err(e) => return Err(e.into()),
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temp_path, std::fs::Permissions::from_mode(0o755))?;
    }

    match std::fs::rename(&temp_path, &exe_path) {
        Ok(()) => {
            println!("Updated to v{version}.");
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            std::fs::remove_file(&temp_path).ok();
            permission_denied(&exe_path)
        }
        Err(e) => {
            std::fs::remove_file(&temp_path).ok();
            Err(e.into())
        }
    }
}

fn permission_denied(exe_path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!(
        "diagram: error: could not replace {} (permission denied).",
        exe_path.display()
    );
    eprintln!("Run as root:");
    eprintln!("  sudo diagram update");
    std::process::exit(1);
}

fn new_spinner(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.enable_steady_tick(Duration::from_millis(80));
    pb.set_message(msg.to_string());
    pb
}

fn parse_version(v: &str) -> (u32, u32, u32) {
    let v = v.trim_start_matches('v');
    let mut parts = v.split('.');
    let major = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let patch = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    (major, minor, patch)
}

#[cfg(test)]
mod tests {
    use super::parse_version;

    #[test]
    fn version_ordering() {
        assert!(parse_version("0.1.1") > parse_version("0.1.0"));
        assert!(parse_version("1.0.0") > parse_version("0.9.9"));
        assert!(parse_version("0.2.0") > parse_version("0.1.99"));
        assert_eq!(parse_version("0.1.0"), parse_version("v0.1.0"));
    }
}
