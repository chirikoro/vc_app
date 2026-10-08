//! 右ペイン: 声の設定と出力設定。

use crate::app::App;
use crate::strings as s;
use vc_core::params::{matching_preset, ConversionParams, PRESETS};
use vc_core::OutputFormat;

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    let running = app.is_running();

    ui.heading(s::PARAMS_HEADER);
    ui.add_space(4.0);

    // プリセット
    let current = matching_preset(&app.settings.params)
        .map(|p| p.name)
        .unwrap_or(s::PRESET_CUSTOM);
    ui.horizontal(|ui| {
        ui.label(s::PRESET);
        ui.add_enabled_ui(!running, |ui| {
            egui::ComboBox::from_id_salt("preset")
                .selected_text(current)
                .width(190.0)
                .show_ui(ui, |ui| {
                    for p in PRESETS {
                        let selected = current == p.name;
                        if ui
                            .selectable_label(selected, p.name)
                            .on_hover_text(p.description)
                            .clicked()
                        {
                            app.settings.params.pitch_semitones = p.pitch_semitones;
                            app.settings.params.formant_semitones = p.formant_semitones;
                        }
                    }
                    let _ = ui.selectable_label(current == s::PRESET_CUSTOM, s::PRESET_CUSTOM);
                });
        });
    });
    if let Some(p) = matching_preset(&app.settings.params) {
        ui.label(egui::RichText::new(p.description).small().weak());
    }

    ui.add_space(6.0);
    ui.add_enabled_ui(!running, |ui| {
        ui.label(s::PITCH);
        ui.add(
            egui::Slider::new(&mut app.settings.params.pitch_semitones, -12.0..=12.0)
                .step_by(0.5)
                .fixed_decimals(1)
                .suffix(s::SEMITONE_SUFFIX),
        );
        ui.label(s::FORMANT);
        ui.add(
            egui::Slider::new(&mut app.settings.params.formant_semitones, -12.0..=12.0)
                .step_by(0.5)
                .fixed_decimals(1)
                .suffix(s::SEMITONE_SUFFIX),
        );
        ui.add_space(4.0);
        ui.checkbox(&mut app.settings.params.cheaper, s::CHEAPER);
        if ui.small_button(s::RESET).clicked() {
            app.settings.params = ConversionParams::default();
        }
    });

    ui.add_space(12.0);
    ui.separator();
    ui.heading(s::OUTPUT_HEADER);
    ui.add_space(4.0);

    ui.add_enabled_ui(!running, |ui| {
        ui.horizontal(|ui| {
            ui.label(s::OUTPUT_FORMAT);
            ui.radio_value(
                &mut app.settings.output_format,
                OutputFormat::Wav,
                s::OUTPUT_WAV,
            );
            ui.radio_value(
                &mut app.settings.output_format,
                OutputFormat::SameAsInput,
                s::OUTPUT_SAME,
            )
            .on_hover_text(s::OUTPUT_SAME_HINT);
        });
        ui.label(
            egui::RichText::new(if app.ffmpeg_available {
                s::FFMPEG_FOUND
            } else {
                s::FFMPEG_MISSING
            })
            .small()
            .weak(),
        );

        ui.horizontal(|ui| {
            ui.label(s::SUFFIX);
            let resp =
                ui.add(egui::TextEdit::singleline(&mut app.settings.suffix).desired_width(120.0));
            if resp.lost_focus() {
                let cleaned = app.settings.clone().sanitized().suffix;
                if cleaned != app.settings.suffix {
                    app.settings.suffix = cleaned;
                }
                app.rescan_batch();
            }
        });
        ui.checkbox(&mut app.settings.overwrite, s::OVERWRITE);

        ui.horizontal(|ui| {
            ui.label(s::WORKERS);
            ui.add(egui::DragValue::new(&mut app.settings.workers).range(0..=64))
                .on_hover_text(s::WORKERS_AUTO_HINT);
            ui.label(
                egui::RichText::new(format!("(実効 {})", app.settings.effective_workers()))
                    .small()
                    .weak(),
            );
        });
    });

    ui.add_space(12.0);
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(s::THEME);
        let before = app.settings.theme.clone();
        egui::ComboBox::from_id_salt("theme")
            .selected_text(match app.settings.theme.as_str() {
                "dark" => s::THEME_DARK,
                "light" => s::THEME_LIGHT,
                _ => s::THEME_SYSTEM,
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut app.settings.theme,
                    "system".to_string(),
                    s::THEME_SYSTEM,
                );
                ui.selectable_value(&mut app.settings.theme, "dark".to_string(), s::THEME_DARK);
                ui.selectable_value(&mut app.settings.theme, "light".to_string(), s::THEME_LIGHT);
            });
        if before != app.settings.theme {
            app.apply_theme(ui.ctx());
        }
    });
    ui.add_space(8.0);
    ui.label(egui::RichText::new(s::SUPPORTED_INPUT).small().weak());
}
