//! 複数ファイルの並列一括変換。
//!
//! GUI からは別スレッドで [`BatchRunner::spawn`] を呼び、`mpsc` チャンネル経由で
//! 進捗イベントを受け取る。キャンセルは `AtomicBool` で伝える。

use crate::audio_io::{self, Audio, OutputFormat, SaveReport};
use crate::engine;
use crate::params::ConversionParams;
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 1 ファイルの処理結果。
#[derive(Debug, Clone, PartialEq)]
pub struct JobResult {
    pub src: PathBuf,
    pub dst: PathBuf,
    pub ok: bool,
    pub error: Option<String>,
    /// 出力形式が希望どおりにならず WAV に置き換えた場合の説明
    pub note: Option<String>,
    pub elapsed: Duration,
    /// 変換した音声の長さ [秒]
    pub audio_seconds: f64,
}

/// 進捗イベント。
#[derive(Debug, Clone, PartialEq)]
pub enum BatchEvent {
    Started {
        total: usize,
    },
    FileStarted {
        src: PathBuf,
    },
    FileDone(JobResult),
    Finished {
        ok: usize,
        failed: usize,
        skipped: usize,
        cancelled: bool,
        elapsed: Duration,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct BatchOptions {
    pub params: ConversionParams,
    pub output_format: OutputFormat,
    /// None なら各入力ファイルと同じフォルダ
    pub output_dir: Option<PathBuf>,
    /// 入力フォルダ(再帰検索時のフォルダ構造を出力側に再現するための基準)
    pub input_root: Option<PathBuf>,
    pub suffix: String,
    pub overwrite: bool,
    pub workers: usize,
}

impl Default for BatchOptions {
    fn default() -> Self {
        Self {
            params: ConversionParams::default(),
            output_format: OutputFormat::Wav,
            output_dir: None,
            input_root: None,
            suffix: "_converted".to_string(),
            overwrite: false,
            workers: 1,
        }
    }
}

/// フォルダ内の対応音声ファイルを列挙する(ソート済み)。
/// 既に `suffix` が付いた出力ファイルは除外する(再変換の無限連鎖を防ぐ)。
pub fn discover_files(dir: &Path, recursive: bool, suffix: &str) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    walk(dir, recursive, suffix, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk(dir: &Path, recursive: bool, suffix: &str, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            if recursive {
                // 出力フォルダの既定名はスキップ(自分の出力を再入力しない)
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name == "converted" {
                    continue;
                }
                walk(&path, recursive, suffix, out)?;
            }
            continue;
        }
        if !ft.is_file() {
            continue;
        }
        if !audio_io::is_supported_input(&path) {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if !suffix.is_empty() && stem.ends_with(suffix) {
            continue;
        }
        out.push(path);
    }
    Ok(())
}

/// 出力パスを決める。
pub fn output_path_for(src: &Path, opts: &BatchOptions) -> PathBuf {
    let stem = src
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output")
        .to_string();
    let ext = audio_io::output_extension(src, opts.output_format);
    let file_name = format!("{stem}{}.{ext}", opts.suffix);

    let dir = match &opts.output_dir {
        None => src.parent().map(Path::to_path_buf).unwrap_or_default(),
        Some(out_root) => {
            // 再帰検索時はサブフォルダ構造を維持する
            match opts
                .input_root
                .as_ref()
                .and_then(|root| src.parent().and_then(|p| p.strip_prefix(root).ok()))
            {
                Some(rel) if !rel.as_os_str().is_empty() => out_root.join(rel),
                _ => out_root.clone(),
            }
        }
    };
    dir.join(file_name)
}

/// 1 ファイルを変換して保存する。エラーは戻り値に畳み込み、panic しない。
pub fn convert_file(src: &Path, dst: &Path, opts: &BatchOptions) -> JobResult {
    let start = Instant::now();
    let mut result = JobResult {
        src: src.to_path_buf(),
        dst: dst.to_path_buf(),
        ok: false,
        error: None,
        note: None,
        elapsed: Duration::ZERO,
        audio_seconds: 0.0,
    };

    let outcome = (|| -> Result<(Audio, SaveReport), String> {
        if !opts.overwrite && dst.exists() {
            return Err(format!(
                "出力先が既に存在します(上書きしない設定): {}",
                dst.display()
            ));
        }
        if same_file(src, dst) {
            return Err("入力と出力が同じファイルです".to_string());
        }
        let audio = audio_io::load_audio(src).map_err(|e| e.to_string())?;
        let converted = engine::convert(&audio, &opts.params).map_err(|e| e.to_string())?;
        if let Some(dir) = dst.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("出力フォルダを作成できません: {}: {e}", dir.display()))?;
        }
        let report =
            audio_io::save_audio(dst, &converted, opts.output_format).map_err(|e| e.to_string())?;
        Ok((converted, report))
    })();

    match outcome {
        Ok((audio, report)) => {
            result.ok = true;
            result.audio_seconds = audio.duration_seconds();
            result.dst = report.path.clone();
            result.note = report.note;
        }
        Err(e) => result.error = Some(e),
    }
    result.elapsed = start.elapsed();
    result
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

/// 一括変換の実行器。
pub struct BatchRunner;

impl BatchRunner {
    /// 別スレッドで一括変換を開始する。イベントは `tx` に送られる。
    /// `cancel` を true にすると、未着手のファイルをスキップして終了する。
    pub fn spawn(
        files: Vec<PathBuf>,
        opts: BatchOptions,
        tx: Sender<BatchEvent>,
        cancel: Arc<AtomicBool>,
    ) -> std::thread::JoinHandle<()> {
        std::thread::Builder::new()
            .name("vc-batch".into())
            .spawn(move || Self::run(files, opts, tx, cancel))
            .expect("spawn batch thread")
    }

    /// 現在のスレッドで一括変換を実行する(内部で rayon のスレッドプールを使う)。
    pub fn run(
        files: Vec<PathBuf>,
        opts: BatchOptions,
        tx: Sender<BatchEvent>,
        cancel: Arc<AtomicBool>,
    ) {
        let start = Instant::now();
        let total = files.len();
        let _ = tx.send(BatchEvent::Started { total });

        let workers = opts.workers.max(1).min(total.max(1));
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .thread_name(|i| format!("vc-worker-{i}"))
            .build();
        let pool = match pool {
            Ok(p) => p,
            Err(e) => {
                let _ = tx.send(BatchEvent::Finished {
                    ok: 0,
                    failed: 0,
                    skipped: total,
                    cancelled: false,
                    elapsed: start.elapsed(),
                });
                eprintln!("スレッドプールの作成に失敗しました: {e}");
                return;
            }
        };

        let counts = pool.install(|| {
            files
                .par_iter()
                .map(|src| {
                    if cancel.load(Ordering::Relaxed) {
                        return (0usize, 0usize, 1usize);
                    }
                    let _ = tx.send(BatchEvent::FileStarted { src: src.clone() });
                    let dst = output_path_for(src, &opts);
                    let result = convert_file(src, &dst, &opts);
                    let ok = result.ok;
                    let _ = tx.send(BatchEvent::FileDone(result));
                    if ok {
                        (1, 0, 0)
                    } else {
                        (0, 1, 0)
                    }
                })
                .reduce(|| (0, 0, 0), |a, b| (a.0 + b.0, a.1 + b.1, a.2 + b.2))
        });

        let _ = tx.send(BatchEvent::Finished {
            ok: counts.0,
            failed: counts.1,
            skipped: counts.2,
            cancelled: cancel.load(Ordering::Relaxed),
            elapsed: start.elapsed(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_path_default_next_to_source() {
        let opts = BatchOptions::default();
        let p = output_path_for(Path::new("/music/a.mp3"), &opts);
        assert_eq!(p, PathBuf::from("/music/a_converted.wav"));
    }

    #[test]
    fn output_path_keeps_subdirs_when_recursive() {
        let opts = BatchOptions {
            output_dir: Some(PathBuf::from("/out")),
            input_root: Some(PathBuf::from("/in")),
            ..Default::default()
        };
        let p = output_path_for(Path::new("/in/sub/deep/a.wav"), &opts);
        assert_eq!(p, PathBuf::from("/out/sub/deep/a_converted.wav"));
        let p = output_path_for(Path::new("/in/a.wav"), &opts);
        assert_eq!(p, PathBuf::from("/out/a_converted.wav"));
    }

    #[test]
    fn same_format_keeps_extension_when_encodable() {
        let opts = BatchOptions {
            output_format: OutputFormat::SameAsInput,
            ..Default::default()
        };
        assert_eq!(
            output_path_for(Path::new("x/a.flac"), &opts),
            PathBuf::from("x/a_converted.flac")
        );
    }
}
