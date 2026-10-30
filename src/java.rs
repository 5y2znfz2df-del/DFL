use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::Sender;

use anyhow::{Context, Result};

use crate::{net, util};

pub struct JavaInfo {
    pub path: PathBuf,
    pub major: u32,
    pub full: String,
}

#[derive(serde::Deserialize)]
struct Asset {
    binary: Binary,
}

#[derive(serde::Deserialize)]
struct Binary {
    package: Package,
}

#[derive(serde::Deserialize)]
struct Package {
    link: String,
    name: String,
}

/// 依次探测：配置路径 → PATH → 常见安装目录 → 启动器 runtime
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
    runtime_java().and_then(probe)
}

fn runtime_java() -> Option<PathBuf> {
    let root = util::base_dir().join("runtime");
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            if let Some(p) = find_java_in(&entry.path()) {
                return Some(p);
            }
        }
    }
    None
}

/// 确保有可用的 Java；没有就自动下载 Adoptium JRE
pub fn ensure(config_path: &str, required_major: u32, tx: &Sender<net::Msg>) -> Result<JavaInfo> {
    if let Some(j) = detect(config_path) {
        if j.major >= required_major {
            return Ok(j);
        }
        let _ = tx.send(net::Msg::Log(format!(
            "已安装 Java {} 低于要求的 {}，尝试自动安装",
            j.major, required_major
        )));
    }
    let path = install_jre(required_major, tx)?;
    let info = probe(path).ok_or_else(|| anyhow::anyhow!("自动安装的 Java 无法运行"))?;
    Ok(info)
}

/// 下载并解压 Adoptium JRE，返回 java.exe
pub fn install_jre(major: u32, tx: &Sender<net::Msg>) -> Result<PathBuf> {
    let runtime_root = util::base_dir().join("runtime");
    let dir = runtime_root.join(format!("jre{major}"));
    if let Some(p) = find_java_in(&dir) {
        let _ = tx.send(net::Msg::Log(format!("复用已下载的 Java {major}")));
        return Ok(p);
    }

    let _ = tx.send(net::Msg::Stage(format!("获取 Java {major} 下载地址")));
    let client = net::client()?;
    let api = format!(
        "https://api.adoptium.net/v3/assets/latest/{major}/hotspot?os=windows&arch=x64&image_type=jre"
    );
    let mut last_err = None;
    let mut package = None;
    for attempt in 0..2 {
        match client.get(&api).send().and_then(|r| r.error_for_status()) {
            Ok(resp) => match resp.json::<Vec<Asset>>() {
                Ok(list) => {
                    if let Some(a) = list.into_iter().next() {
                        package = Some(a.binary.package);
                        break;
                    }
                }
                Err(e) => last_err = Some(anyhow::Error::from(e)),
            },
            Err(e) => last_err = Some(anyhow::Error::from(e)),
        }
        if attempt == 0 {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }
    let package = package.ok_or_else(|| {
        last_err.unwrap_or_else(|| anyhow::anyhow!("未找到适合的 Java {major} 安装包"))
    })?;

    std::fs::create_dir_all(&runtime_root)?;
    let zip_path = runtime_root.join(format!("jre{major}.zip"));
    let _ = tx.send(net::Msg::Stage(format!("下载 {}", package.name)));
    let candidates = [
        format!(
            "https://mirrors.tuna.tsinghua.edu.cn/Adoptium/{major}/jre/x64/windows/{}",
            package.name
        ),
        format!(
            "https://mirrors.ustc.edu.cn/adoptium/{major}/jre/x64/windows/{}",
            package.name
        ),
        package.link.clone(),
    ];
    let dl_client = net::client_with_timeout(180)?;
    let mut last_err = None;
    let mut downloaded = false;
    for url in candidates {
        match net::download(&dl_client, &url, &zip_path, None, tx) {
            Ok(()) => {
                downloaded = true;
                break;
            }
            Err(e) => {
                let _ = tx.send(net::Msg::Log(format!("下载源失败: {e}")));
                last_err = Some(e);
            }
        }
    }
    if !downloaded {
        return Err(last_err.unwrap_or_else(|| anyhow::anyhow!("Java 下载失败")));
    }

    let _ = tx.send(net::Msg::Stage("解压 Java".to_string()));
    extract_zip(&zip_path, &dir)?;
    let _ = std::fs::remove_file(&zip_path);

    let java = find_java_in(&dir).context("解压后未找到 java.exe")?;
    let _ = tx.send(net::Msg::Log(format!("Java 已安装: {}", java.display())));
    Ok(java)
}

fn extract_zip(zip_path: &Path, out_dir: &Path) -> Result<()> {
    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let out_path = out_dir.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = std::fs::File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out)?;
        }
    }
    Ok(())
}

fn find_java_in(dir: &Path) -> Option<PathBuf> {
    let direct = dir.join("bin").join("java.exe");
    if direct.exists() {
        return Some(direct);
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let cand = p.join("bin").join("java.exe");
                if cand.exists() {
                    return Some(cand);
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
