#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod install;
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
