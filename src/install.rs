use std::sync::mpsc::Sender;
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
        let jar_url = net::client_jar_url(&base, &entry.id, &vj.downloads.client.url);
        net::download(
            &client,
            &jar_url,
            &meta::jar_path(&entry.id),
            Some(&vj.downloads.client.sha1),
            &tx,
        )?;

        if let Some(ai) = &vj.asset_index {
            let _ = tx.send(net::Msg::Stage("下载资源索引".to_string()));
            let mirror_url = net::asset_index_url(&base, &ai.id, &ai.url);
            let path = meta::asset_index_path(&ai.id);
            // 镜像可能没有索引文件，失败后回退官方地址
            let result = net::download(&client, &mirror_url, &path, Some(&ai.sha1), &tx)
                .or_else(|_| net::download(&client, &ai.url, &path, Some(&ai.sha1), &tx));
            if let Err(e) = result {
                let _ = tx.send(net::Msg::Log(format!("资源索引下载失败（可稍后重试）: {e}")));
            }
        }

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
