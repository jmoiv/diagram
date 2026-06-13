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
    let binary = extract(archive, &data)?;
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
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Ok("linux-x86_64.tar.gz"),
        ("linux", "aarch64") => Ok("linux-aarch64.tar.gz"),
        ("macos", "x86_64") => Ok("macos-x86_64.tar.gz"),
        ("macos", "aarch64") => Ok("macos-aarch64.tar.gz"),
        ("windows", "x86_64") => Ok("windows-x86_64.zip"),
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

fn extract(archive: &str, data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if archive.ends_with(".tar.gz") {
        extract_tar_gz(data)
    } else {
        extract_zip(data)
    }
}

fn extract_tar_gz(data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use flate2::read::GzDecoder;
    use std::ffi::OsStr;
    use tar::Archive;

    let gz = GzDecoder::new(data);
    let mut ar = Archive::new(gz);
    for entry in ar.entries()? {
        let mut entry = entry?;
        if entry.path()?.file_name() == Some(OsStr::new("diagram")) {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            return Ok(bytes);
        }
    }
    Err("could not find 'diagram' binary in archive".into())
}

fn extract_zip(data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use zip::ZipArchive;

    let cursor = std::io::Cursor::new(data);
    let mut ar = ZipArchive::new(cursor)?;
    for i in 0..ar.len() {
        let mut file = ar.by_index(i)?;
        let name = file.name().to_string();
        if name.ends_with("/diagram.exe") || name == "diagram.exe" {
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            return Ok(bytes);
        }
    }
    Err("could not find 'diagram.exe' in archive".into())
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
