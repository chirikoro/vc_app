//! 画面部品。

pub mod batch;
pub mod log;
pub mod params;
pub mod single;

use std::path::Path;

/// パスを省略表示用に整形する。
pub fn display_path(path: &Path, max_chars: usize) -> String {
    let s = path.display().to_string();
    let n = s.chars().count();
    if n <= max_chars {
        return s;
    }
    let keep = max_chars.saturating_sub(1);
    let tail: String = s.chars().skip(n - keep).collect();
    format!("…{tail}")
}

/// 「パス表示 + 右寄せボタン群」の 1 行。ボタンを先に配置し、残り幅にパスを省略表示する。
pub fn path_row(
    ui: &mut egui::Ui,
    path: Option<&std::path::Path>,
    placeholder: &str,
    buttons: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            buttons(ui);
            let label = match path {
                Some(p) => egui::Label::new(p.display().to_string()).truncate(),
                None => egui::Label::new(egui::RichText::new(placeholder).weak()).truncate(),
            };
            let resp = ui.add(label);
            if let Some(p) = path {
                resp.on_hover_text(p.display().to_string());
            }
        });
    });
}

/// ドロップ領域(ファイルをホバー中は強調表示)。
pub fn drop_zone(ui: &mut egui::Ui, hint: &str, content: impl FnOnce(&mut egui::Ui)) {
    let hovering = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
    let visuals = ui.visuals().clone();
    let stroke = if hovering {
        egui::Stroke::new(2.0, visuals.selection.stroke.color)
    } else {
        egui::Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color)
    };
    let fill = if hovering {
        visuals.selection.bg_fill.gamma_multiply(0.35)
    } else {
        visuals.faint_bg_color
    };
    egui::Frame::new()
        .stroke(stroke)
        .fill(fill)
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            content(ui);
            ui.add_space(4.0);
            ui.label(egui::RichText::new(hint).small().weak());
        });
}

pub fn format_duration(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    if secs >= 3600 {
        format!("{}時間{}分", secs / 3600, (secs % 3600) / 60)
    } else if secs >= 60 {
        format!("{}分{}秒", secs / 60, secs % 60)
    } else {
        format!("{:.1}秒", d.as_secs_f64())
    }
}
