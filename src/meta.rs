use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

use crate::util;

#[derive(Debug, Clone, Deserialize)]
pub struct VersionJson {
    pub id: String,
    #[serde(default, rename = "mainClass")]
    pub main_class: String,
    #[serde(default)]
    pub assets: String,
    pub downloads: Downloads,
    #[serde(default, rename = "assetIndex")]
    pub asset_index: Option<AssetIndex>,
    #[serde(default)]
    pub libraries: Vec<Library>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Downloads {
    pub client: ClientDownload,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClientDownload {
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndex {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Library {
    pub name: String,
    #[serde(default)]
    pub downloads: Option<LibDownloads>,
    #[serde(default)]
    pub rules: Option<Vec<Rule>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LibDownloads {
    #[serde(default)]
    pub artifact: Option<Artifact>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Artifact {
    pub path: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub action: String,
    #[serde(default)]
    pub os: Option<OsRule>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OsRule {
    #[serde(default)]
    pub name: Option<String>,
}

impl Rule {
    /// 当前是否为 Windows x64 允许该依赖
    pub fn allows_windows(&self) -> bool {
        if let Some(os) = &self.os {
            if let Some(name) = &os.name {
                if name != "windows" {
                    return self.action == "disallow";
                }
            }
        }
        self.action == "allow"
    }
}

impl Library {
    pub fn allowed(&self) -> bool {
        match &self.rules {
            None => true,
            Some(rules) => {
                let mut allowed = false;
                for r in rules {
                    if r.allows_windows() {
                        allowed = r.action == "allow";
                    }
                }
                allowed
            }
        }
    }
}

pub fn version_dir(id: &str) -> std::path::PathBuf {
    util::versions_dir().join(id)
}

pub fn save_json(id: &str, raw: &str) -> Result<()> {
    let dir = version_dir(id);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(format!("{id}.json")), raw)?;
    Ok(())
}

pub fn load_json(id: &str) -> Result<VersionJson> {
    let path = version_dir(id).join(format!("{id}.json"));
    let text = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&text)?)
}

pub fn installed(id: &str) -> bool {
    let dir = version_dir(id);
    dir.join(format!("{id}.json")).exists() && dir.join(format!("{id}.jar")).exists()
}

pub fn jar_path(id: &str) -> std::path::PathBuf {
    version_dir(id).join(format!("{id}.jar"))
}

pub fn asset_index_path(id: &str) -> std::path::PathBuf {
    util::assets_dir().join("indexes").join(format!("{id}.json"))
}

pub fn _exists(p: &Path) -> bool {
    p.exists()
}
