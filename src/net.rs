use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;
use sha1::{Digest, Sha1};

pub const OFFICIAL: &str = "https://piston-meta.mojang.com";
pub const BMCLAPI: &str = "https://bmclapi2.bangbang93.com";

#[derive(Debug, Clone, Deserialize)]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub time: String,
    #[serde(rename = "releaseTime")]
    pub release_time: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub latest: Latest,
    pub versions: Vec<VersionEntry>,
}

/// 进度/日志消息，由后台线程发往 UI
pub enum Msg {
    Manifest(Box<Manifest>),
    Stage(String),
    Progress(u64, u64),
    Log(String),
    Done(String),
    Failed(String),
    GameStarted(u32),
    GameExited(i32),
}

pub fn client() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .user_agent(format!("DFL/{}", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .http1_only()
        .build()?)
}

pub fn manifest_url(base: &str) -> String {
    format!("{base}/mc/game/version_manifest_v2.json")
}

fn fetch_manifest_once(base: &str) -> Result<Manifest> {
    let resp = client()?
        .get(manifest_url(base))
        .send()?
        .error_for_status()?;
    Ok(resp.json()?)
}

/// 拉取版本清单：失败重试，镜像失败自动回退官方源
pub fn fetch_manifest(base: &str) -> Result<Manifest> {
    let mut last: Option<anyhow::Error> = None;
    for _ in 0..2 {
        match fetch_manifest_once(base) {
            Ok(m) => return Ok(m),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(Duration::from_millis(300));
            }
        }
    }
    if base != OFFICIAL {
        match fetch_manifest_once(OFFICIAL) {
            Ok(m) => return Ok(m),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| anyhow::anyhow!("未知网络错误")))
}

/// 版本 JSON：官方使用 manifest 里的 url，镜像走 /version/{id}/json
pub fn version_json_url(base: &str, id: &str, official_url: &str) -> String {
    if base == OFFICIAL {
        official_url.to_string()
    } else {
        format!("{base}/version/{id}/json")
    }
}

/// 客户端 jar：镜像走 /version/{id}/client
pub fn client_jar_url(base: &str, id: &str, official_url: &str) -> String {
    if base == OFFICIAL {
        official_url.to_string()
    } else {
        format!("{base}/version/{id}/client")
    }
}

/// 资源索引：镜像走 /assets/indexes/{sha1}.json
pub fn asset_index_url(base: &str, index_id: &str, official_url: &str) -> String {
    if base == OFFICIAL {
        official_url.to_string()
    } else {
        format!("{base}/assets/indexes/{index_id}.json")
    }
}

pub fn sha1_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// 带进度回调的下载，已存在且 SHA1 匹配则跳过
pub fn download(
    client: &reqwest::blocking::Client,
    url: &str,
    dest: &Path,
    expect_sha1: Option<&str>,
    tx: &std::sync::mpsc::Sender<Msg>,
) -> Result<()> {
    if dest.exists() {
        if let Some(expected) = expect_sha1 {
            if sha1_file(dest)?.eq_ignore_ascii_case(expected) {
                let _ = tx.send(Msg::Log(format!(
                    "跳过（已存在）: {}",
                    dest.file_name().unwrap_or_default().to_string_lossy()
                )));
                return Ok(());
            }
        }
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut resp = client.get(url).send()?.error_for_status()?;
    let total = resp.content_length().unwrap_or(0);
    let mut file = File::create(dest).with_context(|| format!("创建文件失败: {}", dest.display()))?;
    let mut buf = [0u8; 64 * 1024];
    let mut done: u64 = 0;
    let mut last_report: u64 = 0;
    loop {
        let n = resp.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        done += n as u64;
        if done - last_report > 128 * 1024 || (total > 0 && done == total) {
            last_report = done;
            let _ = tx.send(Msg::Progress(done, total));
        }
    }
    drop(file);

    if let Some(expected) = expect_sha1 {
        let actual = sha1_file(dest)?;
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = std::fs::remove_file(dest);
            anyhow::bail!("SHA1 校验失败: 期望 {expected}, 实际 {actual}");
        }
    }
    Ok(())
}

/// 静默下载（资源对象等小文件），已存在则跳过
pub fn download_simple(
    client: &reqwest::blocking::Client,
    url: &str,
    dest: &Path,
) -> Result<()> {
    if dest.exists() {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut resp = client.get(url).send()?.error_for_status()?;
    let mut file = File::create(dest)?;
    std::io::copy(&mut resp, &mut file)?;
    Ok(())
}
