use std::path::PathBuf;

/// 启动器数据目录：优先 EXE 同级的 data/（绿色便携），不可写时回退到 %APPDATA%\DFL
pub fn base_dir() -> PathBuf {
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
    {
        let data = exe_dir.join("data");
        if std::fs::create_dir_all(&data).is_ok() && is_writable(&data) {
            return data;
        }
    }
    let dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("DFL");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn is_writable(dir: &std::path::Path) -> bool {
    let probe = dir.join(".write_probe");
    let ok = std::fs::write(&probe, b"1").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

pub fn config_path() -> PathBuf {
    base_dir().join("config.json")
}

pub fn versions_dir() -> PathBuf {
    base_dir().join("versions")
}

pub fn libraries_dir() -> PathBuf {
    base_dir().join("libraries")
}

pub fn assets_dir() -> PathBuf {
    base_dir().join("assets")
}

pub fn instances_dir() -> PathBuf {
    base_dir().join("instances")
}

pub fn ensure_dirs() {
    for d in [versions_dir(), libraries_dir(), assets_dir(), instances_dir()] {
        let _ = std::fs::create_dir_all(d);
    }
}

/// 从内嵌图片生成窗口图标（缩放到 256px，避免占用过多内存）
pub fn load_icon(bytes: &[u8]) -> Option<egui::IconData> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = (img.width(), img.height());
    let side = 256u32;
    let resized = if w > side || h > side {
        let scale = side as f32 / w.max(h) as f32;
        image::imageops::resize(
            &img,
            ((w as f32 * scale) as u32).max(1),
            ((h as f32 * scale) as u32).max(1),
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        img
    };
    Some(egui::IconData {
        width: resized.width(),
        height: resized.height(),
        rgba: resized.into_raw(),
    })
}

/// 打开系统浏览器
pub fn open_url(url: &str) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = url;
    }
}
