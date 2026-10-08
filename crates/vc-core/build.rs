//! Signalsmith Stretch(ヘッダオンリー C++)を小さな C-ABI シム経由でリンクする。
//! bindgen は使わず、`extern "C"` の数関数だけを手書きで公開する。

use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let vendor = manifest_dir.join("../../vendor/signalsmith");
    let shim = manifest_dir.join("csrc/stretch_shim.cpp");

    println!("cargo:rerun-if-changed={}", shim.display());
    println!(
        "cargo:rerun-if-changed={}",
        vendor.join("signalsmith-stretch.h").display()
    );
    for h in ["stft.h", "fft.h", "linear.h", "approx.h"] {
        println!(
            "cargo:rerun-if-changed={}",
            vendor.join("signalsmith-linear").join(h).display()
        );
    }

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .include(&vendor)
        .file(&shim)
        .warnings(false);

    // 最適化(DSP の中核なので debug ビルドでも速くしておく)
    build.opt_level(3);
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_env == "msvc" {
        // MSVC: 例外有効、静的 CRT は Rust 側の設定に従う
        build.flag("/EHsc");
    } else {
        build.flag_if_supported("-fno-exceptions");
    }
    build.compile("vc_stretch_shim");
}
