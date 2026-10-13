//! Apple 系统翻译（macOS Translation framework，通过一次性 Swift 调用）。
//!
//! 需要较新的 macOS；不可用时返回明确错误，不崩溃。

use std::process::Command;

use anyhow::{Result, anyhow};

use crate::services::translate::{SingleResult, Translator};

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
    // TranslationSession 需要 macOS 15+；用 Swift 同步等待下载模型可能很慢。
    // 这里用简洁脚本：若系统不可用则明确报错。
    let src_code = format!(
        r#"
import Foundation
import Translation

let text = {text:?}
let src = Locale.Language(identifier: {src:?})
let tgt = Locale.Language(identifier: {tgt:?})

if #available(macOS 15.0, *) {{
    let req = TranslationSession.Request(sourceText: text, source: src, target: tgt)
    let session = TranslationSession()
    let sem = DispatchSemaphore(value: 0)
    var out = ""
    var errMsg = ""
    Task {{
        do {{
            let resp = try await session.translate(req)
            out = resp.targetText
        }} catch {{
            errMsg = String(describing: error)
        }}
        sem.signal()
    }}
    _ = sem.wait(timeout: .now() + 30)
    if !errMsg.isEmpty {{
        fputs(errMsg + "\n", stderr)
        exit(2)
    }}
    print(out)
}} else {{
    fputs("需要 macOS 15+\n", stderr)
    exit(3)
}}
"#
    );
    let dir = crate::app::paths::cache_dir().join("apple-translate");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("translate.swift");
    std::fs::write(&path, &src_code)?;
    let out = Command::new("/usr/bin/swift").arg(&path).output()?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!("Apple 翻译失败: {}", err.trim());
    }
    let translated = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if translated.is_empty() {
        anyhow::bail!("Apple 翻译返回空结果");
    }
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
