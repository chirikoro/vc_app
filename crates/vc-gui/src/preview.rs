//! 変換結果の試聴(rodio)。
//! オーディオデバイスが無い環境でも落ちないよう、失敗はすべて文字列エラーにする。

use std::io::BufReader;
use std::path::Path;

pub struct Preview {
    _sink: rodio::MixerDeviceSink,
    player: Option<rodio::Player>,
}

impl Preview {
    pub fn new() -> Result<Self, String> {
        let sink = rodio::DeviceSinkBuilder::open_default_sink()
            .map_err(|e| format!("オーディオデバイスを開けません: {e}"))?;
        Ok(Self {
            _sink: sink,
            player: None,
        })
    }

    pub fn play(&mut self, path: &Path) -> Result<(), String> {
        self.stop();
        let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let player = rodio::play(self._sink.mixer(), BufReader::new(file))
            .map_err(|e| format!("再生できません: {e}"))?;
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
