//! Apple 系统翻译（macOS Translation framework）。
//!
//! 把 `macos/apple-translate.swift` 编进缓存里的 `.app` 包再调用：
//! 语言包已安装则直接翻译；否则弹出助手窗口走 `.translationTask` 下载。

#[cfg(target_os = "macos")]
use std::process::Command;

use anyhow::Result;

use crate::services::translate::{SingleResult, Translator};

#[cfg(target_os = "macos")]
static HELPER_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(target_os = "macos")]
const HELPER_BIN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/apple-translate"));

#[derive(Default)]
pub struct AppleTranslator;

impl AppleTranslator {
    pub fn new() -> Self {
        Self
    }
}

impl Translator for AppleTranslator {
    fn name(&self) -> &str {
        "Apple 翻译"
    }

    fn translate_blocking(
        &self,
        text: &str,
        target: &str,
        source_hint: Option<&str>,
    ) -> Result<SingleResult> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (text, target, source_hint);
            anyhow::bail!("Apple 翻译仅支持 macOS");
        }
        #[cfg(target_os = "macos")]
        {
            translate_macos(text, target, source_hint)
        }
    }
}

#[cfg(target_os = "macos")]
fn translate_macos(text: &str, target: &str, source_hint: Option<&str>) -> Result<SingleResult> {
    let src = apple_lang(source_hint.unwrap_or("en"));
    let tgt = apple_lang(target);
    if src == tgt {
        return Ok(SingleResult {
            backend: "Apple 翻译".into(),
            text: text.to_string(),
            source_lang: Some(src.to_string()),
            elapsed_ms: 0,
            ok: true,
            error: None,
            pending: false,
        });
    }
    let app = ensure_helper()?;
    let seq = HELPER_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let work = crate::app::paths::cache_dir()
        .join(format!("apple-translate/run-{}-{seq}", std::process::id()));
    std::fs::create_dir_all(&work)?;
    let in_path = work.join("in.txt");
    let out_path = work.join("out.txt");
    let err_path = work.join("err.txt");
    std::fs::write(&in_path, text)?;
    let _ = std::fs::remove_file(&out_path);
    let _ = std::fs::remove_file(&err_path);
    // 直接跑包内二进制，避免 `open -a` 抢焦点把翻译浮窗关掉。
    let bin = app.join("Contents/MacOS/apple-translate");
    let status = Command::new(&bin)
        .arg(src)
        .arg(tgt)
        .arg(&in_path)
        .arg(&out_path)
        .arg(&err_path)
        .status()?;
    let translated = std::fs::read_to_string(&out_path)
        .unwrap_or_default()
        .trim()
        .to_string();
    if translated.is_empty() {
        let err = std::fs::read_to_string(&err_path)
            .unwrap_or_default()
            .trim()
            .to_string();
        let _ = std::fs::remove_dir_all(&work);
        if err.is_empty() {
            anyhow::bail!("Apple 翻译失败（退出码 {}）", status.code().unwrap_or(-1));
        }
        anyhow::bail!("Apple 翻译失败: {err}");
    }
    let _ = std::fs::remove_dir_all(&work);
    Ok(SingleResult {
        backend: "Apple 翻译".into(),
        text: translated,
        source_lang: Some(src.to_string()),
        elapsed_ms: 0,
        ok: true,
        error: None,
        pending: false,
    })
}

#[cfg(target_os = "macos")]
fn ensure_helper() -> Result<std::path::PathBuf> {
    let dir = crate::app::paths::cache_dir().join("apple-translate");
    let app = dir.join("AppleTranslate.app");
    let macos_dir = app.join("Contents/MacOS");
    let bin_path = macos_dir.join("apple-translate");
    let plist_path = app.join("Contents/Info.plist");
    std::fs::create_dir_all(&macos_dir)?;

    let need_write = if !bin_path.exists() || !plist_path.exists() {
        true
    } else {
        let current_bytes = std::fs::read(&bin_path).unwrap_or_default();
        current_bytes != HELPER_BIN
    };

    if need_write {
        if HELPER_BIN.is_empty() {
            anyhow::bail!("Apple 翻译助手未在构建时预编译");
        }
        std::fs::write(&plist_path, HELPER_PLIST)?;
        std::fs::write(&bin_path, HELPER_BIN)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&bin_path, std::fs::Permissions::from_mode(0o755));
        }
        let _ = Command::new("/usr/bin/codesign")
            .args(["--force", "--sign", "-", "--timestamp=none"])
            .arg(&app)
            .output();
    }
    Ok(app)
}

#[cfg(target_os = "macos")]
const HELPER_PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>Apple Translate</string>
  <key>CFBundleDisplayName</key>
  <string>Apple 翻译</string>
  <key>CFBundleIdentifier</key>
  <string>dev.transel.apple-translate</string>
  <key>CFBundleExecutable</key>
  <string>apple-translate</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>LSMinimumSystemVersion</key>
  <string>15.0</string>
  <key>LSUIElement</key>
  <true/>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
"#;

#[cfg(target_os = "macos")]
fn apple_lang(code: &str) -> &'static str {
    match code.to_ascii_lowercase().as_str() {
        "zh" | "zh-cn" | "zh-hans" => "zh-Hans",
        "zh-tw" | "zh-hant" => "zh-Hant",
        "en" | "en-us" | "en-gb" => "en",
        "ja" => "ja",
        "ko" => "ko",
        "fr" => "fr",
        "de" => "de",
        "es" => "es",
        "ru" => "ru",
        _ => "en",
    }
}
