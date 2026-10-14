//! Google Cloud Vision OCR。

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context as _, anyhow};
use serde_json::{Value, json};

use crate::app::secrets;
use crate::services::ocr::engine::OcrEngine;
use crate::services::ocr::image;
use crate::services::translate::ureq_agent;

pub struct GoogleOcr;

impl GoogleOcr {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

impl OcrEngine for GoogleOcr {
    fn id(&self) -> &'static str {
        "google_ocr"
    }

    fn display_name(&self) -> &'static str {
        "Google OCR"
    }

    fn recognize(&self, image: &Path, _language_hint: Option<&str>) -> anyhow::Result<String> {
        let key = secrets::env_or_keyring(
            &["GOOGLE_OCR_API_KEY", "GOOGLE_TTS_API_KEY", "GOOGLE_API_KEY"],
            &["google_ocr_key", "google"],
        )
        .ok_or_else(|| anyhow!("未配置 Google OCR API Key"))?;
        let b64 = image::read_base64(image)?;
        let v: Value = ureq_agent()
            .post(format!(
                "https://vision.googleapis.com/v1/images:annotate?key={key}"
            ))
            .send_json(json!({
                "requests": [{
                    "image": { "content": b64 },
                    "features": [{ "type": "DOCUMENT_TEXT_DETECTION" }]
                }]
            }))
            .context("请求 Google OCR 失败")?
            .body_mut()
            .read_json()?;
        if let Some(err) = v["error"]["message"]
            .as_str()
            .or_else(|| v["responses"][0]["error"]["message"].as_str())
        {
            anyhow::bail!("Google OCR: {err}");
        }
        let text = v["responses"][0]["fullTextAnnotation"]["text"]
            .as_str()
            .or_else(|| v["responses"][0]["textAnnotations"][0]["description"].as_str())
            .ok_or_else(|| anyhow!("Google OCR 无文字"))?
            .trim()
            .to_string();
        if text.is_empty() {
            anyhow::bail!("Google OCR 无文字");
        }
        Ok(text)
    }
}
