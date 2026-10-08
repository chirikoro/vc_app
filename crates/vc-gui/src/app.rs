//! アプリ状態と更新ループ。

use crate::strings as s;
use crate::ui;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use vc_core::audio_io;
use vc_core::batch::{discover_files, BatchEvent, BatchOptions, BatchRunner, JobResult};
use vc_core::settings::Settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Single,
    Batch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Ok,
    Warn,
    Error,
}

pub struct LogLine {
    pub level: LogLevel,
    pub text: String,
}

/// 実行中のジョブ。
pub struct RunningJob {
    rx: Receiver<BatchEvent>,
    cancel: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    pub kind: Tab,
    pub total: usize,
    pub done: usize,
    pub ok: usize,
    pub failed: usize,
    pub started: Instant,
    pub in_progress: Vec<PathBuf>,
    pub output_dir: Option<PathBuf>,
}

impl RunningJob {
    pub fn progress(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            self.done as f32 / self.total as f32
        }
    }

    pub fn eta(&self) -> Option<Duration> {
        if self.done == 0 || self.done >= self.total {
            return None;
        }
        let per = self.started.elapsed().as_secs_f64() / self.done as f64;
        Some(Duration::from_secs_f64(
            per * (self.total - self.done) as f64,
        ))
    }

    pub fn cancel_requested(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

pub struct App {
    pub settings: Settings,
    settings_path: Option<PathBuf>,
    pub tab: Tab,

    pub single_input: Option<PathBuf>,
    /// None = 入力と同じフォルダ
    pub single_output_dir: Option<PathBuf>,

    pub batch_input_dir: Option<PathBuf>,
    /// None = <入力>/converted
    pub batch_output_dir: Option<PathBuf>,
    pub batch_files: Vec<PathBuf>,
    pub batch_scan_error: Option<String>,

    pub job: Option<RunningJob>,
    pub log: Vec<LogLine>,
    pub last_results: Vec<JobResult>,
    /// 直近に実行したジョブの種類(結果表示をタブごとに分けるため)
    pub last_job_kind: Option<Tab>,
    pub last_output_file: Option<PathBuf>,
    pub last_output_dir: Option<PathBuf>,

    #[cfg(feature = "preview")]
    preview: Option<crate::preview::Preview>,
    pub font_path: Option<PathBuf>,
    pub ffmpeg_available: bool,
    last_save: Instant,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, font_path: Option<PathBuf>) -> Self {
        let settings_path = Settings::default_path();
        let settings = Settings::load();

        let mut app = Self {
            tab: Tab::Single,
            single_input: settings.last_input_file.clone().filter(|p| p.is_file()),
            single_output_dir: None,
            batch_input_dir: settings.last_input_dir.clone().filter(|p| p.is_dir()),
            batch_output_dir: settings.last_output_dir.clone(),
            batch_files: Vec::new(),
            batch_scan_error: None,
            job: None,
            log: Vec::new(),
            last_results: Vec::new(),
            last_job_kind: None,
            last_output_file: None,
            last_output_dir: None,
            #[cfg(feature = "preview")]
            preview: None,
            font_path,
            ffmpeg_available: audio_io::ffmpeg_path().is_some(),
            last_save: Instant::now(),
            settings,
            settings_path,
        };
        app.apply_theme(&cc.egui_ctx);
        Self::apply_style(&cc.egui_ctx);
        if app.batch_input_dir.is_some() {
            app.rescan_batch();
        }
        app.log(
            LogLevel::Info,
            format!(
                "起動しました。{}",
                if app.ffmpeg_available {
                    s::FFMPEG_FOUND
                } else {
                    s::FFMPEG_MISSING
                }
            ),
        );
        match &app.font_path {
            Some(p) => {
                let msg = format!("日本語フォント: {}", p.display());
                app.log(LogLevel::Info, msg);
            }
            None => app.log(
                LogLevel::Warn,
                "日本語フォントが見つかりません。環境変数 VC_APP_FONT にフォントファイルのパスを設定してください。",
            ),
        }
        app
    }

    fn apply_style(ctx: &egui::Context) {
        ctx.all_styles_mut(|st| {
            st.spacing.item_spacing = egui::vec2(8.0, 8.0);
            st.spacing.button_padding = egui::vec2(12.0, 6.0);
            st.spacing.slider_width = 180.0;
            st.spacing.interact_size.y = 26.0;
            st.text_styles
                .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
            st.text_styles
                .insert(egui::TextStyle::Button, egui::FontId::proportional(15.0));
            st.text_styles
                .insert(egui::TextStyle::Heading, egui::FontId::proportional(20.0));
            st.text_styles
                .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
            let r = egui::CornerRadius::same(6);
            st.visuals.widgets.noninteractive.corner_radius = r;
            st.visuals.widgets.inactive.corner_radius = r;
            st.visuals.widgets.hovered.corner_radius = r;
            st.visuals.widgets.active.corner_radius = r;
            st.visuals.widgets.open.corner_radius = r;
            st.visuals.window_corner_radius = egui::CornerRadius::same(10);
        });
    }

    pub fn apply_theme(&self, ctx: &egui::Context) {
        let pref = match self.settings.theme.as_str() {
            "dark" => egui::ThemePreference::Dark,
            "light" => egui::ThemePreference::Light,
            _ => egui::ThemePreference::System,
        };
        ctx.set_theme(pref);
    }

    pub fn log(&mut self, level: LogLevel, text: impl Into<String>) {
        self.log.push(LogLine {
            level,
            text: text.into(),
        });
        if self.log.len() > 2000 {
            self.log.drain(0..1000);
        }
    }

    pub fn is_running(&self) -> bool {
        self.job.is_some()
    }

    pub fn save_settings(&mut self) {
        self.settings.last_input_file = self.single_input.clone();
        self.settings.last_input_dir = self.batch_input_dir.clone();
        self.settings.last_output_dir = self.batch_output_dir.clone();
        if let Some(path) = &self.settings_path {
            if let Err(e) = self.settings.save_to(path) {
                self.log(LogLevel::Warn, format!("設定を保存できません: {e}"));
            }
        }
        self.last_save = Instant::now();
    }

    // ------------------------------------------------------------------
    // 一括変換の対象検索
    // ------------------------------------------------------------------

    pub fn rescan_batch(&mut self) {
        self.batch_files.clear();
        self.batch_scan_error = None;
        let Some(dir) = self.batch_input_dir.clone() else {
            return;
        };
        match discover_files(&dir, self.settings.recursive, &self.settings.suffix) {
            Ok(files) => self.batch_files = files,
            Err(e) => self.batch_scan_error = Some(format!("{}: {e}", dir.display())),
        }
    }

    pub fn effective_batch_output_dir(&self) -> Option<PathBuf> {
        match (&self.batch_output_dir, &self.batch_input_dir) {
            (Some(out), _) => Some(out.clone()),
            (None, Some(input)) => Some(input.join("converted")),
            (None, None) => None,
        }
    }

    pub fn base_options(&self) -> BatchOptions {
        BatchOptions {
            params: self.settings.params,
            output_format: self.settings.output_format,
            output_dir: None,
            input_root: None,
            suffix: self.settings.suffix.clone(),
            overwrite: self.settings.overwrite,
            workers: 1,
        }
    }

    // ------------------------------------------------------------------
    // ジョブ開始/停止
    // ------------------------------------------------------------------

    pub fn start_single(&mut self) {
        if self.is_running() {
            return;
        }
        let Some(input) = self.single_input.clone() else {
            return;
        };
        if let Err(e) = self.settings.params.validate() {
            self.log(LogLevel::Error, e);
            return;
        }
        let mut opts = self.base_options();
        opts.output_dir = self.single_output_dir.clone();
        let output_dir = opts
            .output_dir
            .clone()
            .or_else(|| input.parent().map(Path::to_path_buf));
        self.log(LogLevel::Info, format!("変換開始: {}", input.display()));
        self.spawn_job(vec![input], opts, Tab::Single, output_dir);
    }

    pub fn start_batch(&mut self) {
        if self.is_running() {
            return;
        }
        let Some(input_dir) = self.batch_input_dir.clone() else {
            return;
        };
        self.rescan_batch();
        if self.batch_files.is_empty() {
            self.log(LogLevel::Warn, s::NO_FILES);
            return;
        }
        if let Err(e) = self.settings.params.validate() {
            self.log(LogLevel::Error, e);
            return;
        }
        let output_dir = self.effective_batch_output_dir();
        let mut opts = self.base_options();
        opts.output_dir = output_dir.clone();
        opts.input_root = Some(input_dir.clone());
        opts.workers = self.settings.effective_workers();
        self.log(
            LogLevel::Info,
            format!(
                "一括変換開始: {} 件、{} 並列 → {}",
                self.batch_files.len(),
                opts.workers,
                output_dir
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default()
            ),
        );
        let files = self.batch_files.clone();
        self.spawn_job(files, opts, Tab::Batch, output_dir);
    }

    fn spawn_job(
        &mut self,
        files: Vec<PathBuf>,
        opts: BatchOptions,
        kind: Tab,
        output_dir: Option<PathBuf>,
    ) {
        #[cfg(feature = "preview")]
        self.stop_preview();
        let (tx, rx) = std::sync::mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let handle = BatchRunner::spawn(files, opts, tx, cancel.clone());
        self.last_results.clear();
        self.last_job_kind = Some(kind);
        self.job = Some(RunningJob {
            rx,
            cancel,
            handle: Some(handle),
            kind,
            total: 0,
            done: 0,
            ok: 0,
            failed: 0,
            started: Instant::now(),
            in_progress: Vec::new(),
            output_dir,
        });
        self.save_settings();
    }

    pub fn cancel_job(&mut self) {
        if let Some(job) = &self.job {
            job.cancel.store(true, Ordering::Relaxed);
            self.log(
                LogLevel::Warn,
                "キャンセルを要求しました(処理中のファイルが終わり次第停止します)",
            );
        }
    }

    /// ワーカーからのイベントを処理する。
    pub fn poll_job(&mut self, ctx: &egui::Context) {
        let Some(job) = &mut self.job else {
            return;
        };
        let mut finished = false;
        let mut lines: Vec<(LogLevel, String)> = Vec::new();
        loop {
            match job.rx.try_recv() {
                Ok(BatchEvent::Started { total }) => job.total = total,
                Ok(BatchEvent::FileStarted { src }) => job.in_progress.push(src),
                Ok(BatchEvent::FileDone(r)) => {
                    job.done += 1;
                    job.in_progress.retain(|p| p != &r.src);
                    if r.ok {
                        job.ok += 1;
                        lines.push((
                            LogLevel::Ok,
                            format!(
                                "[{}/{}] 完了 {} ({:.1} 秒の音声 / {:.2} 秒)",
                                job.done,
                                job.total,
                                r.dst.display(),
                                r.audio_seconds,
                                r.elapsed.as_secs_f64()
                            ),
                        ));
                        if let Some(note) = &r.note {
                            lines.push((LogLevel::Warn, format!("    注意: {note}")));
                        }
                        self.last_output_file = Some(r.dst.clone());
                    } else {
                        job.failed += 1;
                        lines.push((
                            LogLevel::Error,
                            format!(
                                "[{}/{}] 失敗 {}: {}",
                                job.done,
                                job.total,
                                r.src.display(),
                                r.error.clone().unwrap_or_default()
                            ),
                        ));
                    }
                    self.last_results.push(r);
                }
                Ok(BatchEvent::Finished {
                    ok,
                    failed,
                    skipped,
                    cancelled,
                    elapsed,
                }) => {
                    let level = if failed > 0 {
                        LogLevel::Warn
                    } else {
                        LogLevel::Ok
                    };
                    lines.push((
                        level,
                        format!(
                            "終了: 成功 {ok} / 失敗 {failed} / スキップ {skipped}{} ({:.1} 秒)",
                            if cancelled { " / キャンセル" } else { "" },
                            elapsed.as_secs_f64()
                        ),
                    ));
                    finished = true;
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    finished = true;
                    break;
                }
            }
        }
        for (level, text) in lines {
            self.log(level, text);
        }
        if finished {
            if let Some(mut job) = self.job.take() {
                if let Some(h) = job.handle.take() {
                    let _ = h.join();
                }
                self.last_output_dir = job.output_dir.clone();
            }
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    // ------------------------------------------------------------------
    // ドラッグ&ドロップ
    // ------------------------------------------------------------------

    pub fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        if dropped.is_empty() || self.is_running() {
            return;
        }
        let dirs: Vec<&PathBuf> = dropped.iter().filter(|p| p.is_dir()).collect();
        let files: Vec<&PathBuf> = dropped
            .iter()
            .filter(|p| p.is_file() && audio_io::is_supported_input(p))
            .collect();

        if let Some(dir) = dirs.first() {
            self.tab = Tab::Batch;
            self.batch_input_dir = Some((*dir).clone());
            self.rescan_batch();
            self.log(LogLevel::Info, format!("フォルダを設定: {}", dir.display()));
        } else if let Some(file) = files.first() {
            match self.tab {
                Tab::Single => {
                    self.single_input = Some((*file).clone());
                    self.log(
                        LogLevel::Info,
                        format!("ファイルを設定: {}", file.display()),
                    );
                }
                Tab::Batch => {
                    if let Some(parent) = file.parent() {
                        self.batch_input_dir = Some(parent.to_path_buf());
                        self.rescan_batch();
                        self.log(
                            LogLevel::Info,
                            format!("ファイルの親フォルダを設定: {}", parent.display()),
                        );
                    }
                }
            }
        } else {
            self.log(
                LogLevel::Warn,
                "対応していないファイルです。".to_string() + s::SUPPORTED_INPUT,
            );
        }
    }

    // ------------------------------------------------------------------
    // 試聴
    // ------------------------------------------------------------------

    #[cfg(feature = "preview")]
    pub fn play_preview(&mut self) {
        let Some(path) = self.last_output_file.clone() else {
            return;
        };
        if self.preview.is_none() {
            match crate::preview::Preview::new() {
                Ok(p) => self.preview = Some(p),
                Err(e) => {
                    self.log(LogLevel::Error, e);
                    return;
                }
            }
        }
        if let Some(p) = &mut self.preview {
            match p.play(&path) {
                Ok(()) => {}
                Err(e) => {
                    self.log(LogLevel::Error, e);
                }
            }
        }
    }

    #[cfg(feature = "preview")]
    pub fn stop_preview(&mut self) {
        if let Some(p) = &mut self.preview {
            p.stop();
        }
    }

    #[cfg(feature = "preview")]
    pub fn is_previewing(&self) -> bool {
        self.preview.as_ref().is_some_and(|p| p.is_playing())
    }

    pub fn open_folder(&mut self, dir: &Path) {
        if let Err(e) = open::that(dir) {
            self.log(
                LogLevel::Error,
                format!("フォルダを開けません: {}: {e}", dir.display()),
            );
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_job(ctx);
        self.handle_drops(ctx);
        // 設定は数秒おきに自動保存(頻繁な書き込みを避ける)
        if self.last_save.elapsed() > Duration::from_secs(10) {
            self.save_settings();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let style = ui.style().clone();

        egui::Panel::top("header")
            .frame(
                egui::Frame::side_top_panel(&style).inner_margin(egui::Margin::symmetric(16, 10)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(s::APP_TITLE);
                    ui.add_space(16.0);
                    ui.selectable_value(&mut self.tab, Tab::Single, s::TAB_SINGLE);
                    ui.selectable_value(&mut self.tab, Tab::Batch, s::TAB_BATCH);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (text, color) = if self.is_running() {
                            (s::STATUS_RUNNING, ui.visuals().warn_fg_color)
                        } else {
                            (s::STATUS_IDLE, ui.visuals().weak_text_color())
                        };
                        ui.label(egui::RichText::new(text).color(color));
                        if self.is_running() {
                            ui.spinner();
                        }
                    });
                });
            });

        egui::Panel::right("params")
            .resizable(false)
            .exact_size(330.0)
            .frame(egui::Frame::side_top_panel(&style).inner_margin(egui::Margin::same(16)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| ui::params::show(ui, self));
            });

        egui::Panel::bottom("log")
            .resizable(true)
            .default_size(140.0)
            .min_size(60.0)
            .frame(egui::Frame::side_top_panel(&style).inner_margin(egui::Margin::symmetric(16, 8)))
            .show(ui, |ui| ui::log::show(ui, self));

        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&style).inner_margin(egui::Margin::same(16)))
            .show(ui, |ui| {
                // 操作ボタンと進捗は下部に固定し、上側の内容だけスクロールさせる
                egui::Panel::bottom("actions")
                    .resizable(false)
                    .show_separator_line(false)
                    .frame(egui::Frame::new().inner_margin(egui::Margin {
                        left: 0,
                        right: 0,
                        top: 10,
                        bottom: 0,
                    }))
                    .show(ui, |ui| match self.tab {
                        Tab::Single => ui::single::show_actions(ui, self),
                        Tab::Batch => ui::batch::show_actions(ui, self),
                    });
                egui::ScrollArea::vertical()
                    .id_salt("central")
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.tab {
                        Tab::Single => ui::single::show(ui, self),
                        Tab::Batch => ui::batch::show(ui, self),
                    });
            });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        #[cfg(feature = "preview")]
        self.stop_preview();
        if let Some(job) = &self.job {
            job.cancel.store(true, Ordering::Relaxed);
        }
        self.save_settings();
    }
}
