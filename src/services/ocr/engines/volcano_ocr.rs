//! 火山引擎多语种 OCR。

use std::path::Path;
use std::sync::Arc;

use anyhow::anyhow;
use serde_json::Value;

use crate::app::secrets;
use crate::services::cloud;
use crate::services::ocr::engine::OcrEngine;
use crate::services::ocr::image;
use crate::services::translate::ureq_agent;

pub struct VolcanoOcr;

impl VolcanoOcr {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

impl OcrEngine for VolcanoOcr {
    fn id(&self) -> &'static str {
        "volcano_ocr"
    }

    fn display_name(&self) -> &'static str {
        "火山 OCR"
    }

    fn recognize(&self, image: &Path, _language_hint: Option<&str>) -> anyhow::Result<String> {
        let ak = secrets::env_or_keyring(
            &["VOLCANO_ACCESS_KEY", "VOLC_ACCESS_KEY"],
            &["volcano_access_key", "volcano"],
        )
        .ok_or_else(|| anyhow!("未配置火山 Access Key"))?;
        let sk = secrets::env_or_keyring(
            &["VOLCANO_SECRET_KEY", "VOLC_SECRET_KEY"],
            &["volcano_secret_key", "volcano"],
        )
        .ok_or_else(|| anyhow!("未配置火山 Secret Key"))?;
        let region = secrets::env_or_keyring(&["VOLCANO_REGION"], &["volcano_region"])
            .unwrap_or_else(|| "cn-north-1".into());
        let b64 = image::read_base64(image)?;
        let payload = serde_json::json!({ "image_base64": b64 }).to_string();
        let v: Value = cloud::volcano_json(
            &ureq_agent(),
            &ak,
            &sk,
            &region,
            "visual.volcengineapi.com",
            "cv",
            "MultiLanguageOCR",
            "2022-08-31",
            &payload,
        )?;
        if let Some(msg) = v["ResponseMetadata"]["Error"]["Message"].as_str() {
            anyhow::bail!("火山 OCR: {msg}");
        }
        let mut out = String::new();
        let lines = v["data"]["line_texts"]
            .as_array()
            .or_else(|| v["data"]["ocr_infos"].as_array());
        if let Some(arr) = lines {
            for item in arr {
                let t = item
                    .as_str()
                    .or_else(|| item["text"].as_str())
                    .unwrap_or("");
                if t.is_empty() {
                    continue;
                }
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(t);
            }
        }
        if out.is_empty() {
            anyhow::bail!("火山 OCR 无文字: {v}");
        }
        Ok(out)
    }
}
