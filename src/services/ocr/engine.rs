//! OCR 引擎多态接口。

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;

/// 可插拔 OCR 引擎。截图仍由 `platform` 负责，这里只做「图 → 文」。
pub trait OcrEngine: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn warmup(&self) {}
    fn recognize(&self, image: &Path, language_hint: Option<&str>) -> Result<String>;
}

/// 配置字符串 / UI 用的后端标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OcrBackendKind {
    Auto,
    RapidOcr,
    Volcano,
    Tencent,
    TencentImage,
    BaiduCloud,
    YoudaoCloud,
    Google,
}

impl OcrBackendKind {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "rapidocr" | "paddle" | "ppocr" => Self::RapidOcr,
            "volcano" | "volcano_ocr" | "volc" => Self::Volcano,
            "tencent" | "tencent_ocr" => Self::Tencent,
            "tencent_image" | "tencent_image_translate" | "tmt_image" => Self::TencentImage,
            "baidu" | "baidu_cloud" | "baidu-ocr" => Self::BaiduCloud,
            "youdao" | "youdao_cloud" | "youdao-ocr" => Self::YoudaoCloud,
            "google" | "google_ocr" | "vision" => Self::Google,
            "livetext" | "live_text" | "visionkit" | "tesseract" | "winrt" | "win_rt"
            | "auto" | "" => Self::Auto,
            _ => Self::Auto,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::RapidOcr => "rapidocr",
            Self::Volcano => "volcano_ocr",
            Self::Tencent => "tencent_ocr",
            Self::TencentImage => "tencent_image",
            Self::BaiduCloud => "baidu_cloud",
            Self::YoudaoCloud => "youdao_cloud",
            Self::Google => "google_ocr",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "自动 (RapidOCR)",
            Self::RapidOcr => "RapidOCR",
            Self::Volcano => "火山 OCR",
            Self::Tencent => "腾讯 OCR",
            Self::TencentImage => "腾讯图片翻译",
            Self::BaiduCloud => "百度 OCR",
            Self::YoudaoCloud => "有道 OCR",
            Self::Google => "Google OCR",
        }
    }

    pub fn is_cloud_placeholder(self) -> bool {
        false
    }

    pub fn available() -> &'static [OcrBackendKind] {
        &[
            Self::Auto,
            Self::RapidOcr,
            Self::Volcano,
            Self::Tencent,
            Self::TencentImage,
            Self::BaiduCloud,
            Self::YoudaoCloud,
            Self::Google,
        ]
    }
}

pub type SharedOcrEngine = Arc<dyn OcrEngine>;
