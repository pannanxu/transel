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
const HELPER_BIN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/apple-ocr"));

#[derive(Default)]
pub struct AppleVisionEngine;

impl AppleVisionEngine {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }

    /// 在后台静默预热 Apple Vision 引擎，避免用户首次截图时出现 ANE 模型冷启动编译等待。
    pub fn warmup() {
        #[cfg(target_os = "macos")]
        {
            std::thread::Builder::new()
                .name("apple-vision-warmup".to_string())
                .spawn(|| {
                    if let Ok(bin) = ensure_helper() {
                        let _ = Command::new(&bin).arg("--warmup").output();
                    }
                })
                .ok();
        }
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
pub fn ensure_helper() -> Result<PathBuf> {
    let dir = crate::app::paths::cache_dir().join("apple-ocr");
    let bin_path = dir.join("apple-ocr");
    std::fs::create_dir_all(&dir)?;

    let need_write = if !bin_path.exists() {
        true
    } else {
        let current_bytes = std::fs::read(&bin_path).unwrap_or_default();
        current_bytes != HELPER_BIN
    };

    if need_write {
        if HELPER_BIN.is_empty() {
            anyhow::bail!("Apple Vision OCR 助手未在构建时预编译");
        }
        std::fs::write(&bin_path, HELPER_BIN)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&bin_path, std::fs::Permissions::from_mode(0o755));
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
