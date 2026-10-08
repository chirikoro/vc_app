//! 1 本の音声データを変換する。

use crate::audio_io::Audio;
use crate::params::ConversionParams;
use crate::stretch::{Stretch, StretchSettings};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConversionError {
    #[error("変換パラメータが不正です: {0}")]
    InvalidParams(String),
    #[error("音声データが空です")]
    EmptyAudio,
    #[error("変換エンジンの初期化に失敗しました(チャンネル数 {channels}, サンプルレート {sample_rate} Hz)")]
    EngineInit { channels: usize, sample_rate: u32 },
}

/// 音声全体をオフラインで変換する。長さ・チャンネル数・サンプルレートは入力と同じになる。
pub fn convert(audio: &Audio, params: &ConversionParams) -> Result<Audio, ConversionError> {
    params.validate().map_err(ConversionError::InvalidParams)?;
    let channels = audio.channels.len();
    let frames = audio.frames();
    if channels == 0 || frames == 0 {
        return Err(ConversionError::EmptyAudio);
    }
    if params.is_identity() {
        return Ok(audio.clone());
    }

    let sample_rate = audio.sample_rate;
    let mut stretch = Stretch::new(channels, sample_rate as f32, params.cheaper).ok_or(
        ConversionError::EngineInit {
            channels,
            sample_rate,
        },
    )?;
    stretch.set(StretchSettings {
        transpose_semitones: params.pitch_semitones,
        formant_semitones: params.formant_semitones,
        formant_base_hz: params.formant_base_hz,
        tonality_limit_hz: params.tonality_limit_hz,
        // true: フォルマントの指定をピッチ変更と独立した絶対量として扱う
        // (false だとピッチ変更に伴ってフォルマントも動き、その上に formant_semitones が加算される)
        compensate_pitch: true,
    });

    // 出力は input_latency + output_latency サンプル遅れて出てくるので、
    // 本体を等速で処理した後、遅延分を flush で取り出し、先頭の遅延分を捨てる。
    let latency = stretch.input_latency() + stretch.output_latency();

    let mut main_out: Vec<Vec<f32>> = vec![vec![0.0; frames]; channels];
    {
        let inputs: Vec<&[f32]> = audio.channels.iter().map(|c| c.as_slice()).collect();
        let mut outputs: Vec<&mut [f32]> = main_out.iter_mut().map(|c| c.as_mut_slice()).collect();
        stretch.process(&inputs, &mut outputs);
    }
    let mut tail: Vec<Vec<f32>> = vec![vec![0.0; latency]; channels];
    {
        let mut outputs: Vec<&mut [f32]> = tail.iter_mut().map(|c| c.as_mut_slice()).collect();
        stretch.flush(&mut outputs, 1.0);
    }

    let out_channels = main_out
        .into_iter()
        .zip(tail)
        .map(|(mut main, tail)| {
            main.extend_from_slice(&tail);
            let mut aligned: Vec<f32> = main.into_iter().skip(latency).take(frames).collect();
            aligned.resize(frames, 0.0);
            for v in &mut aligned {
                if !v.is_finite() {
                    *v = 0.0;
                }
            }
            aligned
        })
        .collect();

    Ok(Audio {
        channels: out_channels,
        sample_rate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_returns_clone() {
        let a = Audio {
            channels: vec![vec![0.1, 0.2, 0.3]],
            sample_rate: 8000,
        };
        let b = convert(&a, &ConversionParams::default()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn empty_audio_is_error() {
        let a = Audio {
            channels: vec![],
            sample_rate: 8000,
        };
        assert!(matches!(
            convert(&a, &ConversionParams::new(1.0, 0.0)),
            Err(ConversionError::EmptyAudio)
        ));
    }

    #[test]
    fn preserves_shape() {
        let n = 12345;
        let a = Audio {
            channels: vec![vec![0.01; n], vec![-0.01; n]],
            sample_rate: 22050,
        };
        let b = convert(&a, &ConversionParams::new(4.0, 2.0)).unwrap();
        assert_eq!(b.channels.len(), 2);
        assert_eq!(b.frames(), n);
        assert_eq!(b.sample_rate, 22050);
    }
}
