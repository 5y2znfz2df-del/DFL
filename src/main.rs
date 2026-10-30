#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod install;
mod java;
mod launch;
mod meta;
mod net;
mod util;

use app::{LauncherApp, APP_NAME};

fn main() -> eframe::Result<()> {
    // 无界面安装模式（便于 CI / 命令行测试）: dfl.exe --install <version>
    let args: Vec<String> = std::env::args().collect();
    if let Some(idx) = args.iter().position(|a| a == "--install") {
        if let Some(id) = args.get(idx + 1) {
            let cfg = config::Config::load();
            let (tx, rx) = std::sync::mpsc::channel();
            let entry = net::VersionEntry {
                id: id.clone(),
                kind: "release".to_string(),
                url: String::new(),
                time: String::new(),
                release_time: String::new(),
            };
            std::thread::spawn(move || install::install_version(&cfg, &entry, tx));
            while let Ok(msg) = rx.recv() {
                match msg {
                    net::Msg::Stage(stage) => println!("[stage] {stage}"),
                    net::Msg::Progress(done, total) => {
                        if total > 0 { println!("[progress] {done}/{total}"); }
                    }
                    net::Msg::Log(line) => println!("[log] {line}"),
                    net::Msg::Done(id) => { println!("[done] {id}"); break; }
                    net::Msg::Failed(err) => { eprintln!("[failed] {err}"); break; }
                    _ => {}
                }
            }
            return Ok(());
        }
    }

    // 无界面启动模式（调试/CI）: dfl.exe --launch <version>
    if let Some(idx) = args.iter().position(|a| a == "--launch") {
        if let Some(id) = args.get(idx + 1) {
            let cfg = config::Config::load();
            let (tx, _rx) = std::sync::mpsc::channel();
            match launch::launch(id, &cfg, &tx) {
                Ok(mut child) => {
                    println!("[pid] {}", child.id());
                    let wait: u64 = args
                        .get(idx + 2)
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(20);
                    std::thread::sleep(std::time::Duration::from_secs(wait));
                    let _ = child.kill();
                    let _ = child.wait();
                    println!("[killed]");
                    let log = util::base_dir().join("logs").join("latest.log");
                    if let Ok(text) = std::fs::read_to_string(&log) {
                        let mut start = text.len().saturating_sub(6000);
                        while start < text.len() && !text.is_char_boundary(start) {
                            start += 1;
                        }
                        println!("--- log tail ---\n{}", &text[start..]);
                    }
                }
                Err(e) => eprintln!("[failed] {e:#}"),
            }
            return Ok(());
        }
    }

    let boot_cfg = config::Config::load();
    util::init(&boot_cfg.data_dir);

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([980.0, 650.0])
        .with_min_inner_size([880.0, 560.0])
        .with_title(APP_NAME);

    if let Some(icon) = util::load_icon(include_bytes!("../assets/icon.jpg")) {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        vsync: true,
        ..Default::default()
    };

    eframe::run_native(
        APP_NAME,
        options,
        Box::new(|cc| Ok(Box::new(LauncherApp::new(cc)))),
    )
}
