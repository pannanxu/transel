//! OCR 引擎多态接口。
//!
//! 本地引擎目前仅 RapidOCR；云端（百度 / 有道）预留同一 trait，后续接入即可。

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
    /// 映射到 RapidOCR。
    Auto,
    RapidOcr,
    /// 云端占位：尚未实现，选择后会提示。
    BaiduCloud,
    /// 云端占位：尚未实现，选择后会提示。
    YoudaoCloud,
}

impl OcrBackendKind {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "rapidocr" | "paddle" | "ppocr" => Self::RapidOcr,
            "baidu" | "baidu_cloud" | "baidu-ocr" => Self::BaiduCloud,
            "youdao" | "youdao_cloud" | "youdao-ocr" => Self::YoudaoCloud,
            // 旧配置（Vision / Live Text / Tesseract / WinRT）统一落到 Auto → RapidOCR
            "vision" | "livetext" | "live_text" | "visionkit" | "tesseract" | "winrt"
            | "win_rt" | "auto" | "" => Self::Auto,
            _ => Self::Auto,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::RapidOcr => "rapidocr",
            Self::BaiduCloud => "baidu_cloud",
            Self::YoudaoCloud => "youdao_cloud",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "自动 (RapidOCR)",
            Self::RapidOcr => "RapidOCR",
            Self::BaiduCloud => "百度 OCR（即将推出）",
            Self::YoudaoCloud => "有道 OCR（即将推出）",
        }
    }

    /// 是否为尚未实现的云端后端。
    pub fn is_cloud_placeholder(self) -> bool {
        matches!(self, Self::BaiduCloud | Self::YoudaoCloud)
    }

    /// 当前可选 OCR 后端。
    pub fn available() -> &'static [OcrBackendKind] {
        &[
            Self::Auto,
            Self::RapidOcr,
            Self::BaiduCloud,
            Self::YoudaoCloud,
        ]
    }
}

pub type SharedOcrEngine = Arc<dyn OcrEngine>;













