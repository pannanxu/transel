//! 图片二维码 / 条码识别。

use std::path::Path;
#[cfg(target_os = "macos")]
use std::process::Command;

/// 识别图片中的二维码/条码载荷；失败或无结果返回空 Vec（不中断 OCR）。
pub fn detect_payloads(image: &Path) -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        match detect_macos(image) {
            Ok(v) => v,
            Err(e) => {
                log::debug!("二维码识别跳过: {e:#}");
                Vec::new()
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = image;
        Vec::new()
    }
}

#[cfg(target_os = "macos")]
fn detect_macos(image: &Path) -> anyhow::Result<Vec<String>> {
    let bin = crate::services::ocr::engines::apple_vision::ensure_helper()?;
    let out = Command::new(&bin)
        .arg(image)
        .arg("--qr")
        .output()?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!(
            "Apple Vision 二维码识别失败: {}",
            err.trim()
        );
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut payloads: Vec<String> = text
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    payloads.sort();
    payloads.dedup();
    Ok(payloads)
}
