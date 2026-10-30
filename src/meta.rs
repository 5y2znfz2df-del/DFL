use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::{net, util};

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
    #[serde(default)]
    pub arguments: Option<Arguments>,
    #[serde(default, rename = "minecraftArguments")]
    pub minecraft_arguments: Option<String>,
    #[serde(default, rename = "javaVersion")]
    pub java_version: Option<JavaVersion>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JavaVersion {
    #[serde(default, rename = "majorVersion")]
    pub major_version: u32,
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
    #[serde(default)]
    pub natives: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LibDownloads {
    #[serde(default)]
    pub artifact: Option<Artifact>,
    #[serde(default)]
    pub classifiers: Option<HashMap<String, Artifact>>,
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
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OsRule {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
}

impl Rule {
    pub fn matches(&self) -> bool {
        if let Some(os) = &self.os {
            if let Some(name) = &os.name {
                if name != "windows" {
                    return false;
                }
            }
            if let Some(arch) = &os.arch {
                if arch != "x86_64" && arch != "x64" {
                    return false;
                }
            }
        }
        if let Some(features) = &self.features {
            // 启动器默认不启用 demo / custom resolution 等特性
            if features.values().any(|v| *v) {
                return false;
            }
        }
        true
    }
}

fn rules_allow(rules: &Option<Vec<Rule>>) -> bool {
    match rules {
        None => true,
        Some(rules) => {
            let mut allowed = false;
            for r in rules {
                if r.matches() {
                    allowed = r.action == "allow";
                }
            }
            allowed
        }
    }
}

impl Library {
    pub fn allowed(&self) -> bool {
        rules_allow(&self.rules)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<serde_json::Value>,
    #[serde(default)]
    pub jvm: Vec<serde_json::Value>,
}

/// 解析带 rules 的参数项，返回需要追加的参数
pub fn collect_args(items: &[serde_json::Value], map: &[(String, String)]) -> Vec<String> {
    let mut out = Vec::new();
    for item in items {
        match item {
            serde_json::Value::String(s) => out.push(subst(s, map)),
            serde_json::Value::Object(obj) => {
                let rules: Option<Vec<Rule>> = obj
                    .get("rules")
                    .and_then(|v| serde_json::from_value(v.clone()).ok());
                if !rules_allow(&rules) {
                    continue;
                }
                if let Some(v) = obj.get("value") {
                    match v {
                        serde_json::Value::String(s) => out.push(subst(s, map)),
                        serde_json::Value::Array(arr) => {
                            for a in arr {
                                if let Some(s) = a.as_str() {
                                    out.push(subst(s, map));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    out
}

pub fn subst(input: &str, map: &[(String, String)]) -> String {
    let mut out = input.to_string();
    for (k, v) in map {
        out = out.replace(&format!("${{{k}}}"), v);
    }
    out
}

pub struct LibraryFile {
    pub url: String,
    pub fallback: String,
    pub dest: PathBuf,
    pub sha1: String,
    pub size: u64,
}

/// 生成需要下载/加入 classpath 的依赖库列表（Windows x64）
pub fn resolve_libraries(vj: &VersionJson, base: &str) -> Vec<LibraryFile> {
    let mut out = Vec::new();
    for lib in &vj.libraries {
        if !lib.allowed() {
            continue;
        }
        if let Some(dl) = &lib.downloads {
            if let Some(a) = &dl.artifact {
                out.push(LibraryFile {
                    url: mirror_maven(base, &a.url),
                    fallback: a.url.clone(),
                    dest: util::libraries_dir().join(&a.path),
                    sha1: a.sha1.clone(),
                    size: a.size,
                });
            }
        }
    }
    out
}

fn mirror_maven(base: &str, official: &str) -> String {
    if base == net::OFFICIAL {
        official.to_string()
    } else {
        official.replace("https://libraries.minecraft.net", &format!("{base}/maven"))
    }
}

/// 资源对象镜像地址
pub fn mirror_asset(base: &str, hash: &str) -> String {
    let prefix = &hash[0..2];
    if base == net::OFFICIAL {
        format!("https://resources.download.minecraft.net/{prefix}/{hash}")
    } else {
        format!("{base}/assets/{prefix}/{hash}")
    }
}

pub fn version_dir(id: &str) -> PathBuf {
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

pub fn jar_path(id: &str) -> PathBuf {
    version_dir(id).join(format!("{id}.jar"))
}

pub fn asset_index_path(id: &str) -> PathBuf {
    util::assets_dir().join("indexes").join(format!("{id}.json"))
}

pub fn installed(id: &str) -> bool {
    jar_path(id).exists() && version_dir(id).join(format!("{id}.json")).exists()
}
