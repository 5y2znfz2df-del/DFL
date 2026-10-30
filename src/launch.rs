use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Sender;

use anyhow::{bail, Context, Result};

use crate::{config::Config, meta, net, util};

/// 组装参数并启动游戏进程
pub fn launch(id: &str, cfg: &Config, tx: &Sender<net::Msg>) -> Result<Child> {
    let eff = meta::load_effective(id).with_context(|| format!("版本 {id} 未安装，请先下载"))?;
    let vj = &eff.json;
    let required = vj
        .java_version
        .as_ref()
        .map(|j| j.major_version)
        .unwrap_or(17);
    let java = crate::java::ensure(&cfg.java_path, required, tx)
        .context("未找到 Java，自动安装也失败")?;
    let _ = tx.send(net::Msg::Log(format!(
        "Java: {} ({})",
        java.full,
        java.path.display()
    )));
    if let Some(jv) = &vj.java_version {
        if jv.major_version > java.major {
            let _ = tx.send(net::Msg::Log(format!(
                "警告: 该版本建议 Java {}，当前为 Java {}",
                jv.major_version, java.major
            )));
        }
    }

    let base = cfg.mirror_base();
    let mut classpath: Vec<String> = Vec::new();
    for lib in meta::resolve_libraries(vj, base) {
        if !lib.dest.exists() {
            let _ = tx.send(net::Msg::Stage(format!(
                "补全依赖 {}",
                lib.dest.file_name().unwrap_or_default().to_string_lossy()
            )));
            let client = net::client()?;
            let result = net::download(&client, &lib.url, &lib.dest, None, tx)
                .or_else(|_| net::download(&client, &lib.fallback, &lib.dest, None, tx));
            if result.is_err() {
                bail!("依赖库下载失败: {}", lib.dest.display());
            }
        }
        classpath.push(lib.dest.to_string_lossy().to_string());
    }
    let jar = meta::jar_path(&eff.base_id);
    if !jar.exists() {
        bail!("客户端 jar 缺失，请重新下载该版本");
    }
    classpath.push(jar.to_string_lossy().to_string());
    let classpath_str = classpath.join(";");

    let natives = meta::version_dir(id).join("natives");
    let _ = std::fs::create_dir_all(&natives);
    let game_dir = if cfg.last_instance.trim().is_empty() {
        util::instances_dir().join(id)
    } else {
        util::instances_dir().join(cfg.last_instance.trim())
    };
    for sub in [
        "mods",
        "saves",
        "resourcepacks",
        "shaderpacks",
        "config",
        "logs",
    ] {
        let _ = std::fs::create_dir_all(game_dir.join(sub));
    }

    let assets_dir = util::assets_dir();
    let index_id = vj
        .asset_index
        .as_ref()
        .map(|a| a.id.clone())
        .unwrap_or_else(|| vj.assets.clone());
    let accounts = crate::accounts::Accounts::load();
    let active = accounts.active_account().cloned();
    let username = active
        .as_ref()
        .map(|a| a.name.clone())
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| {
            if cfg.username.trim().is_empty() {
                "Steve".to_string()
            } else {
                cfg.username.trim().to_string()
            }
        });
    let uuid = active
        .as_ref()
        .map(|a| a.uuid.clone())
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| offline_uuid(&username));
    let access_token = active
        .as_ref()
        .map(|a| a.token.clone())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "0".to_string());
    let user_type = if active
        .as_ref()
        .map(|a| a.kind == "microsoft" || a.kind == "yggdrasil")
        .unwrap_or(false)
    {
        "msa"
    } else {
        "legacy"
    };

    let map: Vec<(String, String)> = vec![
        ("auth_player_name".into(), username),
        ("version_name".into(), id.to_string()),
        ("game_directory".into(), game_dir.to_string_lossy().to_string()),
        ("assets_root".into(), assets_dir.to_string_lossy().to_string()),
        ("assets_index_name".into(), index_id),
        ("auth_uuid".into(), uuid),
        ("auth_access_token".into(), access_token),
        ("clientid".into(), String::new()),
        ("auth_xuid".into(), String::new()),
        ("user_type".into(), user_type.into()),
        ("version_type".into(), "release".into()),
        (
            "natives_directory".into(),
            natives.to_string_lossy().to_string(),
        ),
        ("launcher_name".into(), "DFL".into()),
        ("launcher_version".into(), env!("CARGO_PKG_VERSION").into()),
        ("classpath".into(), classpath_str.clone()),
        ("classpath_separator".into(), ";".into()),
        (
            "library_directory".into(),
            util::libraries_dir().to_string_lossy().to_string(),
        ),
    ];

    let inst = crate::instance::load(cfg.last_instance.trim());
    let max_mem = inst
        .as_ref()
        .map(|i| i.memory_mb)
        .filter(|m| *m > 0)
        .unwrap_or(cfg.max_memory_mb)
        .max(256);
    // 外置登录：注入 authlib-injector
    let mut injector_arg: Option<String> = None;
    if let Some(acc) = &active {
        if acc.kind == "yggdrasil" && !acc.server.trim().is_empty() {
            match ensure_authlib_injector(tx) {
                Ok(jar) => {
                    injector_arg = Some(format!(
                        "-javaagent:{}={}",
                        jar.display(),
                        acc.server.trim()
                    ));
                }
                Err(e) => {
                    let _ = tx.send(net::Msg::Log(format!("authlib-injector 下载失败: {e}")));
                }
            }
        }
    }

    let mut args: Vec<String> = vec![
        format!("-Xms{}m", cfg.min_memory_mb.min(max_mem).max(128)),
        format!("-Xmx{}m", max_mem),
    ];
    for extra in cfg.jvm_args.split_whitespace() {
        args.push(extra.to_string());
    }
    if let Some(i) = &inst {
        for extra in i.jvm_args.split_whitespace() {
            args.push(extra.to_string());
        }
    }
    if let Some(arg) = injector_arg {
        args.push(arg);
    }

    if let Some(arguments) = &vj.arguments {
        args.extend(meta::collect_args(&arguments.jvm, &map));
    } else {
        args.push(format!("-Djava.library.path={}", natives.display()));
        args.push("-cp".to_string());
        args.push(classpath_str.clone());
    }

    args.push(vj.main_class.clone());
    if vj.main_class.is_empty() {
        bail!("版本 JSON 缺少 mainClass");
    }

    if let Some(arguments) = &vj.arguments {
        args.extend(meta::collect_args(&arguments.game, &map));
    } else if let Some(legacy) = &vj.minecraft_arguments {
        for token in legacy.split_whitespace() {
            args.push(meta::subst(token, &map));
        }
    }
    for extra in cfg.game_args.split_whitespace() {
        args.push(extra.to_string());
    }

    let logs_dir = util::base_dir().join("logs");
    std::fs::create_dir_all(&logs_dir)?;
    let mut log_file = std::fs::File::create(logs_dir.join("latest.log"))?;
    writeln!(log_file, "=== DFL 启动 {id} ===")?;
    writeln!(log_file, "Java: {}", java.path.display())?;
    writeln!(log_file, "工作目录: {}", game_dir.display())?;
    writeln!(log_file, "参数: {}", args.join(" "))?;
    writeln!(log_file, "-------------------------")?;
    let out = log_file.try_clone()?;
    let err = log_file.try_clone()?;

    let mut cmd = Command::new(&java.path);
    cmd.args(&args)
        .current_dir(&game_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err));
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }

    let child = cmd.spawn().context("启动 Java 进程失败")?;
    let _ = tx.send(net::Msg::Log(format!("已启动进程 PID {}", child.id())));
    Ok(child)
}

fn offline_uuid(name: &str) -> String {
    let digest = md5::compute(format!("OfflinePlayer:{name}").as_bytes());
    let mut u = digest.0;
    u[6] = (u[6] & 0x0f) | 0x30;
    u[8] = (u[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        u[0], u[1], u[2], u[3], u[4], u[5], u[6], u[7], u[8], u[9], u[10], u[11], u[12], u[13], u[14], u[15]
    )
}

/// 下载 authlib-injector（BMCLAPI 镜像优先）
fn ensure_authlib_injector(tx: &Sender<net::Msg>) -> Result<std::path::PathBuf> {
    let path = util::libraries_dir().join("authlib-injector.jar");
    if path.exists() {
        return Ok(path);
    }
    let client = net::client()?;
    let sources = [
        "https://bmclapi2.bangbang93.com/mirrors/authlib-injector/artifact/latest.json",
        "https://authlib-injector.yushi.moe/artifact/latest.json",
    ];
    for url in sources {
        let Ok(resp) = client.get(url).send().and_then(|r| r.error_for_status()) else {
            continue;
        };
        let Ok(value) = resp.json::<serde_json::Value>() else {
            continue;
        };
        if let Some(dl) = value.get("download_url").and_then(|v| v.as_str()) {
            let _ = tx.send(net::Msg::Stage("下载 authlib-injector".to_string()));
            net::download(&client, dl, &path, None, tx)?;
            return Ok(path);
        }
    }
    anyhow::bail!("所有镜像均不可用")
}
