use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct LoaderEntry {
    loader: Loader,
}

#[derive(Debug, Deserialize)]
struct Loader {
    version: String,
    #[serde(default)]
    stable: bool,
}

fn http_text(url: &str) -> Result<String> {
    Ok(crate::net::client()?
        .get(url)
        .send()?
        .error_for_status()?
        .text()?)
}

/// 获取指定加载器的可用版本
pub fn loaders(kind: &str, game: &str) -> Result<Vec<String>> {
    match kind {
        "fabric" => meta_loaders(&format!("https://meta.fabricmc.net/v2/versions/loader/{game}")),
        "quilt" => meta_loaders(&format!("https://meta.quiltmc.org/v3/versions/loader/{game}")),
        "forge" => forge_versions(game),
        "neoforge" => neoforge_versions(game),
        _ => Ok(Vec::new()),
    }
}

fn meta_loaders(url: &str) -> Result<Vec<String>> {
    let list: Vec<LoaderEntry> = crate::net::client()?
        .get(url)
        .send()?
        .error_for_status()?
        .json()?;
    let mut out: Vec<String> = list
        .iter()
        .filter(|e| e.loader.stable)
        .map(|e| e.loader.version.clone())
        .collect();
    if out.is_empty() {
        out = list.into_iter().map(|e| e.loader.version).collect();
    }
    out.truncate(20);
    Ok(out)
}

fn forge_versions(game: &str) -> Result<Vec<String>> {
    let xml = http_text("https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml")?;
    let prefix = format!("{game}-");
    let mut out = Vec::new();
    for chunk in xml.split("<version>").skip(1) {
        if let Some(end) = chunk.find("</version>") {
            let v = &chunk[..end];
            if let Some(rest) = v.strip_prefix(&prefix) {
                out.push(rest.to_string());
            }
        }
    }
    out.reverse();
    if out.is_empty() {
        anyhow::bail!("该版本没有可用的 Forge")
    }
    out.truncate(20);
    Ok(out)
}

fn neoforge_versions(game: &str) -> Result<Vec<String>> {
    let xml = http_text(
        "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml",
    )?;
    let prefix = neoforge_prefix(game);
    let mut out = Vec::new();
    for chunk in xml.split("<version>").skip(1) {
        if let Some(end) = chunk.find("</version>") {
            let v = &chunk[..end];
            if v.starts_with(&prefix) {
                out.push(v.to_string());
            }
        }
    }
    out.reverse();
    if out.is_empty() {
        anyhow::bail!("该版本没有可用的 NeoForge")
    }
    out.truncate(20);
    Ok(out)
}

fn neoforge_prefix(game: &str) -> String {
    // 1.21.1 -> 21.1. ；1.20.4 -> 20.4.
    let g = game.strip_prefix("1.").unwrap_or(game);
    let parts: Vec<&str> = g.split('.').collect();
    format!(
        "{}.{}.",
        parts.first().unwrap_or(&"0"),
        parts.get(1).unwrap_or(&"0")
    )
}

/// Fabric / Quilt：直接写入 profile JSON
pub fn install_profile(kind: &str, game: &str, loader: &str) -> Result<String> {
    let base = if kind == "quilt" {
        "https://meta.quiltmc.org/v3/versions/loader"
    } else {
        "https://meta.fabricmc.net/v2/versions/loader"
    };
    let raw = http_text(&format!("{base}/{game}/{loader}/profile/json"))?;
    let value: serde_json::Value = serde_json::from_str(&raw)?;
    let id = value
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{kind}-loader-{loader}-{game}"));
    crate::meta::save_json(&id, &raw)?;
    Ok(id)
}

/// Forge / NeoForge：下载官方 installer 并执行 --installClient
pub fn install_installer(
    kind: &str,
    game: &str,
    ver: &str,
    java: &std::path::Path,
    tx: &std::sync::mpsc::Sender<crate::net::Msg>,
) -> Result<String> {
    let (url, marker) = if kind == "neoforge" {
        (
            format!(
                "https://maven.neoforged.net/releases/net/neoforged/neoforge/{ver}/neoforge-{ver}-installer.jar"
            ),
            "neoforge-",
        )
    } else {
        (
            format!(
                "https://maven.minecraftforge.net/net/minecraftforge/forge/{game}-{ver}/forge-{game}-{ver}-installer.jar"
            ),
            "forge-",
        )
    };

    let dl_dir = crate::util::base_dir().join("installers");
    std::fs::create_dir_all(&dl_dir)?;
    let jar = dl_dir.join(format!("{kind}-{game}-{ver}-installer.jar"));
    let _ = tx.send(crate::net::Msg::Stage(format!("下载 {kind} 安装器")));
    crate::net::download(&crate::net::client()?, &url, &jar, None, tx)?;

    let _ = tx.send(crate::net::Msg::Stage(format!("运行 {kind} 安装器")));
    let base = crate::util::base_dir();
    let output = std::process::Command::new(java)
        .arg("-jar")
        .arg(&jar)
        .arg("--installClient")
        .arg(&base)
        .current_dir(&base)
        .output()?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("安装器失败: {}", err.lines().last().unwrap_or("未知错误"));
    }

    // 安装器会把版本写入 versions/{forge-...}
    let mut best: Option<(std::time::SystemTime, String)> = None;
    if let Ok(entries) = std::fs::read_dir(crate::util::versions_dir()) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with(marker) {
                continue;
            }
            if let Ok(meta) = entry.metadata() {
                let t = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
                if best.as_ref().map(|(bt, _)| t > *bt).unwrap_or(true) {
                    best = Some((t, name));
                }
            }
        }
    }
    best.map(|(_, name)| name)
        .ok_or_else(|| anyhow::anyhow!("安装器没有生成版本目录"))
}
