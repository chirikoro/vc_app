//! 「フォルダ一括」タブ。

use super::{drop_zone, format_duration, path_row};
use crate::app::{App, Tab};
use crate::strings as s;

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    let running = app.is_running();

    ui.heading(s::TAB_BATCH);
    ui.add_space(8.0);

    // 入力フォルダ
    ui.label(egui::RichText::new(s::BATCH_INPUT_DIR).strong());
    let mut pick_input = false;
    let mut rescan = false;
    drop_zone(ui, s::BATCH_DROP_HINT, |ui| {
        path_row(ui, app.batch_input_dir.as_deref(), "(未選択)", |ui| {
            ui.add_enabled_ui(!running, |ui| {
                if ui.button(s::SELECT).clicked() {
                    pick_input = true;
                }
                if app.batch_input_dir.is_some() && ui.button(s::RESCAN).clicked() {
                    rescan = true;
                }
            });
        });
    });
    if pick_input {
        let mut dialog = rfd::FileDialog::new().set_title(s::BATCH_INPUT_DIR);
        if let Some(d) = app.batch_input_dir.as_ref().filter(|d| d.is_dir()) {
            dialog = dialog.set_directory(d);
        }
        if let Some(d) = dialog.pick_folder() {
            app.batch_input_dir = Some(d);
            rescan = true;
        }
    }
    if rescan {
        app.rescan_batch();
    }

    ui.add_enabled_ui(!running, |ui| {
        if ui
            .checkbox(&mut app.settings.recursive, s::RECURSIVE)
            .changed()
        {
            app.rescan_batch();
        }
    });

    ui.add_space(10.0);

    // 出力フォルダ
    ui.label(egui::RichText::new(s::BATCH_OUTPUT_DIR).strong());
    let mut pick_output = false;
    let mut clear_output = false;
    ui.add_enabled_ui(!running, |ui| {
        path_row(
            ui,
            app.batch_output_dir.as_deref(),
            s::BATCH_DEFAULT_OUTPUT,
            |ui| {
                if ui.button(s::SELECT).clicked() {
                    pick_output = true;
                }
                if app.batch_output_dir.is_some() && ui.button(s::CLEAR).clicked() {
                    clear_output = true;
                }
            },
        );
    });
    if pick_output {
        let mut dialog = rfd::FileDialog::new().set_title(s::BATCH_OUTPUT_DIR);
        if let Some(d) = app
            .batch_output_dir
            .clone()
            .or_else(|| app.batch_input_dir.clone())
            .filter(|d| d.is_dir())
        {
            dialog = dialog.set_directory(d);
        }
        if let Some(d) = dialog.pick_folder() {
            app.batch_output_dir = Some(d);
        }
    }
    if clear_output {
        app.batch_output_dir = None;
    }

    ui.add_space(10.0);

    // 対象ファイル一覧
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(s::FILES_FOUND).strong());
        ui.label(format!("{} 件", app.batch_files.len()));
    });
    if let Some(err) = &app.batch_scan_error {
        ui.label(egui::RichText::new(err).color(ui.visuals().error_fg_color));
    }
    // 残りの高さを一覧に使う(操作ボタンはパネル下部に固定されている)
    let list_height = (ui.available_height() - 24.0).clamp(110.0, 400.0);
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::vertical()
                .id_salt("batch_files")
                .max_height(list_height)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    if app.batch_files.is_empty() {
                        ui.label(egui::RichText::new(s::NO_FILES).weak());
                    }
                    let root = app.batch_input_dir.clone();
                    let in_progress: Vec<_> = app
                        .job
                        .as_ref()
                        .map(|j| j.in_progress.clone())
                        .unwrap_or_default();
                    for f in &app.batch_files {
                        let rel = root
                            .as_ref()
                            .and_then(|r| f.strip_prefix(r).ok())
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| f.display().to_string());
                        let status = if app.last_job_kind == Some(Tab::Batch) {
                            app.last_results.iter().find(|r| &r.src == f)
                        } else {
                            None
                        };
                        let (mark, color) = match status {
                            Some(r) if r.ok => ("✔", egui::Color32::from_rgb(0x3c, 0xb3, 0x71)),
                            Some(_) => ("✖", ui.visuals().error_fg_color),
                            None if in_progress.contains(f) => ("…", ui.visuals().warn_fg_color),
                            None => ("", ui.visuals().text_color()),
                        };
                        ui.horizontal(|ui| {
                            ui.add_sized(
                                [18.0, 18.0],
                                egui::Label::new(egui::RichText::new(mark).color(color)),
                            );
                            let resp = ui.label(egui::RichText::new(rel).color(color));
                            if let Some(r) = status {
                                if let Some(e) = &r.error {
                                    resp.on_hover_text(e);
                                } else if let Some(n) = &r.note {
                                    resp.on_hover_text(n);
                                }
                            }
                        });
                    }
                });
        });
}

/// 中央パネル下部に固定表示する操作ボタンと進捗。
pub fn show_actions(ui: &mut egui::Ui, app: &mut App) {
    let running = app.is_running();

    // 操作
    ui.horizontal(|ui| {
        let can_start = app.batch_input_dir.is_some() && !app.batch_files.is_empty() && !running;
        if ui
            .add_enabled(
                can_start,
                egui::Button::new(egui::RichText::new(s::START).strong())
                    .min_size(egui::vec2(160.0, 32.0)),
            )
            .clicked()
        {
            app.start_batch();
        }
        if running && app.job.as_ref().is_some_and(|j| j.kind == Tab::Batch) {
            let requested = app.job.as_ref().is_some_and(|j| j.cancel_requested());
            if ui
                .add_enabled(!requested, egui::Button::new(s::CANCEL))
                .clicked()
            {
                app.cancel_job();
            }
        }
        let dir = app
            .last_output_dir
            .clone()
            .or_else(|| app.effective_batch_output_dir())
            .filter(|d| d.is_dir());
        if let Some(dir) = dir {
            if ui.button(s::OPEN_FOLDER).clicked() {
                app.open_folder(&dir);
            }
        }
    });

    // 進捗
    if let Some(job) = &app.job {
        if job.kind == Tab::Batch {
            ui.add_space(8.0);
            let mut text = format!(
                "{} / {}  (成功 {} / 失敗 {})  経過 {}",
                job.done,
                job.total,
                job.ok,
                job.failed,
                format_duration(job.started.elapsed())
            );
            if let Some(eta) = job.eta() {
                text.push_str(&format!("  残り約 {}", format_duration(eta)));
            }
            ui.add(
                egui::ProgressBar::new(job.progress())
                    .animate(true)
                    .text(text),
            );
        }
    } else if app.last_job_kind == Some(Tab::Batch) && !app.last_results.is_empty() {
        let ok = app.last_results.iter().filter(|r| r.ok).count();
        let failed = app.last_results.len() - ok;
        let total_audio: f64 = app.last_results.iter().map(|r| r.audio_seconds).sum();
        ui.add_space(8.0);
        ui.label(format!(
            "前回の結果: 成功 {ok} / 失敗 {failed}(音声合計 {})",
            format_duration(std::time::Duration::from_secs_f64(total_audio))
        ));
    }
}
