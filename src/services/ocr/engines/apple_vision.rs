//! Apple Vision 框架 OCR 引擎（macOS 原生）。

use std::path::Path;
#[cfg(target_os = "macos")]
use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::process::Command;
use std::sync::Arc;

#[cfg(target_os = "macos")]
use anyhow::Context;
use anyhow::Result;

use crate::services::ocr::boxes::TextBox;
use crate::services::ocr::engine::OcrEngine;

#[cfg(target_os = "macos")]
const HELPER_SRC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/macos/apple-ocr.swift"
));

#[derive(Default)]
pub struct AppleVisionEngine;

impl AppleVisionEngine {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }

    /// 识别图片中的文本并返回带归一化坐标的边界框（供原图翻译 / overlay 使用）
    pub fn recognize_boxes(image: &Path) -> Result<Vec<TextBox>> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = image;
            anyhow::bail!("Apple Vision OCR 仅在 macOS 上可用");
        }
        #[cfg(target_os = "macos")]
        {
            let bin = ensure_helper()?;
            let output = Command::new(&bin)
                .arg(image)
                .arg("--boxes")
                .output()
                .with_context(|| format!("执行 Apple Vision OCR 助手失败: {}", bin.display()))?;

            if !output.status.success() {
                let err = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("Apple Vision OCR 失败: {}", err.trim());
            }

            let text = String::from_utf8_lossy(&output.stdout);
            let boxes: Vec<TextBox> = serde_json::from_str(&text)
                .with_context(|| format!("解析 Apple Vision OCR 坐标数据失败: {text}"))?;
            Ok(boxes)
        }
    }
}

impl OcrEngine for AppleVisionEngine {
    fn id(&self) -> &'static str {
        "apple_vision"
    }

    fn display_name(&self) -> &'static str {
        "Apple Vision"
    }

    fn recognize(&self, image: &Path, language_hint: Option<&str>) -> Result<String> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (image, language_hint);
            anyhow::bail!("Apple Vision OCR 仅在 macOS 上可用");
        }
        #[cfg(target_os = "macos")]
        {
            let bin = ensure_helper()?;
            let mut cmd = Command::new(&bin);
            cmd.arg(image);
            if let Some(hint) = language_hint {
                cmd.args(["--lang", hint]);
            }
            let output = cmd
                .output()
                .with_context(|| format!("执行 Apple Vision OCR 助手失败: {}", bin.display()))?;

            if !output.status.success() {
                let err = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("Apple Vision OCR 失败: {}", err.trim());
            }

            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok(text)
        }
    }
}

#[cfg(target_os = "macos")]
fn ensure_helper() -> Result<PathBuf> {
    let dir = crate::app::paths::cache_dir().join("apple-ocr");
    let src_path = dir.join("apple-ocr.swift");
    let bin_path = dir.join("apple-ocr");
    std::fs::create_dir_all(&dir)?;

    let src_changed = std::fs::read_to_string(&src_path)
        .map(|old| old != HELPER_SRC)
        .unwrap_or(true);

    if src_changed {
        std::fs::write(&src_path, HELPER_SRC)?;
    }

    let need_build = src_changed
        || !bin_path.exists()
        || (|| {
            let src_m = std::fs::metadata(&src_path).ok()?.modified().ok()?;
            let bin_m = std::fs::metadata(&bin_path).ok()?.modified().ok()?;
            Some(bin_m < src_m)
        })()
        .unwrap_or(true);

    if need_build {
        log::info!("正在编译 Apple Vision OCR 助手: {}", bin_path.display());
        let compile = Command::new("/usr/bin/swiftc")
            .args([
                "-O",
                "-o",
                bin_path.to_str().unwrap_or("apple-ocr"),
                "-framework",
                "Vision",
                "-framework",
                "AppKit",
                src_path.to_str().unwrap_or("apple-ocr.swift"),
            ])
            .output()?;

        if !compile.status.success() {
            anyhow::bail!(
                "编译 Apple Vision OCR 助手失败: {}",
                String::from_utf8_lossy(&compile.stderr).trim()
            );
        }

        let _ = Command::new("/usr/bin/codesign")
            .args(["--force", "--sign", "-", "--timestamp=none"])
            .arg(&bin_path)
            .output();
    }

    Ok(bin_path)
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn test_apple_vision_recognize_on_demo_image() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let img = manifest_dir.join("docs").join("demo1.png");
        if img.exists() {
            let engine = AppleVisionEngine::new();
            let text = engine
                .recognize(&img, None)
                .expect("Apple Vision recognize");
            assert!(text.contains("Transel") || text.contains("Google"));

            let boxes = AppleVisionEngine::recognize_boxes(&img).expect("Apple Vision boxes");
            assert!(!boxes.is_empty());
            assert!(
                boxes
                    .iter()
                    .any(|b| b.text.contains("Transel") || b.text.contains("Google"))
            );
        }
    }
}
