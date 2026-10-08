//! 変換結果の試聴(rodio)。
//!
//! デコードは rodio に任せず、アプリ本体と同じ `vc_core::audio_io::load_audio`(symphonia)で行う。
//! これにより「変換して保存できる形式」と「試聴できる形式」が常に一致する。
//! オーディオデバイスが無い環境でも落ちないよう、失敗はすべて文字列エラーにする。

use rodio::buffer::SamplesBuffer;
use std::num::NonZero;
use std::path::Path;

pub struct Preview {
    sink: rodio::MixerDeviceSink,
    player: Option<rodio::Player>,
}

impl Preview {
    pub fn new() -> Result<Self, String> {
        let sink = rodio::DeviceSinkBuilder::open_default_sink()
            .map_err(|e| format!("オーディオデバイスを開けません: {e}"))?;
        Ok(Self { sink, player: None })
    }

    pub fn play(&mut self, path: &Path) -> Result<(), String> {
        self.stop();
        let audio =
            vc_core::audio_io::load_audio(path).map_err(|e| format!("再生できません: {e}"))?;
        let channels = NonZero::new(u16::try_from(audio.channels.len()).unwrap_or(0))
            .ok_or_else(|| "再生できません: チャンネル数が不正です".to_string())?;
        let sample_rate = NonZero::new(audio.sample_rate)
            .ok_or_else(|| "再生できません: サンプルレートが不正です".to_string())?;

        // チャンネル別 → インターリーブ([L0, R0, L1, R1, ...])
        let frames = audio.frames();
        let mut interleaved = Vec::with_capacity(frames * audio.channels.len());
        for i in 0..frames {
            for ch in &audio.channels {
                interleaved.push(ch[i]);
            }
        }

        let player = rodio::Player::connect_new(self.sink.mixer());
        player.append(SamplesBuffer::new(channels, sample_rate, interleaved));
        self.player = Some(player);
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(p) = self.player.take() {
            p.stop();
        }
    }

    pub fn is_playing(&self) -> bool {
        self.player.as_ref().is_some_and(|p| !p.empty())
    }
}
