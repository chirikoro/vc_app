//! Signalsmith Stretch の安全なラッパ。`unsafe` はこのモジュールに閉じ込める。

use std::ffi::c_void;

extern "C" {
    fn vc_stretch_new(channels: i32, sample_rate: f32, cheaper: i32) -> *mut c_void;
    fn vc_stretch_free(p: *mut c_void);
    fn vc_stretch_reset(p: *mut c_void);
    fn vc_stretch_set(
        p: *mut c_void,
        transpose_semitones: f32,
        formant_semitones: f32,
        formant_base_hz: f32,
        tonality_limit_hz: f32,
        compensate_pitch: i32,
    );
    fn vc_stretch_input_latency(p: *const c_void) -> i32;
    fn vc_stretch_output_latency(p: *const c_void) -> i32;
    fn vc_stretch_process(
        p: *mut c_void,
        input: *const *const f32,
        n_in: i32,
        output: *const *mut f32,
        n_out: i32,
    );
    fn vc_stretch_flush(p: *mut c_void, output: *const *mut f32, n_out: i32, playback_rate: f32);
}

/// ピッチ/フォルマント変換器(ステートフル)。チャンネル数とサンプルレートで初期化する。
pub struct Stretch {
    raw: *mut c_void,
    channels: usize,
}

// 内部状態は raw ポインタの先にしか無く、同時アクセスは &mut で防がれる。
unsafe impl Send for Stretch {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StretchSettings {
    pub transpose_semitones: f32,
    pub formant_semitones: f32,
    /// 0 なら自動検出
    pub formant_base_hz: f32,
    /// 0 なら無効
    pub tonality_limit_hz: f32,
    pub compensate_pitch: bool,
}

impl Default for StretchSettings {
    fn default() -> Self {
        Self {
            transpose_semitones: 0.0,
            formant_semitones: 0.0,
            formant_base_hz: 0.0,
            tonality_limit_hz: 0.0,
            compensate_pitch: false,
        }
    }
}

impl Stretch {
    /// `cheaper` を true にすると軽量プリセット(品質はやや低下)。
    pub fn new(channels: usize, sample_rate: f32, cheaper: bool) -> Option<Self> {
        if channels == 0
            || channels > i32::MAX as usize
            || sample_rate.is_nan()
            || sample_rate <= 0.0
        {
            return None;
        }
        // SAFETY: 引数は検証済み。戻り値の null は失敗として扱う。
        let raw = unsafe { vc_stretch_new(channels as i32, sample_rate, cheaper as i32) };
        if raw.is_null() {
            None
        } else {
            Some(Self { raw, channels })
        }
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn reset(&mut self) {
        // SAFETY: raw は有効なポインタ。
        unsafe { vc_stretch_reset(self.raw) }
    }

    pub fn set(&mut self, s: StretchSettings) {
        // SAFETY: raw は有効なポインタ。
        unsafe {
            vc_stretch_set(
                self.raw,
                s.transpose_semitones,
                s.formant_semitones,
                s.formant_base_hz,
                s.tonality_limit_hz,
                s.compensate_pitch as i32,
            )
        }
    }

    pub fn input_latency(&self) -> usize {
        // SAFETY: raw は有効なポインタ。
        unsafe { vc_stretch_input_latency(self.raw) }.max(0) as usize
    }

    pub fn output_latency(&self) -> usize {
        // SAFETY: raw は有効なポインタ。
        unsafe { vc_stretch_output_latency(self.raw) }.max(0) as usize
    }

    /// `input[c]` を読み `output[c]` に書く。全チャンネルの長さが揃っていること。
    /// 再生速度は `input_len / output_len` で決まる(同じ長さなら等速)。
    pub fn process(&mut self, input: &[&[f32]], output: &mut [&mut [f32]]) {
        assert_eq!(input.len(), self.channels, "input channel count mismatch");
        assert_eq!(output.len(), self.channels, "output channel count mismatch");
        let n_in = input[0].len();
        let n_out = output[0].len();
        assert!(input.iter().all(|c| c.len() == n_in), "ragged input");
        assert!(output.iter().all(|c| c.len() == n_out), "ragged output");
        if n_in == 0 && n_out == 0 {
            return;
        }
        let in_ptrs: Vec<*const f32> = input.iter().map(|c| c.as_ptr()).collect();
        let out_ptrs: Vec<*mut f32> = output.iter_mut().map(|c| c.as_mut_ptr()).collect();
        // SAFETY: 各ポインタは長さ n_in / n_out の有効なバッファを指し、
        // チャンネル数は self.channels と一致する。
        unsafe {
            vc_stretch_process(
                self.raw,
                in_ptrs.as_ptr(),
                n_in as i32,
                out_ptrs.as_ptr(),
                n_out as i32,
            )
        }
    }

    /// 残りの出力を取り出す(入力はゼロとみなす)。
    pub fn flush(&mut self, output: &mut [&mut [f32]], playback_rate: f32) {
        assert_eq!(output.len(), self.channels, "output channel count mismatch");
        let n_out = output[0].len();
        assert!(output.iter().all(|c| c.len() == n_out), "ragged output");
        if n_out == 0 {
            return;
        }
        let out_ptrs: Vec<*mut f32> = output.iter_mut().map(|c| c.as_mut_ptr()).collect();
        // SAFETY: 上と同様。
        unsafe { vc_stretch_flush(self.raw, out_ptrs.as_ptr(), n_out as i32, playback_rate) }
    }
}

impl Drop for Stretch {
    fn drop(&mut self) {
        // SAFETY: new で生成した raw を一度だけ解放する。
        unsafe { vc_stretch_free(self.raw) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_and_reports_latency() {
        let s = Stretch::new(2, 48000.0, false).expect("create");
        assert!(s.input_latency() > 0);
        assert!(s.output_latency() > 0);
        assert_eq!(s.channels(), 2);
    }

    #[test]
    fn rejects_bad_args() {
        assert!(Stretch::new(0, 48000.0, false).is_none());
        assert!(Stretch::new(1, 0.0, false).is_none());
    }

    #[test]
    fn processes_without_crashing() {
        let mut s = Stretch::new(1, 16000.0, false).unwrap();
        s.set(StretchSettings {
            transpose_semitones: 3.0,
            ..Default::default()
        });
        let input: Vec<f32> = (0..16000)
            .map(|i| ((i as f32) * 0.05).sin() * 0.5)
            .collect();
        let mut output = vec![0.0f32; 16000];
        s.process(&[&input], &mut [&mut output[..]]);
        let mut tail = vec![0.0f32; 1024];
        s.flush(&mut [&mut tail[..]], 1.0);
        assert!(output.iter().all(|v| v.is_finite()));
    }
}
