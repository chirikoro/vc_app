//! 下部ログ。

use crate::app::{App, LogLevel};
use crate::strings as s;

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(s::LOG_HEADER).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button(s::CLEAR_LOG).clicked() {
                app.log.clear();
            }
        });
    });
    let ok_color = egui::Color32::from_rgb(0x3c, 0xb3, 0x71);
    egui::ScrollArea::vertical()
        .id_salt("log_scroll")
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for line in &app.log {
                let color = match line.level {
                    LogLevel::Info => ui.visuals().text_color(),
                    LogLevel::Ok => ok_color,
                    LogLevel::Warn => ui.visuals().warn_fg_color,
                    LogLevel::Error => ui.visuals().error_fg_color,
                };
                ui.label(egui::RichText::new(&line.text).color(color).small());
            }
        });
}
