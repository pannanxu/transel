//! 屏幕 OCR：截取屏幕区域 → 可插拔引擎识别。
//!
//! - [`platform`]：各平台截图
//! - [`engine`] / [`engines`] / [`registry`]：RapidOCR + 云端 OCR
//! - [`postprocess`]：智能分段；[`qr`]：二维码；[`finder`]：文件管理器选图

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

pub mod boxes;
pub mod engine;
pub mod engines;
pub mod finder;
pub mod image;
pub mod ort_runtime;
pub mod platform;
pub mod postprocess;
pub mod qr;
pub mod registry;

pub use boxes::{TextBox, image_size, recognize_boxes};
pub use engine::{OcrBackendKind, OcrEngine, SharedOcrEngine};
pub use engines::{
    DownloadProgressState, RapidOcrModelInfo, check_model_status, download_model,
    get_download_progress, is_any_model_downloading, is_downloading_model, set_downloading_model,
};
pub use ort_runtime::{
    ORT_VERSION, OrtStatus, download_and_load as download_ort_runtime, status as ort_status,
    status_label as ort_status_label,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrResult {
    pub text: String,
    pub language: Option<String>,
    /// 调试用：图片保存到哪了
    pub screenshot: Option<PathBuf>,
    /// 图片中识别到的二维码/条码载荷
    #[serde(default)]
    pub qr_payloads: Vec<String>,
}

static CURRENT: Mutex<Option<SharedOcrEngine>> = Mutex::new(None);

/// 安装当前 OCR 引擎（启动 / 设置保存后调用）。
pub fn set_engine(engine: SharedOcrEngine) {
    log::info!("OCR 引擎: {} ({})", engine.display_name(), engine.id());
    *CURRENT.lock() = Some(engine);
}

/// 按配置切换引擎。
pub fn set_engine_from_config(ocr_backend: &str, ocr_model: &str, ocr_threads: usize) {
    set_engine(registry::resolve_with_options(ocr_backend, ocr_model, ocr_threads));
}

fn current_engine() -> SharedOcrEngine {
    let mut g = CURRENT.lock();
    if g.is_none() {
        *g = Some(registry::resolve("auto"));
    }
    g.as_ref().unwrap().clone()
}

/// 解析配置并设置引擎（已停用后台预热）。
pub fn warmup_with_config(ocr_backend: &str, ocr_model: &str, ocr_threads: usize) {
    set_engine_from_config(ocr_backend, ocr_model, ocr_threads);
}

/// 兼容调用（已停用预热）。
pub fn warmup() {}

/// 截取整屏并识别。
pub fn capture_and_recognize(language_hint: Option<&str>) -> Result<OcrResult> {
    let (_img, path) = platform::capture_full_screen()?;
    recognize_path(&path, language_hint)
}

/// 交互式框选截图（不识别）。
pub fn capture_interactive() -> Result<PathBuf> {
    platform::capture_interactive()
}

/// 对已有截图路径做 OCR（含二维码探测；不做智能分段，由调用方按配置决定）。
pub fn recognize_path(path: &Path, language_hint: Option<&str>) -> Result<OcrResult> {
    recognize_path_ex(path, language_hint, true)
}

/// 对已有图片路径做 OCR，可开关二维码探测。
pub fn recognize_path_ex(
    path: &Path,
    language_hint: Option<&str>,
    detect_qr: bool,
) -> Result<OcrResult> {
    let engine = current_engine();
    let text = engine.recognize(path, language_hint)?;
    let qr_payloads = if detect_qr {
        qr::detect_payloads(path)
    } else {
        Vec::new()
    };
    Ok(OcrResult {
        text,
        language: None,
        screenshot: Some(path.to_path_buf()),
        qr_payloads,
    })
}

/// 按配置后处理 OCR 文本（智能分段 + 二维码优先展示）。
pub fn finalize_text(raw: &str, qr: &[String], smart_paragraph: bool) -> String {
    let body = if smart_paragraph {
        postprocess::smart_paragraph(raw)
    } else {
        raw.to_string()
    };
    postprocess::merge_with_qr(&body, qr)
}

/// 交互式框选并识别。
pub fn capture_interactive_and_recognize(language_hint: Option<&str>) -> Result<OcrResult> {
    let path = capture_interactive()?;
    recognize_path(&path, language_hint)
}

/// 截取指定矩形并识别。
pub fn capture_region_and_recognize(
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    language_hint: Option<&str>,
) -> Result<OcrResult> {
    let (_img, path) = platform::capture_region(x, y, w, h)?;
    recognize_path(&path, language_hint)
}

/// 当前引擎 id（调试/UI）。
pub fn current_engine_id() -> String {
    current_engine().id().to_string()
}

pub fn available_backends() -> &'static [OcrBackendKind] {
    registry::available_backends()
}

// 供外部持有引擎引用
pub type EngineHandle = Arc<dyn OcrEngine>;
