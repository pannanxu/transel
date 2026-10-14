//! 根据配置解析 OCR 引擎。

use super::engine::{OcrBackendKind, SharedOcrEngine};
use super::engines::{
    BaiduCloudOcr, GoogleOcr, RapidOcrEngine, TencentImageTranslate, TencentOcr, VolcanoOcr,
    YoudaoCloudOcr,
};

/// 按配置字符串构造引擎。`auto` → RapidOCR。
pub fn resolve(ocr_backend: &str) -> SharedOcrEngine {
    resolve_kind(OcrBackendKind::parse(ocr_backend))
}

pub fn resolve_kind(kind: OcrBackendKind) -> SharedOcrEngine {
    match kind {
        OcrBackendKind::Auto | OcrBackendKind::RapidOcr => RapidOcrEngine::new(),
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
