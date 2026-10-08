//! 音声ファイルの読み書き。
//!
//! - 読み込み: symphonia(純 Rust)。wav / flac / mp3 / ogg / m4a(aac) / aiff
//! - 書き込み: wav は `hound`、flac は `flacenc`(いずれも純 Rust)。
//!   mp3 / ogg / m4a への出力は PATH 上の ffmpeg があれば使い、無ければ wav にフォールバックする。

use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use thiserror::Error;

/// デコード済み音声(チャンネルごとの float、-1.0〜1.0)。
#[derive(Debug, Clone, PartialEq)]
pub struct Audio {
    pub channels: Vec<Vec<f32>>,
    pub sample_rate: u32,
}

impl Audio {
    pub fn frames(&self) -> usize {
        self.channels.first().map(|c| c.len()).unwrap_or(0)
    }

    pub fn duration_seconds(&self) -> f64 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.frames() as f64 / self.sample_rate as f64
        }
    }
}

/// 出力形式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OutputFormat {
    /// 常に WAV(16bit PCM)
    #[default]
    Wav,
    /// 入力と同じ形式(エンコードできない形式は WAV)
    SameAsInput,
}

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("ファイルを開けません: {path}: {source}")]
    Open {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("対応していない形式です: {path}(対応: wav, flac, mp3, ogg, m4a, aiff)")]
    Unsupported { path: PathBuf },
    #[error("音声をデコードできません: {path}: {reason}")]
    Decode { path: PathBuf, reason: String },
    #[error("音声トラックが見つかりません: {path}")]
    NoAudioTrack { path: PathBuf },
    #[error("音声データが空です: {path}")]
    Empty { path: PathBuf },
    #[error("書き込みに失敗しました: {path}: {reason}")]
    Write { path: PathBuf, reason: String },
    #[error("ffmpeg によるエンコードに失敗しました: {path}: {reason}")]
    Ffmpeg { path: PathBuf, reason: String },
}

/// 保存結果。希望の形式で保存できず WAV に置き換えた場合は `note` に説明が入る。
#[derive(Debug, Clone, PartialEq)]
pub struct SaveReport {
    pub path: PathBuf,
    pub note: Option<String>,
}

/// 読み込める拡張子(小文字)。
pub const INPUT_EXTENSIONS: &[&str] = &[
    "wav", "wave", "flac", "mp3", "ogg", "oga", "m4a", "mp4", "aac", "aiff", "aif", "aifc",
];

/// ffmpeg 無しでエンコードできる拡張子。
const NATIVE_OUTPUT_EXTENSIONS: &[&str] = &["wav", "flac"];
/// ffmpeg があればエンコードできる拡張子。
const FFMPEG_OUTPUT_EXTENSIONS: &[&str] = &["mp3", "ogg", "m4a"];

fn ext_lower(path: &Path) -> String {
    path.extension()
        .and_then(OsStr::to_str)
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default()
}

/// 入力として対応している拡張子か。
pub fn is_supported_input(path: &Path) -> bool {
    INPUT_EXTENSIONS.contains(&ext_lower(path).as_str())
}

/// 出力ファイルの拡張子を決める(ffmpeg の有無を考慮)。
pub fn output_extension(src: &Path, format: OutputFormat) -> String {
    output_extension_with(src, format, ffmpeg_path().is_some())
}

/// `output_extension` の純粋版(テスト用に ffmpeg の有無を引数で受ける)。
pub fn output_extension_with(src: &Path, format: OutputFormat, has_ffmpeg: bool) -> String {
    match format {
        OutputFormat::Wav => "wav".to_string(),
        OutputFormat::SameAsInput => {
            let ext = ext_lower(src);
            let ext = match ext.as_str() {
                "wave" => "wav".to_string(),
                "oga" => "ogg".to_string(),
                "mp4" | "aac" => "m4a".to_string(),
                _ => ext,
            };
            if NATIVE_OUTPUT_EXTENSIONS.contains(&ext.as_str())
                || (has_ffmpeg && FFMPEG_OUTPUT_EXTENSIONS.contains(&ext.as_str()))
            {
                ext
            } else {
                "wav".to_string()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 読み込み
// ---------------------------------------------------------------------------

/// 音声ファイルを読み込む。
pub fn load_audio(path: &Path) -> Result<Audio, AudioError> {
    use symphonia::core::audio::sample::Sample;
    use symphonia::core::codecs::audio::AudioDecoderOptions;
    use symphonia::core::errors::Error;
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::formats::{FormatOptions, TrackType};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;

    if !is_supported_input(path) {
        return Err(AudioError::Unsupported {
            path: path.to_path_buf(),
        });
    }
    let file = std::fs::File::open(path).map_err(|source| AudioError::Open {
        path: path.to_path_buf(),
        source,
    })?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    let ext = ext_lower(path);
    if !ext.is_empty() {
        hint.with_extension(&ext);
    }

    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| AudioError::Decode {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })?;

    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| AudioError::NoAudioTrack {
            path: path.to_path_buf(),
        })?;
    let track_id = track.id;
    let codec_params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| AudioError::NoAudioTrack {
            path: path.to_path_buf(),
        })?;

    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(codec_params, &AudioDecoderOptions::default())
        .map_err(|e| AudioError::Decode {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })?;

    let mut channels: Vec<Vec<f32>> = Vec::new();
    let mut sample_rate: u32 = 0;
    let mut planar: Vec<Vec<f32>> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(Error::ResetRequired) => break,
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => {
                return Err(AudioError::Decode {
                    path: path.to_path_buf(),
                    reason: e.to_string(),
                })
            }
        };
        if packet.track_id != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(buf) => {
                let spec = buf.spec();
                if channels.is_empty() {
                    sample_rate = spec.rate();
                    channels = vec![Vec::new(); spec.channels().count()];
                }
                buf.copy_to_vecs_planar::<f32>(&mut planar);
                for (dst, src) in channels.iter_mut().zip(planar.iter()) {
                    dst.extend_from_slice(src);
                }
            }
            // 壊れたパケットは飛ばす(MP3 の末尾などでよくある)
            Err(Error::DecodeError(_)) | Err(Error::IoError(_)) => continue,
            Err(e) => {
                return Err(AudioError::Decode {
                    path: path.to_path_buf(),
                    reason: e.to_string(),
                })
            }
        }
    }

    let _ = f32::MID; // 型推論の補助(symphonia の Sample トレイトを使うことを明示)

    if channels.is_empty() || channels[0].is_empty() || sample_rate == 0 {
        return Err(AudioError::Empty {
            path: path.to_path_buf(),
        });
    }
    // 念のため全チャンネルの長さを揃える
    let n = channels.iter().map(Vec::len).min().unwrap_or(0);
    for c in &mut channels {
        c.truncate(n);
    }
    Ok(Audio {
        channels,
        sample_rate,
    })
}

// ---------------------------------------------------------------------------
// 書き込み
// ---------------------------------------------------------------------------

/// `path` の拡張子に応じた形式で保存する。
/// 拡張子が mp3/ogg/m4a で ffmpeg が無い場合は `.wav` に置き換えて保存し、`note` で知らせる。
pub fn save_audio(
    path: &Path,
    audio: &Audio,
    _format: OutputFormat,
) -> Result<SaveReport, AudioError> {
    if audio.channels.is_empty() || audio.frames() == 0 {
        return Err(AudioError::Empty {
            path: path.to_path_buf(),
        });
    }
    let ext = ext_lower(path);
    match ext.as_str() {
        "wav" => {
            write_wav_pcm16(path, audio)?;
            Ok(SaveReport {
                path: path.to_path_buf(),
                note: None,
            })
        }
        "flac" => {
            write_flac(path, audio)?;
            Ok(SaveReport {
                path: path.to_path_buf(),
                note: None,
            })
        }
        e if FFMPEG_OUTPUT_EXTENSIONS.contains(&e) => match ffmpeg_path() {
            Some(ffmpeg) => {
                write_via_ffmpeg(&ffmpeg, path, audio)?;
                Ok(SaveReport {
                    path: path.to_path_buf(),
                    note: None,
                })
            }
            None => {
                let fallback = path.with_extension("wav");
                write_wav_pcm16(&fallback, audio)?;
                Ok(SaveReport {
                    path: fallback,
                    note: Some(format!(
                        "ffmpeg が見つからないため .{e} ではなく WAV で保存しました"
                    )),
                })
            }
        },
        _ => {
            let fallback = path.with_extension("wav");
            write_wav_pcm16(&fallback, audio)?;
            Ok(SaveReport {
                path: fallback,
                note: Some(format!(
                    "拡張子 .{ext} には出力できないため WAV で保存しました"
                )),
            })
        }
    }
}

fn to_i16(v: f32) -> i16 {
    let v = if v.is_finite() {
        v.clamp(-1.0, 1.0)
    } else {
        0.0
    };
    (v * 32767.0).round() as i16
}

fn write_wav_pcm16(path: &Path, audio: &Audio) -> Result<(), AudioError> {
    let spec = hound::WavSpec {
        channels: audio.channels.len() as u16,
        sample_rate: audio.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let wrap = |e: hound::Error| AudioError::Write {
        path: path.to_path_buf(),
        reason: e.to_string(),
    };
    let mut writer = hound::WavWriter::create(path, spec).map_err(wrap)?;
    {
        let mut w = writer.get_i16_writer((audio.frames() * audio.channels.len()) as u32);
        for i in 0..audio.frames() {
            for ch in &audio.channels {
                w.write_sample(to_i16(ch[i]));
            }
        }
        w.flush().map_err(wrap)?;
    }
    writer.finalize().map_err(wrap)
}

/// ffmpeg に渡す中間ファイル(32bit float WAV、量子化による劣化なし)。
fn write_wav_f32(path: &Path, audio: &Audio) -> Result<(), AudioError> {
    let spec = hound::WavSpec {
        channels: audio.channels.len() as u16,
        sample_rate: audio.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let wrap = |e: hound::Error| AudioError::Write {
        path: path.to_path_buf(),
        reason: e.to_string(),
    };
    let mut writer = hound::WavWriter::create(path, spec).map_err(wrap)?;
    for i in 0..audio.frames() {
        for ch in &audio.channels {
            let v = if ch[i].is_finite() {
                ch[i].clamp(-1.0, 1.0)
            } else {
                0.0
            };
            writer.write_sample(v).map_err(wrap)?;
        }
    }
    writer.finalize().map_err(wrap)
}

fn write_flac(path: &Path, audio: &Audio) -> Result<(), AudioError> {
    use flacenc::component::BitRepr;
    use flacenc::error::Verify;

    let wrap = |reason: String| AudioError::Write {
        path: path.to_path_buf(),
        reason,
    };
    let channels = audio.channels.len();
    let frames = audio.frames();
    let mut interleaved: Vec<i32> = Vec::with_capacity(frames * channels);
    for i in 0..frames {
        for ch in &audio.channels {
            interleaved.push(to_i16(ch[i]) as i32);
        }
    }
    let config = flacenc::config::Encoder::default()
        .into_verified()
        .map_err(|e| wrap(format!("FLAC 設定エラー: {e:?}")))?;
    let source = flacenc::source::MemSource::from_samples(
        &interleaved,
        channels,
        16,
        audio.sample_rate as usize,
    );
    let mut stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
        .map_err(|e| wrap(format!("FLAC エンコードエラー: {e:?}")))?;
    // 固定ブロック長ストリームでは STREAMINFO の min/max ブロックサイズを一致させる必要がある
    // (最終フレームが短くても min には反映しない)。flacenc は最終フレームの長さを min に
    // 書いてしまい、symphonia などのデコーダが可変ブロック長と誤認して読めなくなるため補正する。
    {
        let info = stream.stream_info_mut();
        let max = info.max_block_size();
        if info.min_block_size() != max {
            info.set_block_sizes(max, max)
                .map_err(|e| wrap(format!("FLAC ヘッダ補正エラー: {e:?}")))?;
        }
    }
    let mut sink = flacenc::bitsink::ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|e| wrap(format!("FLAC 書き出しエラー: {e:?}")))?;
    std::fs::write(path, sink.as_slice()).map_err(|e| wrap(e.to_string()))
}

fn write_via_ffmpeg(ffmpeg: &Path, path: &Path, audio: &Audio) -> Result<(), AudioError> {
    let ext = ext_lower(path);
    let tmp_dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let tmp = tmp_dir.join(format!(
        ".{}.{}.tmp.wav",
        path.file_stem().and_then(OsStr::to_str).unwrap_or("vc"),
        std::process::id()
    ));
    write_wav_f32(&tmp, audio)?;
    let _guard = RemoveOnDrop(tmp.clone());

    let mut cmd = Command::new(ffmpeg);
    cmd.arg("-y")
        .arg("-loglevel")
        .arg("error")
        .arg("-nostdin")
        .arg("-i")
        .arg(&tmp);
    match ext.as_str() {
        "mp3" => {
            cmd.args(["-codec:a", "libmp3lame", "-q:a", "2"]);
        }
        "ogg" => {
            cmd.args(["-codec:a", "libvorbis", "-q:a", "6"]);
        }
        "m4a" => {
            cmd.args(["-codec:a", "aac", "-b:a", "192k"]);
        }
        _ => {}
    }
    cmd.arg(path);
    hide_console(&mut cmd);

    let output = cmd.output().map_err(|e| AudioError::Ffmpeg {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;
    if !output.status.success() {
        return Err(AudioError::Ffmpeg {
            path: path.to_path_buf(),
            reason: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(())
}

struct RemoveOnDrop(PathBuf);
impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(windows)]
fn hide_console(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console(_cmd: &mut Command) {}

/// PATH または実行ファイルと同じフォルダにある ffmpeg を探す(結果はキャッシュ)。
pub fn ffmpeg_path() -> Option<PathBuf> {
    static CACHE: OnceLock<Option<PathBuf>> = OnceLock::new();
    CACHE.get_or_init(find_ffmpeg).clone()
}

fn find_ffmpeg() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(name));
        }
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            candidates.push(dir.join(name));
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_audio() -> Audio {
        let sr = 8000;
        let n = 4000;
        let left: Vec<f32> = (0..n)
            .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / sr as f32).sin() * 0.5)
            .collect();
        let right: Vec<f32> = left.iter().map(|v| -v * 0.5).collect();
        Audio {
            channels: vec![left, right],
            sample_rate: sr,
        }
    }

    fn max_abs_diff(a: &Audio, b: &Audio) -> f32 {
        a.channels
            .iter()
            .zip(&b.channels)
            .flat_map(|(x, y)| x.iter().zip(y).map(|(p, q)| (p - q).abs()))
            .fold(0.0, f32::max)
    }

    #[test]
    fn wav_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.wav");
        let a = sample_audio();
        let r = save_audio(&path, &a, OutputFormat::Wav).unwrap();
        assert_eq!(r.path, path);
        assert!(r.note.is_none());
        let b = load_audio(&path).unwrap();
        assert_eq!(b.sample_rate, 8000);
        assert_eq!(b.channels.len(), 2);
        assert_eq!(b.frames(), a.frames());
        assert!(max_abs_diff(&a, &b) < 1.0 / 32767.0 * 1.5);
    }

    #[test]
    fn flac_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.flac");
        let a = sample_audio();
        save_audio(&path, &a, OutputFormat::SameAsInput).unwrap();
        let b = load_audio(&path).unwrap();
        assert_eq!(b.sample_rate, 8000);
        assert_eq!(b.channels.len(), 2);
        assert_eq!(b.frames(), a.frames());
        assert!(max_abs_diff(&a, &b) < 1.0 / 32767.0 * 1.5);
    }

    #[test]
    fn unsupported_extension_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.wma");
        std::fs::write(&path, b"not audio").unwrap();
        assert!(matches!(
            load_audio(&path),
            Err(AudioError::Unsupported { .. })
        ));
        assert!(!is_supported_input(&path));
        assert!(is_supported_input(Path::new("x.MP3")));
    }

    #[test]
    fn garbage_wav_is_decode_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.wav");
        std::fs::write(&path, b"RIFFxxxxWAVEgarbage").unwrap();
        assert!(load_audio(&path).is_err());
    }

    #[test]
    fn output_extension_rules() {
        let p = Path::new("x/a.mp3");
        assert_eq!(output_extension_with(p, OutputFormat::Wav, true), "wav");
        assert_eq!(
            output_extension_with(p, OutputFormat::SameAsInput, true),
            "mp3"
        );
        assert_eq!(
            output_extension_with(p, OutputFormat::SameAsInput, false),
            "wav"
        );
        assert_eq!(
            output_extension_with(Path::new("a.FLAC"), OutputFormat::SameAsInput, false),
            "flac"
        );
        assert_eq!(
            output_extension_with(Path::new("a.aiff"), OutputFormat::SameAsInput, true),
            "wav"
        );
        assert_eq!(
            output_extension_with(Path::new("a.aac"), OutputFormat::SameAsInput, true),
            "m4a"
        );
    }

    #[test]
    fn unknown_output_extension_falls_back_to_wav() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.xyz");
        let r = save_audio(&path, &sample_audio(), OutputFormat::SameAsInput).unwrap();
        assert_eq!(r.path, dir.path().join("a.wav"));
        assert!(r.note.is_some());
        assert!(r.path.exists());
    }

    #[test]
    fn mp3_roundtrip_when_ffmpeg_available() {
        if ffmpeg_path().is_none() {
            eprintln!("ffmpeg が無いためスキップ");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.mp3");
        let a = sample_audio();
        let r = save_audio(&path, &a, OutputFormat::SameAsInput).unwrap();
        assert_eq!(r.path, path);
        let b = load_audio(&path).unwrap();
        assert_eq!(b.channels.len(), 2);
        assert_eq!(b.sample_rate, 8000);
        // MP3 はエンコーダ遅延で長さが少し変わるので概ね一致していればよい
        assert!((b.frames() as i64 - a.frames() as i64).abs() < 4000);
        // 中間ファイルが消えていること
        assert!(std::fs::read_dir(dir.path()).unwrap().count() == 1);
    }
}
