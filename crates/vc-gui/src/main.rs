//! ボイスチェンジャー GUI(egui / eframe)。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod fonts;
#[cfg(feature = "preview")]
mod preview;
mod strings;
mod ui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(strings::APP_TITLE)
            .with_app_id("vc-app")
            .with_inner_size([1000.0, 720.0])
            .with_min_inner_size([760.0, 520.0])
            .with_drag_and_drop(true),
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        strings::APP_TITLE,
        options,
        Box::new(|cc| {
            let font_path = fonts::install(&cc.egui_ctx);
            Ok(Box::new(app::App::new(cc, font_path)))
        }),
    )
}
