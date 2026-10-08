mod common;

use common::*;
use vc_core::{convert, Audio, ConversionParams};

const SR: u32 = 16000;

#[test]
fn synthetic_voice_has_expected_f0() {
    let x = make_voiced(150.0, SR, 2.0);
    let f0 = estimate_f0(&x, SR);
    assert!((f0 - 150.0).abs() / 150.0 < 0.03, "f0={f0}");
}

#[test]
fn pitch_up_one_octave_doubles_f0() {
    let a = mono(make_voiced(150.0, SR, 2.0), SR);
    let b = convert(&a, &ConversionParams::new(12.0, 0.0)).unwrap();
    assert_eq!(b.frames(), a.frames());
    let f0 = estimate_f0(&b.channels[0], SR);
    assert!((f0 - 300.0).abs() / 300.0 < 0.06, "f0={f0}");
}

#[test]
fn pitch_down_seven_semitones() {
    let a = mono(make_voiced(150.0, SR, 2.0), SR);
    let b = convert(&a, &ConversionParams::new(-7.0, 0.0)).unwrap();
    let expected = 150.0 * 2f32.powf(-7.0 / 12.0);
    let f0 = estimate_f0(&b.channels[0], SR);
    assert!(
        (f0 - expected).abs() / expected < 0.06,
        "f0={f0} expected={expected}"
    );
}

#[test]
fn formant_only_keeps_f0() {
    let a = mono(make_voiced(150.0, SR, 2.0), SR);
    let b = convert(&a, &ConversionParams::new(0.0, 4.0)).unwrap();
    let f0 = estimate_f0(&b.channels[0], SR);
    assert!((f0 - 150.0).abs() / 150.0 < 0.06, "f0={f0}");
    // 倍音が 150 Hz の整数倍に留まり、フォルマント分ずれたグリッド(189 Hz)には乗っていないこと
    let ratio_ok = harmonic_grid_ratio(&b.channels[0], SR, 150.0, 10);
    let ratio_wrong = harmonic_grid_ratio(&b.channels[0], SR, 189.0, 10);
    assert!(ratio_ok > 20.0, "grid ratio={ratio_ok}");
    assert!(ratio_wrong < 5.0, "wrong grid ratio={ratio_wrong}");
    // 何かしら変化していること
    let diff: f32 = a.channels[0]
        .iter()
        .zip(&b.channels[0])
        .map(|(p, q)| (p - q).abs())
        .sum();
    assert!(diff > 1.0);
}

#[test]
fn output_is_time_aligned_with_input() {
    // 0.5〜1.0 秒だけ音がある入力。出力でも同じ区間に音が集中していれば遅延補正が正しい。
    let n = (SR as f32 * 1.5) as usize;
    let voiced = make_voiced(150.0, SR, 0.5);
    let mut x = vec![0.0f32; n];
    let start = SR as usize / 2;
    x[start..start + voiced.len()].copy_from_slice(&voiced);
    let a = mono(x, SR);
    let b = convert(&a, &ConversionParams::new(3.0, 2.0)).unwrap();
    let y = &b.channels[0];
    let ms = |t: f32| (SR as f32 * t) as usize;
    let before = rms(&y[0..ms(0.45)]);
    let inside = rms(&y[ms(0.55)..ms(0.95)]);
    let after = rms(&y[ms(1.08)..]);
    assert!(inside > 0.05, "inside={inside}");
    assert!(before < inside * 0.05, "before={before} inside={inside}");
    assert!(after < inside * 0.05, "after={after} inside={inside}");
}

#[test]
fn stereo_channels_are_processed_independently() {
    let left = make_voiced(150.0, SR, 2.0);
    let right = vec![0.0f32; left.len()];
    let a = Audio {
        channels: vec![left, right],
        sample_rate: SR,
    };
    let b = convert(&a, &ConversionParams::new(12.0, 0.0)).unwrap();
    assert_eq!(b.channels.len(), 2);
    assert!(rms(&b.channels[1]) < 1e-4, "silent channel stayed silent");
    let f0 = estimate_f0(&b.channels[0], SR);
    assert!((f0 - 300.0).abs() / 300.0 < 0.06, "f0={f0}");
}

#[test]
fn very_short_and_silent_inputs_do_not_panic() {
    for n in [1usize, 10, 100, 1000] {
        let a = mono(vec![0.0; n], SR);
        let b = convert(&a, &ConversionParams::new(5.0, 3.0)).unwrap();
        assert_eq!(b.frames(), n);
        assert!(b.channels[0].iter().all(|v| v.is_finite()));
    }
}

#[test]
fn cheaper_mode_also_works() {
    let a = mono(make_voiced(150.0, SR, 2.0), SR);
    let p = ConversionParams {
        cheaper: true,
        ..ConversionParams::new(12.0, 0.0)
    };
    let b = convert(&a, &p).unwrap();
    let f0 = estimate_f0(&b.channels[0], SR);
    assert!((f0 - 300.0).abs() / 300.0 < 0.08, "f0={f0}");
}

#[test]
fn output_level_is_reasonable() {
    let a = mono(make_voiced(150.0, SR, 2.0), SR);
    let b = convert(&a, &ConversionParams::new(-5.0, -3.0)).unwrap();
    let ra = rms(&a.channels[0]);
    let rb = rms(&b.channels[0]);
    assert!(rb > ra * 0.3 && rb < ra * 3.0, "rms in={ra} out={rb}");
    assert!(b.channels[0].iter().all(|v| v.abs() <= 1.5));
}
