//! 图片二维码 / 条码识别。

use std::path::Path;
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
    let path = image
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("路径非 UTF-8"))?;
    // 一次性 Swift：VNDetectBarcodesRequest
    let src = format!(
        r#"
import Foundation
import Vision
import ImageIO
import CoreGraphics

let path = {path:?}
let url = URL(fileURLWithPath: path) as CFURL
guard let src = CGImageSourceCreateWithURL(url, nil),
      let cg = CGImageSourceCreateImageAtIndex(src, 0, nil) else {{
    fputs("load failed\n", stderr)
    exit(2)
}}
let handler = VNImageRequestHandler(cgImage: cg, options: [:])
let req = VNDetectBarcodesRequest()
try handler.perform([req])
guard let results = req.results else {{ exit(0) }}
for r in results {{
    if let p = r.payloadStringValue, !p.isEmpty {{
        print(p)
    }}
}}
"#
    );
    let dir = crate::app::paths::ocr_cache_dir();
    let swift_path = dir.join("st-qr-detect.swift");
    std::fs::write(&swift_path, &src)?;
    let out = Command::new("/usr/bin/swift").arg(&swift_path).output()?;
    if !out.status.success() {
        anyhow::bail!(
            "swift 二维码识别失败: {}",
            String::from_utf8_lossy(&out.stderr).trim()
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
