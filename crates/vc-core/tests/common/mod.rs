//! テスト用ヘルパ: 合成有声音の生成と基本周波数の推定。
#![allow(dead_code)]

use vc_core::Audio;

/// 2 次共振器(フォルマント風)。
struct Resonator {
    b0: f32,
    a1: f32,
    a2: f32,
    y1: f32,
    y2: f32,
}

impl Resonator {
    fn new(freq: f32, bandwidth: f32, sr: f32) -> Self {
        let r = (-std::f32::consts::PI * bandwidth / sr).exp();
        let theta = std::f32::consts::TAU * freq / sr;
        Self {
            b0: (1.0 - r * r) * 0.5,
            a1: 2.0 * r * theta.cos(),
            a2: -r * r,
            y1: 0.0,
            y2: 0.0,
        }
    }
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.a1 * self.y1 + self.a2 * self.y2;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// 声門パルス列を 2 つの共振器(500 Hz / 1500 Hz)に通した「声っぽい」信号を作る。
///
/// 実際の声に近づけるため、ビブラート(±3%、5 Hz)、周期ジッター(±0.5%)、
/// 気息ノイズを加えている。完全に周期的な信号は実音声とかけ離れていて、
/// STFT ベースの処理では倍音間の漏れが極端に小さくなり現実的でない挙動を示すため。
pub fn make_voiced(f0: f32, sr: u32, seconds: f32) -> Vec<f32> {
    let n = (sr as f32 * seconds) as usize;
    let mut r1 = Resonator::new(500.0, 100.0, sr as f32);
    let mut r2 = Resonator::new(1500.0, 150.0, sr as f32);
    let mut seed = 0x1234_5678u32;
    let mut rnd = move || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
    };
    let mut next_pulse = 0.0f32;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / sr as f32;
        let inst_f0 = f0 * (1.0 + 0.03 * (std::f32::consts::TAU * 5.0 * t).sin());
        let mut x = 0.02 * rnd();
        if i as f32 >= next_pulse {
            x += 1.0;
            next_pulse += sr as f32 / inst_f0 * (1.0 + 0.01 * rnd());
        }
        let y = r1.tick(x) + 0.5 * r2.tick(x);
        out.push(y);
    }
    let peak = out.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-9);
    for v in &mut out {
        *v *= 0.5 / peak;
    }
    out
}

pub fn mono(samples: Vec<f32>, sr: u32) -> Audio {
    Audio {
        channels: vec![samples],
        sample_rate: sr,
    }
}

/// 信号中央の 1 秒分から自己相関で基本周波数を推定する(50〜600 Hz)。
pub fn estimate_f0(x: &[f32], sr: u32) -> f32 {
    let win = (sr as usize).min(x.len());
    let start = (x.len() - win) / 2;
    let seg = &x[start..start + win];
    let min_lag = (sr as f32 / 600.0) as usize;
    let max_lag = (sr as f32 / 50.0) as usize;
    let energy: f32 = seg.iter().map(|v| v * v).sum::<f32>().max(1e-12);
    let mut best = Vec::with_capacity(max_lag);
    for lag in min_lag..=max_lag.min(win / 2) {
        let mut acc = 0.0f32;
        for i in 0..win - lag {
            acc += seg[i] * seg[i + lag];
        }
        best.push((lag, acc / energy));
    }
    // 全ての倍音が揃う基本周期で自己相関は最大になる。
    // ただし 2 倍周期(1 オクターブ下)も同じ高さになるので、整数分の 1 の周期で
    // 同程度に高いものがあればそちら(短い周期)を採用する。
    let value_at = |lag: usize| -> f32 {
        best.iter()
            .find(|(l, _)| *l == lag)
            .map(|(_, v)| *v)
            .unwrap_or(f32::MIN)
    };
    let (mut lag, max) =
        best.iter().copied().fold(
            (max_lag, f32::MIN),
            |acc, (l, v)| if v > acc.1 { (l, v) } else { acc },
        );
    for div in [4usize, 3, 2] {
        let cand = lag / div;
        if cand >= min_lag && value_at(cand) >= 0.9 * max {
            lag = cand;
            break;
        }
    }
    sr as f32 / lag as f32
}

/// Goertzel 法で周波数 f [Hz] のパワーを求める(窓はなし)。
fn goertzel_power(x: &[f32], sr: u32, f: f32) -> f64 {
    let w = std::f64::consts::TAU * f as f64 / sr as f64;
    let coeff = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for &v in x {
        let s0 = v as f64 + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    s1 * s1 + s2 * s2 - coeff * s1 * s2
}

/// 信号中央 1 秒分について、f0 の倍音位置(±10 Hz)のエネルギーと
/// 倍音の中間位置(±10 Hz)のエネルギーの比を返す。周期性が f0 で保たれていれば大きな値になる。
pub fn harmonic_grid_ratio(x: &[f32], sr: u32, f0: f32, harmonics: usize) -> f64 {
    let win = (sr as usize).min(x.len());
    let start = (x.len() - win) / 2;
    let seg = &x[start..start + win];
    let band = |center: f32| -> f64 {
        (-10..=10)
            .map(|d| goertzel_power(seg, sr, center + d as f32))
            .sum::<f64>()
    };
    let mut on = 0.0;
    let mut off = 0.0;
    for k in 1..=harmonics {
        on += band(k as f32 * f0);
        off += band((k as f32 + 0.5) * f0);
    }
    on / off.max(1e-12)
}

pub fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}
