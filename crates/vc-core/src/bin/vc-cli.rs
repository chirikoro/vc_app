//! コマンドライン版。GUI 無しで変換・一括変換・速度計測ができる。
//!
//! 例:
//!   vc-cli convert input.wav --preset 男性→女性
//!   vc-cli batch ./voices --out ./voices/converted --pitch -4 --formant -2 --recursive
//!   vc-cli bench

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Instant;
use vc_core::batch::{discover_files, output_path_for, BatchEvent, BatchOptions, BatchRunner};
use vc_core::params::{find_preset, PRESETS};
use vc_core::settings::effective_workers;
use vc_core::{Audio, ConversionParams, OutputFormat};

#[derive(Parser)]
#[command(
    name = "vc-cli",
    about = "音声ファイルを別人の声に変換する(CLI 版)",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Args, Clone)]
struct ParamArgs {
    /// プリセット名(`presets` で一覧表示)
    #[arg(long)]
    preset: Option<String>,
    /// ピッチの変化量 [半音]
    #[arg(long)]
    pitch: Option<f32>,
    /// フォルマントの変化量 [半音]
    #[arg(long)]
    formant: Option<f32>,
    /// 軽量モード(速度優先)
    #[arg(long)]
    cheaper: bool,
}

impl ParamArgs {
    fn to_params(&self) -> Result<ConversionParams> {
        let mut p = match &self.preset {
            Some(name) => find_preset(name)
                .ok_or_else(|| {
                    anyhow!("プリセット '{name}' がありません(`vc-cli presets` で一覧表示)")
                })?
                .params(),
            None => ConversionParams::default(),
        };
        if let Some(v) = self.pitch {
            p.pitch_semitones = v;
        }
        if let Some(v) = self.formant {
            p.formant_semitones = v;
        }
        p.cheaper = self.cheaper;
        p.validate().map_err(|e| anyhow!(e))?;
        Ok(p)
    }
}

#[derive(Subcommand)]
enum Command {
    /// 1 ファイルを変換する
    Convert {
        input: PathBuf,
        /// 出力ファイル(省略時は同じフォルダに `_converted` 付きで保存)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// 入力と同じ形式で出力する(既定は WAV)
        #[arg(long)]
        same_format: bool,
        /// 既存ファイルを上書きする
        #[arg(long)]
        overwrite: bool,
        #[command(flatten)]
        params: ParamArgs,
    },
    /// フォルダ内の音声ファイルを一括変換する
    Batch {
        input_dir: PathBuf,
        /// 出力フォルダ(省略時は `<入力フォルダ>/converted`)
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// サブフォルダも対象にする
        #[arg(short, long)]
        recursive: bool,
        /// 入力と同じ形式で出力する(既定は WAV)
        #[arg(long)]
        same_format: bool,
        /// 既存ファイルを上書きする
        #[arg(long)]
        overwrite: bool,
        /// 並列数(0 = 自動: コア数 − 1)
        #[arg(short = 'j', long, default_value_t = 0)]
        workers: usize,
        #[command(flatten)]
        params: ParamArgs,
    },
    /// プリセット一覧を表示する
    Presets,
    /// 合成音声で変換速度を計測する
    Bench {
        /// 音声の長さ [秒]
        #[arg(long, default_value_t = 60.0)]
        seconds: f32,
        /// サンプルレート [Hz]
        #[arg(long, default_value_t = 44100)]
        sample_rate: u32,
        /// 並列数(0 = 自動)
        #[arg(short = 'j', long, default_value_t = 0)]
        workers: usize,
        /// 一括計測で処理するファイル数
        #[arg(long, default_value_t = 8)]
        files: usize,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Presets => {
            println!(
                "{:<14} {:>6} {:>10}  説明",
                "名前", "ピッチ", "フォルマント"
            );
            for p in PRESETS {
                println!(
                    "{:<14} {:>+6.1} {:>+10.1}  {}",
                    p.name, p.pitch_semitones, p.formant_semitones, p.description
                );
            }
            Ok(())
        }
        Command::Convert {
            input,
            output,
            same_format,
            overwrite,
            params,
        } => {
            let opts = BatchOptions {
                params: params.to_params()?,
                output_format: if same_format {
                    OutputFormat::SameAsInput
                } else {
                    OutputFormat::Wav
                },
                output_dir: None,
                input_root: None,
                suffix: "_converted".into(),
                overwrite,
                workers: 1,
            };
            let dst = output.unwrap_or_else(|| output_path_for(&input, &opts));
            let r = vc_core::batch::convert_file(&input, &dst, &opts);
            if let Some(note) = &r.note {
                println!("注意: {note}");
            }
            if r.ok {
                println!(
                    "完了: {} ({:.1} 秒の音声を {:.2} 秒で変換)",
                    r.dst.display(),
                    r.audio_seconds,
                    r.elapsed.as_secs_f64()
                );
                Ok(())
            } else {
                Err(anyhow!("{}", r.error.unwrap_or_default()))
            }
        }
        Command::Batch {
            input_dir,
            out,
            recursive,
            same_format,
            overwrite,
            workers,
            params,
        } => {
            let suffix = "_converted".to_string();
            let files = discover_files(&input_dir, recursive, &suffix)
                .with_context(|| format!("フォルダを読めません: {}", input_dir.display()))?;
            if files.is_empty() {
                println!(
                    "対応する音声ファイルが見つかりませんでした: {}",
                    input_dir.display()
                );
                return Ok(());
            }
            let out_dir = out.unwrap_or_else(|| input_dir.join("converted"));
            let opts = BatchOptions {
                params: params.to_params()?,
                output_format: if same_format {
                    OutputFormat::SameAsInput
                } else {
                    OutputFormat::Wav
                },
                output_dir: Some(out_dir.clone()),
                input_root: Some(input_dir.clone()),
                suffix,
                overwrite,
                workers: effective_workers(workers),
            };
            println!(
                "{} ファイルを {} 並列で変換します → {}",
                files.len(),
                opts.workers,
                out_dir.display()
            );
            let (tx, rx) = mpsc::channel();
            let cancel = Arc::new(AtomicBool::new(false));
            let handle = BatchRunner::spawn(files, opts, tx, cancel);
            let mut done = 0usize;
            let mut total = 0usize;
            for ev in rx {
                match ev {
                    BatchEvent::Started { total: t } => total = t,
                    BatchEvent::FileStarted { .. } => {}
                    BatchEvent::FileDone(r) => {
                        done += 1;
                        if r.ok {
                            println!(
                                "[{done}/{total}] OK  {} ({:.2}s)",
                                r.dst.display(),
                                r.elapsed.as_secs_f64()
                            );
                            if let Some(note) = r.note {
                                println!("        注意: {note}");
                            }
                        } else {
                            println!(
                                "[{done}/{total}] 失敗 {}: {}",
                                r.src.display(),
                                r.error.unwrap_or_default()
                            );
                        }
                    }
                    BatchEvent::Finished {
                        ok,
                        failed,
                        skipped,
                        cancelled,
                        elapsed,
                    } => {
                        println!(
                            "完了: 成功 {ok} / 失敗 {failed} / スキップ {skipped}{} ({:.2} 秒)",
                            if cancelled { " / キャンセル" } else { "" },
                            elapsed.as_secs_f64()
                        );
                    }
                }
            }
            handle
                .join()
                .map_err(|_| anyhow!("一括処理スレッドが異常終了しました"))?;
            Ok(())
        }
        Command::Bench {
            seconds,
            sample_rate,
            workers,
            files,
        } => bench(seconds, sample_rate, effective_workers(workers), files),
    }
}

fn synth_voice(sr: u32, seconds: f32) -> Vec<f32> {
    let n = (sr as f32 * seconds) as usize;
    let mut phase = 0.0f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / sr as f32;
            let f0 = 120.0 * (1.0 + 0.05 * (std::f32::consts::TAU * 0.5 * t).sin());
            phase += std::f32::consts::TAU * f0 / sr as f32;
            (1..=24)
                .map(|k| (k as f32 * phase).sin() / k as f32)
                .sum::<f32>()
                * 0.2
        })
        .collect()
}

fn bench(seconds: f32, sr: u32, workers: usize, n_files: usize) -> Result<()> {
    let params = ConversionParams::new(5.0, 3.0);
    let audio = Audio {
        channels: vec![synth_voice(sr, seconds)],
        sample_rate: sr,
    };
    println!(
        "合成音声 {seconds:.0} 秒 / {sr} Hz / モノラル, パラメータ: ピッチ +5, フォルマント +3"
    );

    // ウォームアップ
    let _ = vc_core::convert(&audio, &params)?;

    let t = Instant::now();
    let _ = vc_core::convert(&audio, &params)?;
    let el = t.elapsed().as_secs_f64();
    println!(
        "1 ファイル(1 スレッド): {el:.2} 秒 → 実時間の {:.1} 倍速",
        seconds as f64 / el
    );

    let dir = tempfile_dir()?;
    let mut paths = Vec::new();
    for i in 0..n_files {
        let p = dir.join(format!("bench_{i}.wav"));
        vc_core::audio_io::save_audio(&p, &audio, OutputFormat::Wav)?;
        paths.push(p);
    }
    let opts = BatchOptions {
        params,
        output_format: OutputFormat::Wav,
        output_dir: Some(dir.join("out")),
        input_root: Some(dir.clone()),
        suffix: "_converted".into(),
        overwrite: true,
        workers,
    };
    let (tx, rx) = mpsc::channel();
    let t = Instant::now();
    BatchRunner::run(paths, opts, tx, Arc::new(AtomicBool::new(false)));
    let events: Vec<_> = rx.iter().collect();
    let el = t.elapsed().as_secs_f64();
    let ok = events
        .iter()
        .filter(|e| matches!(e, BatchEvent::FileDone(r) if r.ok))
        .count();
    println!(
        "{n_files} ファイル一括(読込+変換+書出, {workers} 並列): {el:.2} 秒 → 実時間の {:.1} 倍速 (成功 {ok})",
        n_files as f64 * seconds as f64 / el
    );
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

fn tempfile_dir() -> Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("vc-bench-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}
