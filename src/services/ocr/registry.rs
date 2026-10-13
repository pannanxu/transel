//! 根据配置解析 OCR 引擎。
//!
//! 本地仅 RapidOCR；云端 kind 走占位引擎，后续实现后在此接线即可。

use super::engine::{OcrBackendKind, SharedOcrEngine};
use super::engines::{BaiduCloudOcr, RapidOcrEngine, YoudaoCloudOcr};

/// 按配置字符串构造引擎。`auto` → RapidOCR。
pub fn resolve(ocr_backend: &str) -> SharedOcrEngine {
    resolve_kind(OcrBackendKind::parse(ocr_backend))
}

pub fn resolve_kind(kind: OcrBackendKind) -> SharedOcrEngine {
    match kind {
        OcrBackendKind::Auto | OcrBackendKind::RapidOcr => RapidOcrEngine::new(),
        OcrBackendKind::BaiduCloud => BaiduCloudOcr::new(),
        OcrBackendKind::YoudaoCloud => YoudaoCloudOcr::new(),
    }
}

/// 列出当前可选后端（供 UI）。
pub fn available_backends() -> &'static [OcrBackendKind] {
    OcrBackendKind::available()
}
