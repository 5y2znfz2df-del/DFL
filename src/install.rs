use std::sync::mpsc::Sender;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use crate::{config::Config, meta, net};

/// 在后台线程执行：版本 JSON → client.jar → 资源索引
pub fn install_version(cfg: &Config, entry: &net::VersionEntry, tx: Sender<net::Msg>) {
    let base = cfg.mirror_base().to_string();
    let result = (|| -> anyhow::Result<String> {
        let client = net::client()?;

        let _ = tx.send(net::Msg::Stage(format!("获取版本信息 {}", entry.id)));
        let url = net::version_json_url(&base, &entry.id, &entry.url);
        let raw = match get_text(&client, &url) {
            Ok(text) => text,
            Err(_) if !entry.url.is_empty() && base != net::OFFICIAL => {
                let _ = tx.send(net::Msg::Log("镜像版本信息请求失败，回退官方源".to_string()));
                get_text(&client, &entry.url)?
            }
            Err(e) => return Err(e),
        };
        meta::save_json(&entry.id, &raw)?;
        let vj: meta::VersionJson = serde_json::from_str(&raw)?;
        let _ = tx.send(net::Msg::Log(format!(
            "版本 JSON 已保存（mainClass={}）",
            vj.main_class
        )));

        let _ = tx.send(net::Msg::Stage(format!("下载 {}.jar", entry.id)));
        let jar_url = net::client_jar_url(&base, &entry.id, vj.client_download().map(|c| c.url.as_str()).unwrap_or_default());
        net::download(
            &client,
            &jar_url,
            &meta::jar_path(&entry.id),
            vj.client_download().map(|c| c.sha1.as_str()),
            &tx,
        )?;

        if let Some(ai) = &vj.asset_index {
            let _ = tx.send(net::Msg::Stage("下载资源索引".to_string()));
            let mirror_url = net::asset_index_url(&base, &ai.id, &ai.url);
            let path = meta::asset_index_path(&ai.id);
            // 镜像可能没有索引文件，失败后回退官方地址
            let result = net::download(&client, &mirror_url, &path, Some(&ai.sha1), &tx)
                .or_else(|_| net::download(&client, &ai.url, &path, Some(&ai.sha1), &tx));
            match result {
                Ok(()) => {
                    let _ = tx.send(net::Msg::Stage("下载游戏资源文件".to_string()));
                    if let Err(e) = download_assets(&client, &path, &base, &tx) {
                        let _ = tx.send(net::Msg::Log(format!("资源文件下载失败: {e}")));
                    }
                }
                Err(e) => {
                    let _ = tx.send(net::Msg::Log(format!("资源索引下载失败（可稍后重试）: {e}")));
                }
            }
        }

        let libs = meta::resolve_libraries(&vj, &base);
        let total = libs.len();
        for (i, lib) in libs.iter().enumerate() {
            let _ = tx.send(net::Msg::Stage(format!("依赖库 {}/{}", i + 1, total)));
            if let Err(e) = net::download(&client, &lib.url, &lib.dest, Some(&lib.sha1), &tx) {
                let _ = tx.send(net::Msg::Log(format!("镜像库失败，回退官方: {e}")));
                net::download(&client, &lib.fallback, &lib.dest, Some(&lib.sha1), &tx)?;
            }
        }
        let _ = tx.send(net::Msg::Log(format!("依赖库完成，共 {total} 个")));

        Ok(entry.id.clone())
    })();

    match result {
        Ok(id) => {
            let _ = tx.send(net::Msg::Done(id));
        }
        Err(e) => {
            let _ = tx.send(net::Msg::Failed(format!("{e:#}")));
        }
    }
}

fn get_text(client: &reqwest::blocking::Client, url: &str) -> anyhow::Result<String> {
    let mut last: Option<anyhow::Error> = None;
    for _ in 0..2 {
        match client
            .get(url)
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.text())
        {
            Ok(text) => return Ok(text),
            Err(e) => {
                last = Some(e.into());
                std::thread::sleep(Duration::from_millis(300));
            }
        }
    }
    Err(last.unwrap_or_else(|| anyhow::anyhow!("请求失败")))
}

fn download_assets(
    client: &reqwest::blocking::Client,
    index_path: &std::path::Path,
    base: &str,
    tx: &Sender<net::Msg>,
) -> anyhow::Result<()> {
    #[derive(serde::Deserialize)]
    struct Obj {
        hash: String,
        size: u64,
    }
    #[derive(serde::Deserialize)]
    struct Index {
        objects: std::collections::HashMap<String, Obj>,
    }

    let text = std::fs::read_to_string(index_path)?;
    let index: Index = serde_json::from_str(&text)?;
    let items: Vec<(String, u64)> = index
        .objects
        .values()
        .map(|o| (o.hash.clone(), o.size))
        .collect();
    let total = items.len();
    let _ = tx.send(net::Msg::Log(format!("资源对象 {total} 个，开始并发下载")));

    let done = AtomicUsize::new(0);
    let queue = Mutex::new(items.into_iter().enumerate());
    let base = base.to_string();
    let client = client.clone();

    std::thread::scope(|scope| {
        for _ in 0..8 {
            let queue = &queue;
            let done = &done;
            let base = &base;
            let client = &client;
            let tx = tx.clone();
            scope.spawn(move || {
                loop {
                    let next = {
                        let mut q = queue.lock().unwrap();
                        q.next()
                    };
                    let Some((_, (hash, _size))) = next else { break };
                    let dest = crate::util::assets_dir()
                        .join("objects")
                        .join(&hash[0..2])
                        .join(&hash);
                    if !dest.exists() {
                        let mirror = crate::meta::mirror_asset(base, &hash);
                        let official = crate::meta::mirror_asset(crate::net::OFFICIAL, &hash);
                        let result = crate::net::download_simple(client, &mirror, &dest)
                            .or_else(|_| crate::net::download_simple(client, &official, &dest));
                        if let Err(e) = result {
                            let _ = tx.send(net::Msg::Log(format!("资源 {hash} 下载失败: {e}")));
                        }
                    }
                    let n = done.fetch_add(1, Ordering::SeqCst) + 1;
                    if n % 128 == 0 || n == total {
                        let _ = tx.send(net::Msg::Stage(format!("资源文件 {n}/{total}")));
                    }
                }
            });
        }
    });

    let _ = tx.send(net::Msg::Log(format!("资源文件完成，共 {total} 个")));
    Ok(())
}

/// 安装版本 + 可选 Fabric + 创建命名实例
pub fn install_full(
    cfg: &Config,
    entry: &net::VersionEntry,
    instance: &str,
    loader: &str,
    loader_ver: &str,
    tx: Sender<net::Msg>,
) {
    let result = (|| -> anyhow::Result<String> {
        install_version(cfg, entry, tx.clone());
        let base = cfg.mirror_base().to_string();
        let mut version_id = entry.id.clone();

        if !loader.is_empty() {
            let mut ver = loader_ver.to_string();
            if ver.is_empty() {
                ver = crate::fabric::loaders(loader, &entry.id)
                    .ok()
                    .and_then(|v| v.into_iter().next())
                    .unwrap_or_default();
            }
            version_id = if loader == "fabric" || loader == "quilt" {
                let _ = tx.send(net::Msg::Stage(format!("安装 {} Loader", loader)));
                crate::fabric::install_profile(loader, &entry.id, &ver)?
            } else {
                let java = crate::java::ensure(&cfg.java_path, 17, &tx)?;
                crate::fabric::install_installer(loader, &entry.id, &ver, &java.path, &tx)?
            };
            let eff = crate::meta::load_effective(&version_id)?;
            let client = net::client()?;
            for lib in crate::meta::resolve_libraries(&eff.json, &base) {
                let _ = tx.send(net::Msg::Stage(format!(
                    "{} 依赖 {}",
                    loader,
                    lib.dest.file_name().unwrap_or_default().to_string_lossy()
                )));
                net::download(&client, &lib.url, &lib.dest, None, &tx)?;
            }
            let _ = tx.send(net::Msg::Done(version_id.clone()));
        }

        let dir = crate::util::instances_dir().join(instance);
        for sub in ["mods", "saves", "resourcepacks", "shaderpacks", "config", "logs"] {
            std::fs::create_dir_all(dir.join(sub))?;
        }
        let meta = serde_json::json!({
            "name": instance,
            "version": version_id,
            "minecraft": entry.id,
            "loader": loader,
            "loader_version": loader_ver,
        });
        std::fs::write(dir.join("instance.json"), serde_json::to_string_pretty(&meta)?)?;
        let _ = tx.send(net::Msg::InstanceCreated(instance.to_string()));
        let _ = tx.send(net::Msg::Info(format!("实例 {instance} 已创建")));
        Ok(version_id)
    })();
    if let Err(e) = result {
        let _ = tx.send(net::Msg::Failed(format!("安装失败: {e:#}")));
    }
}
