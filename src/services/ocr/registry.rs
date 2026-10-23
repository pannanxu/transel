//! 根据配置解析 OCR 引擎。

use super::engine::{OcrBackendKind, SharedOcrEngine};
use super::engines::{
    AppleVisionEngine, BaiduCloudOcr, GoogleOcr, RapidOcrEngine, TencentImageTranslate, TencentOcr,
    VolcanoOcr, YoudaoCloudOcr,
};

/// 按配置字符串构造引擎。`auto` → RapidOCR。
pub fn resolve(ocr_backend: &str) -> SharedOcrEngine {
    resolve_with_options(ocr_backend, "", 0)
}

pub fn resolve_with_options(
    ocr_backend: &str,
    ocr_model: &str,
    ocr_threads: usize,
) -> SharedOcrEngine {
    resolve_kind_with_options(OcrBackendKind::parse(ocr_backend), ocr_model, ocr_threads)
}

pub fn resolve_kind(kind: OcrBackendKind) -> SharedOcrEngine {
    resolve_kind_with_options(kind, "", 0)
}

pub fn resolve_kind_with_options(
    kind: OcrBackendKind,
    ocr_model: &str,
    ocr_threads: usize,
) -> SharedOcrEngine {
    match kind {
        OcrBackendKind::AppleVision => AppleVisionEngine::new(),
        OcrBackendKind::Auto => {
            #[cfg(target_os = "macos")]
            {
                AppleVisionEngine::new()
            }
            #[cfg(not(target_os = "macos"))]
            {
                RapidOcrEngine::new(ocr_model.to_string(), ocr_threads)
            }
        }
        OcrBackendKind::RapidOcr => RapidOcrEngine::new(ocr_model.to_string(), ocr_threads),
        OcrBackendKind::Volcano => VolcanoOcr::new(),
        OcrBackendKind::Tencent => TencentOcr::new(),
        OcrBackendKind::TencentImage => TencentImageTranslate::new(),
        OcrBackendKind::BaiduCloud => BaiduCloudOcr::new(),
        OcrBackendKind::YoudaoCloud => YoudaoCloudOcr::new(),
        OcrBackendKind::Google => GoogleOcr::new(),
    }
}

/// 列出当前可选后端（供 UI）。
pub fn available_backends() -> &'static [OcrBackendKind] {
    OcrBackendKind::available()
}
