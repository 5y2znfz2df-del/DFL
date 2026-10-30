use std::path::PathBuf;
use std::process::Command;

pub struct JavaInfo {
    pub path: PathBuf,
    pub major: u32,
    pub full: String,
}

pub fn detect(config_path: &str) -> Option<JavaInfo> {
    if !config_path.trim().is_empty() {
        if let Some(j) = probe(PathBuf::from(config_path.trim())) {
            return Some(j);
        }
    }
    if let Ok(paths) = std::env::var("PATH") {
        for dir in std::env::split_paths(&paths) {
            let cand = dir.join("java.exe");
            if cand.exists() {
                if let Some(j) = probe(cand) {
                    return Some(j);
                }
            }
        }
    }
    for root in [
        r"C:\Program Files\Java",
        r"C:\Program Files (x86)\Java",
        r"C:\Program Files\Eclipse Adoptium",
        r"C:\Program Files\Microsoft\jdk",
        r"C:\Program Files\Zulu",
    ] {
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                let cand = entry.path().join("bin").join("java.exe");
                if cand.exists() {
                    if let Some(j) = probe(cand) {
                        return Some(j);
                    }
                }
            }
        }
    }
    None
}

pub fn probe(path: PathBuf) -> Option<JavaInfo> {
    let out = Command::new(&path).arg("-version").output().ok()?;
    let text = String::from_utf8_lossy(&out.stderr).to_string();
    let major = parse_major(&text)?;
    let full = text.lines().next().unwrap_or("").trim().to_string();
    Some(JavaInfo { path, major, full })
}

fn parse_major(text: &str) -> Option<u32> {
    let idx = text.find("version \"")? + "version \"".len();
    let rest = &text[idx..];
    let ver: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '_')
        .collect();
    if ver.starts_with("1.") {
        ver.split('.').nth(1)?.parse().ok()
    } else {
        ver.split('.').next()?.parse().ok()
    }
}
