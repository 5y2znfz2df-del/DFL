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
    #[serde(default)]
    pub downloads: Option<Downloads>,
    #[serde(default, rename = "inheritsFrom")]
    pub inherits_from: Option<String>,
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
    pub url: Option<String>,
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
        } else if let (Some(root), Some(path)) = (&lib.url, maven_path(&lib.name)) {
            // Fabric / Quilt 等 maven 库：name 推导路径 + url 前缀
            let full = format!("{}/{}", root.trim_end_matches('/'), path);
            out.push(LibraryFile {
                url: full.clone(),
                fallback: full,
                dest: util::libraries_dir().join(&path),
                sha1: String::new(),
                size: 0,
            });
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

pub fn maven_path(name: &str) -> Option<String> {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let (group, artifact, version) = (parts[0], parts[1], parts[2]);
    let file = match parts.get(3) {
        Some(classifier) => format!("{artifact}-{version}-{classifier}.jar"),
        None => format!("{artifact}-{version}.jar"),
    };
    Some(format!(
        "{}/{}/{}/{}",
        group.replace('.', "/"),
        artifact,
        version,
        file
    ))
}

impl VersionJson {
    pub fn client_download(&self) -> Option<&ClientDownload> {
        self.downloads.as_ref().map(|d| &d.client)
    }
}

pub struct Effective {
    pub json: VersionJson,
    pub base_id: String,
}

/// 解析 inheritsFrom：子版本（Fabric 等）与父版本合并
pub fn load_effective(id: &str) -> Result<Effective> {
    let child = load_json(id)?;
    let Some(parent_id) = child.inherits_from.clone() else {
        return Ok(Effective {
            json: child,
            base_id: id.to_string(),
        });
    };
    let parent = load_json(&parent_id)?;
    let base_id = parent
        .inherits_from
        .clone()
        .unwrap_or_else(|| parent_id.clone());

    let mut merged = child.clone();
    if merged.main_class.is_empty() {
        merged.main_class = parent.main_class.clone();
    }
    if merged.asset_index.is_none() {
        merged.asset_index = parent.asset_index.clone();
    }
    if merged.assets.is_empty() {
        merged.assets = parent.assets.clone();
    }
    if merged.downloads.is_none() {
        merged.downloads = parent.downloads.clone();
    }
    if merged.java_version.is_none() {
        merged.java_version = parent.java_version.clone();
    }

    // 库：子在前，父在后，按 name 去重
    let mut libs = child.libraries.clone();
    for l in &parent.libraries {
        if !libs.iter().any(|x| x.name == l.name) {
            libs.push(l.clone());
        }
    }
    merged.libraries = libs;

    // 参数：父在前，子在后
    match (&parent.arguments, &child.arguments) {
        (Some(p), Some(c)) => {
            merged.arguments = Some(Arguments {
                game: p.game.iter().chain(c.game.iter()).cloned().collect(),
                jvm: p.jvm.iter().chain(c.jvm.iter()).cloned().collect(),
            });
        }
        (Some(p), None) => merged.arguments = Some(p.clone()),
        _ => {}
    }

    Ok(Effective {
        json: merged,
        base_id,
    })
}
