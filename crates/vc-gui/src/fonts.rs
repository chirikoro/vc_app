//! 日本語表示用フォントの登録。
//!
//! egui の既定フォントには CJK が含まれないため、OS にインストール済みの日本語フォントを
//! 起動時に読み込んで登録する(exe にフォントを同梱せず小さく保つ)。

use egui::{FontData, FontDefinitions, FontFamily};
use std::path::PathBuf;
use std::sync::Arc;

/// 探索するフォント(優先順)。`(パス, TTC 内のインデックス)`。
fn candidates() -> Vec<(PathBuf, u32)> {
    let mut v: Vec<(PathBuf, u32)> = Vec::new();

    #[cfg(windows)]
    {
        let win = std::env::var_os("WINDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let fonts = win.join("Fonts");
        for name in [
            "YuGothM.ttc",  // 游ゴシック Medium(Windows 10/11)
            "YuGothR.ttc",  // 游ゴシック Regular
            "meiryo.ttc",   // メイリオ
            "msgothic.ttc", // MS ゴシック
            "BIZ-UDGothicR.ttc",
        ] {
            v.push((fonts.join(name), 0));
        }
    }

    #[cfg(target_os = "macos")]
    {
        for name in [
            "/System/Library/Fonts/ヒラギノ角ゴシック W4.ttc",
            "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
            "/Library/Fonts/Arial Unicode.ttf",
        ] {
            v.push((PathBuf::from(name), 0));
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        for name in [
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJKjp-Regular.otf",
            "/usr/share/fonts/opentype/ipafont-gothic/ipagp.ttf",
            "/usr/share/fonts/opentype/ipaexfont-gothic/ipaexg.ttf",
            "/usr/share/fonts/truetype/vlgothic/VL-PGothic-Regular.ttf",
            "/usr/share/fonts/truetype/takao-gothic/TakaoPGothic.ttf",
        ] {
            v.push((PathBuf::from(name), 0));
        }
    }

    // 環境変数で明示指定も可能
    if let Some(p) = std::env::var_os("VC_APP_FONT") {
        v.insert(0, (PathBuf::from(p), 0));
    }
    v
}

/// 見つかった日本語フォントを egui に登録する。見つからなければ既定のまま(日本語は豆腐になる)。
pub fn install(ctx: &egui::Context) -> Option<PathBuf> {
    let mut defs = FontDefinitions::default();
    for (path, index) in candidates() {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        if bytes.is_empty() {
            continue;
        }
        let mut data = FontData::from_owned(bytes);
        data.index = index;
        defs.font_data.insert("jp".to_owned(), Arc::new(data));
        // 日本語フォントを最優先にし、無いグリフ(絵文字など)は既定フォントで補う
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            defs.families
                .entry(family)
                .or_default()
                .insert(0, "jp".to_owned());
        }
        ctx.set_fonts(defs);
        return Some(path);
    }
    None
}
