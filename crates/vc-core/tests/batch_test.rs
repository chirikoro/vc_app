mod common;

use common::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use vc_core::audio_io::{load_audio, save_audio};
use vc_core::batch::{discover_files, BatchEvent, BatchOptions, BatchRunner};
use vc_core::{ConversionParams, OutputFormat};

const SR: u32 = 16000;

fn write_voice(path: &Path, f0: f32) {
    let a = mono(make_voiced(f0, SR, 1.5), SR);
    save_audio(path, &a, OutputFormat::Wav).unwrap();
}

fn setup() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input");
    std::fs::create_dir_all(input.join("sub")).unwrap();
    write_voice(&input.join("a.wav"), 150.0);
    write_voice(&input.join("b.wav"), 150.0);
    write_voice(&input.join("sub").join("c.wav"), 150.0);
    write_voice(&input.join("a_converted.wav"), 150.0); // 既出力は除外される
    std::fs::write(input.join("notes.txt"), "not audio").unwrap();
    std::fs::write(input.join("broken.mp3"), b"garbage garbage garbage").unwrap();
    (dir, input)
}

fn collect(rx: mpsc::Receiver<BatchEvent>) -> Vec<BatchEvent> {
    rx.iter().collect()
}

#[test]
fn discover_filters_and_sorts() {
    let (_dir, input) = setup();
    let flat = discover_files(&input, false, "_converted").unwrap();
    let names: Vec<_> = flat
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert_eq!(names, vec!["a.wav", "b.wav", "broken.mp3"]);

    let deep = discover_files(&input, true, "_converted").unwrap();
    assert_eq!(deep.len(), 4);
    assert!(deep.iter().any(|p| p.ends_with("sub/c.wav")));
}

#[test]
fn batch_converts_in_parallel_and_continues_after_failure() {
    let (dir, input) = setup();
    let out = dir.path().join("out");
    let files = discover_files(&input, true, "_converted").unwrap();
    let opts = BatchOptions {
        params: ConversionParams::new(12.0, 0.0),
        output_format: OutputFormat::Wav,
        output_dir: Some(out.clone()),
        input_root: Some(input.clone()),
        suffix: "_converted".into(),
        overwrite: false,
        workers: 3,
    };
    let (tx, rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    BatchRunner::spawn(files, opts, tx, cancel).join().unwrap();
    let events = collect(rx);

    assert!(matches!(
        events.first(),
        Some(BatchEvent::Started { total: 4 })
    ));
    let done: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            BatchEvent::FileDone(r) => Some(r.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(done.len(), 4);
    let failed: Vec<_> = done.iter().filter(|r| !r.ok).collect();
    assert_eq!(failed.len(), 1);
    assert!(failed[0].src.ends_with("broken.mp3"));
    assert!(failed[0].error.is_some());

    match events.last() {
        Some(BatchEvent::Finished {
            ok,
            failed,
            skipped,
            cancelled,
            ..
        }) => {
            assert_eq!((*ok, *failed, *skipped, *cancelled), (3, 1, 0, false));
        }
        other => panic!("unexpected last event: {other:?}"),
    }

    // 出力の存在とフォルダ構造、ピッチが上がっていることを確認
    let c = out.join("sub").join("c_converted.wav");
    assert!(out.join("a_converted.wav").exists());
    assert!(out.join("b_converted.wav").exists());
    assert!(c.exists());
    let audio = load_audio(&c).unwrap();
    let f0 = estimate_f0(&audio.channels[0], SR);
    assert!((f0 - 300.0).abs() / 300.0 < 0.06, "f0={f0}");
}

#[test]
fn existing_output_is_skipped_unless_overwrite() {
    let (dir, input) = setup();
    let out = dir.path().join("out");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("a_converted.wav"), b"old").unwrap();
    let files = vec![input.join("a.wav")];
    let mut opts = BatchOptions {
        output_dir: Some(out.clone()),
        params: ConversionParams::new(2.0, 0.0),
        ..Default::default()
    };

    let (tx, rx) = mpsc::channel();
    BatchRunner::run(
        files.clone(),
        opts.clone(),
        tx,
        Arc::new(AtomicBool::new(false)),
    );
    let events = collect(rx);
    assert!(matches!(
        events.last(),
        Some(BatchEvent::Finished {
            ok: 0,
            failed: 1,
            ..
        })
    ));
    assert_eq!(std::fs::read(out.join("a_converted.wav")).unwrap(), b"old");

    opts.overwrite = true;
    let (tx, rx) = mpsc::channel();
    BatchRunner::run(files, opts, tx, Arc::new(AtomicBool::new(false)));
    let events = collect(rx);
    assert!(matches!(
        events.last(),
        Some(BatchEvent::Finished {
            ok: 1,
            failed: 0,
            ..
        })
    ));
    assert!(
        std::fs::metadata(out.join("a_converted.wav"))
            .unwrap()
            .len()
            > 100
    );
}

#[test]
fn cancel_skips_remaining_files() {
    let (dir, input) = setup();
    let files = discover_files(&input, true, "_converted").unwrap();
    let opts = BatchOptions {
        output_dir: Some(dir.path().join("out")),
        params: ConversionParams::new(2.0, 0.0),
        workers: 1,
        ..Default::default()
    };
    let (tx, rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(true)); // 開始前にキャンセル
    BatchRunner::run(files, opts, tx, cancel.clone());
    let events = collect(rx);
    match events.last() {
        Some(BatchEvent::Finished {
            ok,
            skipped,
            cancelled,
            ..
        }) => {
            assert_eq!(*ok, 0);
            assert_eq!(*skipped, 4);
            assert!(*cancelled);
        }
        other => panic!("unexpected: {other:?}"),
    }
    assert!(cancel.load(Ordering::Relaxed));
}

#[test]
fn same_format_output_keeps_flac() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("v.flac");
    let a = mono(make_voiced(150.0, SR, 1.0), SR);
    save_audio(&src, &a, OutputFormat::SameAsInput).unwrap();
    let opts = BatchOptions {
        output_format: OutputFormat::SameAsInput,
        params: ConversionParams::new(-3.0, -2.0),
        ..Default::default()
    };
    let (tx, rx) = mpsc::channel();
    BatchRunner::run(vec![src], opts, tx, Arc::new(AtomicBool::new(false)));
    let events = collect(rx);
    let done = events
        .iter()
        .find_map(|e| match e {
            BatchEvent::FileDone(r) => Some(r.clone()),
            _ => None,
        })
        .unwrap();
    assert!(done.ok, "{:?}", done.error);
    assert_eq!(done.dst, dir.path().join("v_converted.flac"));
    assert!(done.note.is_none());
    assert!(load_audio(&done.dst).is_ok());
}
