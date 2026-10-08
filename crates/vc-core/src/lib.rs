//! vc-core: ボイスチェンジャーのコア処理。
//!
//! - `stretch`: Signalsmith Stretch(ピッチ/フォルマント変換)の安全なラッパ
//! - `params`: 変換パラメータとプリセット
//! - `audio_io`: 音声ファイルの読み書き
//! - `engine`: 1 本の音声データを変換する
//! - `batch`: 複数ファイルの並列一括変換
//! - `settings`: ユーザー設定の永続化

pub mod audio_io;
pub mod batch;
pub mod engine;
pub mod params;
pub mod settings;
pub mod stretch;

pub use audio_io::{Audio, AudioError, OutputFormat, SaveReport};
pub use batch::{BatchEvent, BatchOptions, BatchRunner, JobResult};
pub use engine::{convert, ConversionError};
pub use params::{ConversionParams, Preset, PRESETS};
pub use settings::Settings;
