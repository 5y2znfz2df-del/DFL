#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod util;

use app::{LauncherApp, APP_NAME};

fn main() -> eframe::Result<()> {
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
