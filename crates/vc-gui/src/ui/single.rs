//! 「単一ファイル」タブ。

use super::{display_path, drop_zone, format_duration, path_row};
use crate::app::{App, Tab};
use crate::strings as s;
use std::path::PathBuf;
use vc_core::audio_io::INPUT_EXTENSIONS;
use vc_core::batch::output_path_for;

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    let running = app.is_running();

    ui.heading(s::TAB_SINGLE);
    ui.add_space(8.0);

    // 入力ファイル
    ui.label(egui::RichText::new(s::SINGLE_INPUT).strong());
    let mut pick_input = false;
    let mut clear_input = false;
    drop_zone(ui, s::SINGLE_DROP_HINT, |ui| {
        path_row(ui, app.single_input.as_deref(), "(未選択)", |ui| {
            ui.add_enabled_ui(!running, |ui| {
                if ui.button(s::SELECT).clicked() {
                    pick_input = true;
                }
                if app.single_input.is_some() && ui.button(s::CLEAR).clicked() {
                    clear_input = true;
                }
            });
        });
    });
    if pick_input {
        let mut dialog = rfd::FileDialog::new()
            .set_title(s::SINGLE_INPUT)
            .add_filter("音声ファイル", INPUT_EXTENSIONS);
        if let Some(dir) = app
            .single_input
            .as_ref()
            .and_then(|p| p.parent())
            .filter(|d| d.is_dir())
        {
            dialog = dialog.set_directory(dir);
        }
        if let Some(p) = dialog.pick_file() {
            app.single_input = Some(p);
        }
    }
    if clear_input {
        app.single_input = None;
    }

    ui.add_space(10.0);

    // 出力先
    ui.label(egui::RichText::new(s::SINGLE_OUTPUT_DIR).strong());
    ui.add_enabled_ui(!running, |ui| {
        ui.horizontal(|ui| {
            let mut same = app.single_output_dir.is_none();
            if ui
                .radio_value(&mut same, true, s::SAME_AS_INPUT_DIR)
                .clicked()
            {
                app.single_output_dir = None;
            }
            let label = match &app.single_output_dir {
                Some(p) => display_path(p, 50),
                None => "別のフォルダ".to_string(),
            };
            if ui.radio_value(&mut same, false, label).clicked() && app.single_output_dir.is_none()
            {
                pick_output_dir(app);
            }
            if !same && ui.button(s::SELECT).clicked() {
                pick_output_dir(app);
            }
        });
    });

    // 出力ファイル名のプレビュー
    if let Some(input) = &app.single_input {
        let mut opts = app.base_options();
        opts.output_dir = app.single_output_dir.clone();
        let out = output_path_for(input, &opts);
        ui.label(
            egui::RichText::new(format!("→ {}", display_path(&out, 80)))
                .small()
                .weak(),
        )
        .on_hover_text(out.display().to_string());
    }
}

/// 中央パネル下部に固定表示する操作ボタンと進捗。
pub fn show_actions(ui: &mut egui::Ui, app: &mut App) {
    let running = app.is_running();

    // 操作ボタン
    ui.horizontal(|ui| {
        let can_convert = app.single_input.is_some() && !running;
        if ui
            .add_enabled(
                can_convert,
                egui::Button::new(egui::RichText::new(s::CONVERT).strong())
                    .min_size(egui::vec2(120.0, 32.0)),
            )
            .clicked()
        {
            app.start_single();
        }
        if running
            && app.job.as_ref().is_some_and(|j| j.kind == Tab::Single)
            && ui.button(s::CANCEL).clicked()
        {
            app.cancel_job();
        }

        #[cfg(feature = "preview")]
        {
            let has_output = app.last_output_file.as_ref().is_some_and(|p| p.is_file());
            if app.is_previewing() {
                if ui.button(s::PREVIEW_STOP).clicked() {
                    app.stop_preview();
                }
            } else if ui
                .add_enabled(has_output && !running, egui::Button::new(s::PREVIEW_PLAY))
                .clicked()
            {
                app.play_preview();
            }
        }

        let dir = app.last_output_dir.clone().filter(|d| d.is_dir());
        if let Some(dir) = dir {
            if ui.button(s::OPEN_FOLDER).clicked() {
                app.open_folder(&dir);
            }
        }
    });

    // 進捗
    if let Some(job) = &app.job {
        if job.kind == Tab::Single {
            ui.add_space(8.0);
            ui.add(
                egui::ProgressBar::new(if job.done > 0 { 1.0 } else { 0.0 })
                    .animate(true)
                    .text(format!(
                        "変換中… {}",
                        format_duration(job.started.elapsed())
                    )),
            );
        }
    }

    // 直近の結果
    if !running {
        if let Some(r) = app
            .last_results
            .last()
            .filter(|_| app.last_job_kind == Some(Tab::Single))
        {
            ui.add_space(8.0);
            if r.ok {
                ui.label(
                    egui::RichText::new(format!(
                        "完了: {} ({:.1} 秒の音声を {:.2} 秒で変換)",
                        r.dst.display(),
                        r.audio_seconds,
                        r.elapsed.as_secs_f64()
                    ))
                    .color(egui::Color32::from_rgb(0x3c, 0xb3, 0x71)),
                );
            } else {
                ui.label(
                    egui::RichText::new(format!("失敗: {}", r.error.clone().unwrap_or_default()))
                        .color(ui.visuals().error_fg_color),
                );
            }
        }
    }
}

fn pick_output_dir(app: &mut App) {
    let mut dialog = rfd::FileDialog::new().set_title(s::SINGLE_OUTPUT_DIR);
    let start: Option<PathBuf> = app.single_output_dir.clone().or_else(|| {
        app.single_input
            .as_ref()
            .and_then(|p| p.parent().map(Into::into))
    });
    if let Some(d) = start.filter(|d| d.is_dir()) {
        dialog = dialog.set_directory(d);
    }
    if let Some(d) = dialog.pick_folder() {
        app.single_output_dir = Some(d);
    }
}
