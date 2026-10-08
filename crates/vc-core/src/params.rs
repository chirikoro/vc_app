//! 変換パラメータとプリセット。

use serde::{Deserialize, Serialize};

/// 半音の許容範囲(これを超えると音質が大きく崩れる)。
pub const SEMITONE_MIN: f32 = -24.0;
pub const SEMITONE_MAX: f32 = 24.0;

/// 声の変換パラメータ。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConversionParams {
    /// ピッチ(声の高さ)の変化量 [半音]。+12 で 1 オクターブ上。
    pub pitch_semitones: f32,
    /// フォルマント(声道の長さ由来の響き)の変化量 [半音]。
    /// 正で小柄/若い印象、負で大柄/低い印象になる。
    pub formant_semitones: f32,
    /// フォルマント解析に使う基本周波数の目安 [Hz]。0 なら自動検出。
    pub formant_base_hz: f32,
    /// この周波数以上は倍音関係を保たず自然にシフトする [Hz]。0 なら無効。
    pub tonality_limit_hz: f32,
    /// 軽量モード(品質はやや低下、速度は向上)。
    pub cheaper: bool,
}

impl Default for ConversionParams {
    fn default() -> Self {
        Self {
            pitch_semitones: 0.0,
            formant_semitones: 0.0,
            formant_base_hz: 0.0,
            tonality_limit_hz: 8000.0,
            cheaper: false,
        }
    }
}

impl ConversionParams {
    pub fn new(pitch_semitones: f32, formant_semitones: f32) -> Self {
        Self {
            pitch_semitones,
            formant_semitones,
            ..Default::default()
        }
    }

    /// 値域の検証。GUI のスライダー範囲外や壊れた設定ファイルを弾く。
    pub fn validate(&self) -> Result<(), String> {
        let check = |name: &str, v: f32| -> Result<(), String> {
            if !v.is_finite() || !(SEMITONE_MIN..=SEMITONE_MAX).contains(&v) {
                Err(format!(
                    "{name} は {SEMITONE_MIN:+.0}〜{SEMITONE_MAX:+.0} 半音の範囲で指定してください(指定値: {v})"
                ))
            } else {
                Ok(())
            }
        };
        check("ピッチ", self.pitch_semitones)?;
        check("フォルマント", self.formant_semitones)?;
        if !self.formant_base_hz.is_finite() || self.formant_base_hz < 0.0 {
            return Err("フォルマント基準周波数は 0 以上で指定してください".into());
        }
        if !self.tonality_limit_hz.is_finite() || self.tonality_limit_hz < 0.0 {
            return Err("トナリティ上限は 0 以上で指定してください".into());
        }
        Ok(())
    }

    /// 範囲内に丸めたコピーを返す(壊れた設定の復旧用)。
    pub fn clamped(mut self) -> Self {
        let clamp = |v: f32| {
            if v.is_finite() {
                v.clamp(SEMITONE_MIN, SEMITONE_MAX)
            } else {
                0.0
            }
        };
        self.pitch_semitones = clamp(self.pitch_semitones);
        self.formant_semitones = clamp(self.formant_semitones);
        if !self.formant_base_hz.is_finite() || self.formant_base_hz < 0.0 {
            self.formant_base_hz = 0.0;
        }
        if !self.tonality_limit_hz.is_finite() || self.tonality_limit_hz < 0.0 {
            self.tonality_limit_hz = 8000.0;
        }
        self
    }

    /// 変換が実質的に何もしないか。
    pub fn is_identity(&self) -> bool {
        self.pitch_semitones == 0.0 && self.formant_semitones == 0.0
    }
}

/// 名前付きプリセット。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Preset {
    pub name: &'static str,
    pub description: &'static str,
    pub pitch_semitones: f32,
    pub formant_semitones: f32,
}

impl Preset {
    pub fn params(&self) -> ConversionParams {
        ConversionParams::new(self.pitch_semitones, self.formant_semitones)
    }
}

/// GUI に表示する順に並べたプリセット一覧。
pub const PRESETS: &[Preset] = &[
    Preset {
        name: "男性→女性",
        description: "ピッチとフォルマントを上げて女性的な声に",
        pitch_semitones: 5.0,
        formant_semitones: 3.0,
    },
    Preset {
        name: "女性→男性",
        description: "ピッチとフォルマントを下げて男性的な声に",
        pitch_semitones: -5.0,
        formant_semitones: -3.0,
    },
    Preset {
        name: "低く太い声",
        description: "落ち着いた低音",
        pitch_semitones: -4.0,
        formant_semitones: -2.0,
    },
    Preset {
        name: "高く軽い声",
        description: "明るく若々しい声",
        pitch_semitones: 4.0,
        formant_semitones: 2.0,
    },
    Preset {
        name: "別人風(軽め)",
        description: "本人と分かりにくい程度に少し変える",
        pitch_semitones: 2.0,
        formant_semitones: 1.5,
    },
    Preset {
        name: "別人風(強め)",
        description: "はっきり別人に聞こえるように変える",
        pitch_semitones: -3.0,
        formant_semitones: -2.5,
    },
];

/// プリセット名から検索。
pub fn find_preset(name: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.name == name)
}

/// パラメータに一致するプリセット名(無ければ None = カスタム)。
pub fn matching_preset(params: &ConversionParams) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| {
        (p.pitch_semitones - params.pitch_semitones).abs() < 1e-4
            && (p.formant_semitones - params.formant_semitones).abs() < 1e-4
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_valid() {
        for p in PRESETS {
            p.params().validate().unwrap();
            assert_eq!(matching_preset(&p.params()).unwrap().name, p.name);
        }
    }

    #[test]
    fn validate_rejects_out_of_range() {
        assert!(ConversionParams::new(30.0, 0.0).validate().is_err());
        assert!(ConversionParams::new(0.0, f32::NAN).validate().is_err());
        assert!(ConversionParams::new(-24.0, 24.0).validate().is_ok());
    }

    #[test]
    fn clamped_recovers_garbage() {
        let p = ConversionParams {
            pitch_semitones: 100.0,
            formant_semitones: f32::NAN,
            formant_base_hz: -1.0,
            tonality_limit_hz: f32::INFINITY,
            cheaper: false,
        }
        .clamped();
        assert!(p.validate().is_ok());
        assert_eq!(p.pitch_semitones, SEMITONE_MAX);
        assert_eq!(p.formant_semitones, 0.0);
    }

    #[test]
    fn serde_roundtrip_with_missing_fields() {
        let p: ConversionParams = serde_json::from_str(r#"{"pitch_semitones": 3}"#).unwrap();
        assert_eq!(p.pitch_semitones, 3.0);
        assert_eq!(p.formant_semitones, 0.0);
        let s = serde_json::to_string(&p).unwrap();
        let back: ConversionParams = serde_json::from_str(&s).unwrap();
        assert_eq!(p, back);
    }
}
