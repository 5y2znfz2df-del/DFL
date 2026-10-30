use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::util;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Instance {
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub minecraft: String,
    #[serde(default)]
    pub loader: String,
    #[serde(default)]
    pub loader_version: String,
    /// 0 = 跟随全局设置
    #[serde(default)]
    pub memory_mb: u32,
    #[serde(default)]
    pub jvm_args: String,
}

pub fn dir(name: &str) -> std::path::PathBuf {
    util::instances_dir().join(name)
}

pub fn meta_path(name: &str) -> std::path::PathBuf {
    dir(name).join("instance.json")
}

pub fn list() -> Vec<Instance> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(util::instances_dir()) {
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let mut inst = std::fs::read_to_string(meta_path(&name))
                .ok()
                .and_then(|t| serde_json::from_str::<Instance>(&t).ok())
                .unwrap_or_default();
            if inst.name.is_empty() {
                inst.name = name;
            }
            out.push(inst);
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn save(inst: &Instance) -> Result<()> {
    let d = dir(&inst.name);
    std::fs::create_dir_all(&d)?;
    std::fs::write(meta_path(&inst.name), serde_json::to_string_pretty(inst)?)?;
    Ok(())
}

pub fn delete(name: &str) -> Result<()> {
    let d = dir(name);
    if d.exists() {
        std::fs::remove_dir_all(d)?;
    }
    Ok(())
}

pub fn load(name: &str) -> Option<Instance> {
    if name.trim().is_empty() {
        return None;
    }
    std::fs::read_to_string(meta_path(name))
        .ok()
        .and_then(|t| serde_json::from_str::<Instance>(&t).ok())
}

/// 列出某个资源目录中的文件：(路径, 是否启用)
pub fn list_files(sub: &str, instance: &str) -> Vec<(std::path::PathBuf, bool)> {
    let base = if instance.trim().is_empty() {
        util::instances_dir().join("_global")
    } else {
        dir(instance)
    };
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(base.join(sub)) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                let enabled = !name.contains(".disabled");
                out.push((path, enabled));
            }
        }
    }
    out.sort();
    out
}

pub fn toggle(path: &std::path::Path) -> Result<()> {
    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
    let parent = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    if let Some(stripped) = name.strip_suffix(".disabled") {
        std::fs::rename(path, parent.join(stripped))?;
    } else {
        std::fs::rename(path, parent.join(format!("{name}.disabled")))?;
    }
    Ok(())
}
