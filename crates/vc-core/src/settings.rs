//! ユーザー設定の永続化(JSON)。
//! Windows では `%APPDATA%\VoiceChanger\settings.json` に保存する。

use crate::audio_io::OutputFormat;
use crate::params::ConversionParams;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub params: ConversionParams,
    pub output_format: OutputFormat,
    pub last_input_file: Option<PathBuf>,
    pub last_input_dir: Option<PathBuf>,
    pub last_output_dir: Option<PathBuf>,
    pub recursive: bool,
    pub overwrite: bool,
    /// 0 なら自動(コア数 − 1)
    pub workers: usize,
    /// "system" | "dark" | "light"
    pub theme: String,
    /// 出力ファイル名に付ける接尾辞
    pub suffix: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            params: ConversionParams::default(),
            output_format: OutputFormat::Wav,
            last_input_file: None,
            last_input_dir: None,
            last_output_dir: None,
            recursive: false,
            overwrite: false,
            workers: 0,
            theme: "system".to_string(),
            suffix: "_converted".to_string(),
        }
    }
}

impl Settings {
    /// 設定ファイルの既定パス。取得できない環境では None。
    pub fn default_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("", "", "VoiceChanger")
            .map(|d| d.config_dir().join("settings.json"))
    }

    /// 読み込み。ファイルが無い/壊れている場合は既定値(壊れた値は丸める)。
    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => match serde_json::from_str::<Settings>(&text) {
                Ok(s) => s.sanitized(),
                Err(_) => Settings::default(),
            },
            Err(_) => Settings::default(),
        }
    }

    pub fn load() -> Self {
        Self::default_path()
            .map(|p| Self::load_from(&p))
            .unwrap_or_default()
    }

    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        // 書きかけで壊れないよう一時ファイル経由で置き換える
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)
    }

    pub fn save(&self) -> std::io::Result<()> {
        match Self::default_path() {
            Some(p) => self.save_to(&p),
            None => Err(std::io::Error::other(
                "設定ファイルの保存先を決定できません",
            )),
        }
    }

    /// 壊れた値を安全な範囲に丸める。
    pub fn sanitized(mut self) -> Self {
        self.params = self.params.clamped();
        if self.workers > 256 {
            self.workers = 0;
        }
        if !matches!(self.theme.as_str(), "system" | "dark" | "light") {
            self.theme = "system".into();
        }
        if self.suffix.trim().is_empty()
            || self
                .suffix
                .chars()
                .any(|c| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        {
            self.suffix = "_converted".into();
        }
        self
    }

    /// 実際に使うワーカー数(0 = 自動)。
    pub fn effective_workers(&self) -> usize {
        effective_workers(self.workers)
    }
}

/// 0 なら「コア数 − 1(最低 1)」、それ以外はそのまま。
pub fn effective_workers(requested: usize) -> usize {
    if requested > 0 {
        return requested;
    }
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2);
    cores.saturating_sub(1).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("settings.json");
        let s = Settings {
            params: ConversionParams::new(3.0, -1.0),
            recursive: true,
            workers: 3,
            last_input_dir: Some(PathBuf::from("C:/music")),
            ..Default::default()
        };
        s.save_to(&path).unwrap();
        let back = Settings::load_from(&path);
        assert_eq!(s, back);
    }

    #[test]
    fn broken_file_gives_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "{ this is not json").unwrap();
        assert_eq!(Settings::load_from(&path), Settings::default());
        assert_eq!(
            Settings::load_from(&dir.path().join("missing.json")),
            Settings::default()
        );
    }

    #[test]
    fn sanitizes_garbage_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(
            &path,
            r#"{"params":{"pitch_semitones":99},"workers":9999,"theme":"neon","suffix":"a/b"}"#,
        )
        .unwrap();
        let s = Settings::load_from(&path);
        assert_eq!(s.params.pitch_semitones, crate::params::SEMITONE_MAX);
        assert_eq!(s.workers, 0);
        assert_eq!(s.theme, "system");
        assert_eq!(s.suffix, "_converted");
    }

    #[test]
    fn workers_auto_is_at_least_one() {
        assert!(effective_workers(0) >= 1);
        assert_eq!(effective_workers(5), 5);
    }
}
