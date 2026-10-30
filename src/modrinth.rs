use anyhow::Result;
use serde::Deserialize;

const API: &str = "https://api.modrinth.com/v2";

#[derive(Debug, Clone, Deserialize)]
pub struct Hit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub icon_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SearchResp {
    hits: Vec<Hit>,
}

#[derive(Debug, Clone, Deserialize)]
struct Version {
    #[serde(default)]
    files: Vec<VersionFile>,
}

#[derive(Debug, Clone, Deserialize)]
struct VersionFile {
    url: String,
    filename: String,
    #[serde(default)]
    primary: bool,
}

/// kind: mod / shader；loader: fabric / forge / 空；game: 1.20.1
pub fn search(kind: &str, loader: &str, game: &str, category: &str, query: &str, limit: u32) -> Result<Vec<Hit>> {
    let mut facets: Vec<String> = vec![format!("[\"project_type:{kind}\"]")];
    if !category.is_empty() {
        facets.push(format!("[\"categories:{category}\"]"));
    }
    if !game.is_empty() {
        facets.push(format!("[\"versions:{game}\"]"));
    }
    if !loader.is_empty() {
        facets.push(format!("[\"categories:{loader}\"]"));
    }
    let facets = format!("[{}]", facets.join(","));
    let url = format!("{API}/search");
    let resp: SearchResp = crate::net::client()?
        .get(&url)
        .query(&[
            ("limit", limit.to_string()),
            ("index", "downloads".to_string()),
            ("query", query.to_string()),
            ("facets", facets),
        ])
        .send()?
        .error_for_status()?
        .json()?;
    Ok(resp.hits)
}

/// 下载项目最新兼容版本到 dest_dir，返回文件名
pub fn install(project: &str, loader: &str, game: &str, dest_dir: &std::path::Path) -> Result<String> {
    let mut req = crate::net::client()?.get(format!("{API}/project/{project}/version"));
    if !game.is_empty() {
        req = req.query(&[("game_versions", format!("[\"{game}\"]"))]);
    }
    if !loader.is_empty() {
        req = req.query(&[("loaders", format!("[\"{loader}\"]"))]);
    }
    let versions: Vec<Version> = req.send()?.error_for_status()?.json()?;
    let file = versions
        .iter()
        .filter_map(|v| v.files.iter().find(|f| f.primary).or_else(|| v.files.first()))
        .next()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("没有适用于 {game} / {loader} 的版本"))?;

    std::fs::create_dir_all(dest_dir)?;
    let dest = dest_dir.join(&file.filename);
    if !dest.exists() {
        crate::net::download_simple(&crate::net::client()?, &file.url, &dest)?;
    }
    Ok(file.filename)
}
