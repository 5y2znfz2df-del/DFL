use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::util;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Java 可执行文件路径，空表示自动检测
    pub java_path: String,
    /// 最大内存 (MB)
    pub max_memory_mb: u32,
    /// 最小内存 (MB)
    pub min_memory_mb: u32,
    /// 额外 JVM 参数
    pub jvm_args: String,
    /// 额外游戏参数
    pub game_args: String,
    /// 下载源: bmclapi / official / custom
    pub mirror: String,
    /// 自定义下载源根地址
    pub custom_mirror: String,
    /// 自动检测并下载 Java
    pub auto_java: bool,
    /// 窗口主题: dark / light
    pub theme: String,
    /// 用户名（离线）
    pub username: String,
    /// 账号类型: offline / microsoft / yggdrasil
    pub account_type: String,
    /// 上次选择的版本
    pub last_version: String,
    /// 上次选择的实例
    pub last_instance: String,
    /// 项目仓库地址（用于 Star 页面）
    pub github_url: String,
    /// 自定义数据目录（空 = exe 同级 data/）
    pub data_dir: String,
    /// 微软登录用 Azure 客户端 ID
    pub ms_client_id: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            java_path: String::new(),
            max_memory_mb: 4096,
            min_memory_mb: 512,
            jvm_args: String::new(),
            game_args: String::new(),
            mirror: "bmclapi".to_string(),
            custom_mirror: String::new(),
            auto_java: true,
            theme: "light".to_string(),
            username: "Steve".to_string(),
            account_type: "offline".to_string(),
            last_version: String::new(),
            last_instance: String::new(),
            github_url: "https://github.com/5y2znfz2df-del/DFL".to_string(),
            data_dir: String::new(),
            ms_client_id: String::new(),
        }
    }
}

impl Config {
    pub fn load_from(path: &std::path::Path) -> Self {
        if let Ok(text) = std::fs::read_to_string(path) {
            let text = text.trim_start_matches('\u{feff}');
            if let Ok(cfg) = serde_json::from_str::<Config>(text) {
                return cfg;
            }
        }
        Config::default()
    }

    pub fn load() -> Self {
        Self::load_from(&util::config_path())
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = util::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// 当前生效的下载源根地址
    pub fn mirror_base(&self) -> &str {
        match self.mirror.as_str() {
            "official" => "https://piston-meta.mojang.com",
            "custom" => {
                if self.custom_mirror.trim().is_empty() {
                    "https://bmclapi2.bangbang93.com"
                } else {
                    self.custom_mirror.trim()
                }
            }
            _ => "https://bmclapi2.bangbang93.com",
        }
    }
}

pub fn exists(path: &Path) -> bool {
    path.exists()
}
