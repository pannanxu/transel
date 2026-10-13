//! 有道云 OCR 占位。
//!
//! 预留与 [`crate::services::ocr::engine::OcrEngine`] 同一接口；
//! 正式接入时在此实现鉴权与 ocrapi 调用即可。

use std::path::Path;
use std::sync::Arc;

use crate::services::ocr::engine::OcrEngine;

pub struct YoudaoCloudOcr;

impl YoudaoCloudOcr {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

impl OcrEngine for YoudaoCloudOcr {
    fn id(&self) -> &'static str {
        "youdao_cloud"
    }

    fn display_name(&self) -> &'static str {
        "有道 OCR"
    }

    fn recognize(&self, _image: &Path, _language_hint: Option<&str>) -> anyhow::Result<String> {
        anyhow::bail!("有道云 OCR 尚未接入，请先使用 RapidOCR")
    }
}
