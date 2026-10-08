//! UI に表示する日本語文言を一箇所に集約する。
#![allow(dead_code)]

pub const APP_TITLE: &str = "ボイスチェンジャー";

pub const TAB_SINGLE: &str = "単一ファイル";
pub const TAB_BATCH: &str = "フォルダ一括";

pub const PARAMS_HEADER: &str = "声の設定";
pub const PRESET: &str = "プリセット";
pub const PRESET_CUSTOM: &str = "カスタム";
pub const PITCH: &str = "ピッチ(声の高さ)";
pub const FORMANT: &str = "フォルマント(声の太さ)";
pub const SEMITONE_SUFFIX: &str = " 半音";
pub const CHEAPER: &str = "軽量モード(速度優先・音質やや低下)";
pub const RESET: &str = "リセット";

pub const OUTPUT_HEADER: &str = "出力";
pub const OUTPUT_FORMAT: &str = "形式";
pub const OUTPUT_WAV: &str = "WAV";
pub const OUTPUT_SAME: &str = "入力と同じ";
pub const OUTPUT_SAME_HINT: &str =
    "WAV/FLAC はそのまま、MP3/OGG/M4A は ffmpeg がある場合のみ同形式、無ければ WAV で保存します";
pub const SUFFIX: &str = "ファイル名の接尾辞";
pub const OVERWRITE: &str = "既存ファイルを上書き";
pub const WORKERS: &str = "並列数";
pub const WORKERS_AUTO_HINT: &str = "0 = 自動(CPU コア数 − 1)";
pub const THEME: &str = "外観";
pub const THEME_SYSTEM: &str = "システム";
pub const THEME_DARK: &str = "ダーク";
pub const THEME_LIGHT: &str = "ライト";
pub const FFMPEG_FOUND: &str = "ffmpeg: 検出済み(MP3/OGG/M4A 出力可)";
pub const FFMPEG_MISSING: &str = "ffmpeg: 未検出(MP3/OGG/M4A 出力は WAV になります)";

pub const SINGLE_INPUT: &str = "入力ファイル";
pub const SINGLE_DROP_HINT: &str = "ここに音声ファイルをドラッグ&ドロップ、または「選択」から開く";
pub const SINGLE_OUTPUT_DIR: &str = "出力先フォルダ";
pub const SAME_AS_INPUT_DIR: &str = "入力と同じフォルダ";
pub const CONVERT: &str = "変換";
pub const PREVIEW_PLAY: &str = "試聴";
pub const PREVIEW_STOP: &str = "停止";
pub const OPEN_FOLDER: &str = "フォルダを開く";

pub const BATCH_INPUT_DIR: &str = "入力フォルダ";
pub const BATCH_DROP_HINT: &str = "ここにフォルダをドラッグ&ドロップ、または「選択」から開く";
pub const BATCH_OUTPUT_DIR: &str = "出力フォルダ";
pub const BATCH_DEFAULT_OUTPUT: &str = "<入力フォルダ>/converted";
pub const RECURSIVE: &str = "サブフォルダも含める";
pub const RESCAN: &str = "再検索";
pub const START: &str = "一括変換を開始";
pub const CANCEL: &str = "キャンセル";
pub const FILES_FOUND: &str = "対象ファイル";
pub const NO_FILES: &str = "対応する音声ファイルがありません";

pub const SELECT: &str = "選択...";
pub const CLEAR: &str = "クリア";
pub const LOG_HEADER: &str = "ログ";
pub const CLEAR_LOG: &str = "ログを消去";
pub const STATUS_IDLE: &str = "待機中";
pub const STATUS_RUNNING: &str = "変換中";
pub const SUPPORTED_INPUT: &str = "対応入力: wav, flac, mp3, ogg, m4a, aiff";
