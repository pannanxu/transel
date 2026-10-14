//! 有道智云 OCR。

use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, anyhow};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::app::secrets;
use crate::services::ocr::engine::OcrEngine;
use crate::services::ocr::image;
use crate::services::translate::ureq_agent;

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

    fn recognize(&self, image: &Path, language_hint: Option<&str>) -> anyhow::Result<String> {
        let app_id = secrets::env_or_keyring(
            &["YOUDAO_OCR_APP_ID", "YOUDAO_APP_ID"],
            &["youdao_ocr_app_id", "youdao"],
        )
        .ok_or_else(|| anyhow!("未配置有道 OCR AppKey"))?;
        let key = secrets::env_or_keyring(
            &["YOUDAO_OCR_SECRET", "YOUDAO_SECRET"],
            &["youdao_ocr_secret", "youdao"],
        )
        .ok_or_else(|| anyhow!("未配置有道 OCR 密钥"))?;
        let b64 = image::read_base64(image)?;
        let salt = uuid::Uuid::new_v4().to_string();
        let curtime = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
            .to_string();
        let sign_src = format!(
            "{}{}{}{}{}",
            truncate(&b64, 10),
            salt,
            curtime,
            app_id,
            key
        );
        let mut h = Sha256::new();
        h.update(sign_src.as_bytes());
        let sign = format!("{:x}", h.finalize());
        let lang = language_hint.unwrap_or("auto");
        let v: Value = ureq_agent()
            .post("https://openapi.youdao.com/ocrapi")
            .send_form([
                ("img", b64.as_str()),
                ("langType", lang),
                ("detectType", "10012"),
                ("imageType", "1"),
                ("appKey", app_id.as_str()),
                ("salt", salt.as_str()),
                ("sign", sign.as_str()),
                ("docType", "json"),
                ("signType", "v3"),
                ("curtime", curtime.as_str()),
            ])
            .context("请求有道 OCR 失败")?
            .body_mut()
            .read_json()?;
        if let Some(err) = v.get("errorCode").and_then(|e| e.as_str())
            && err != "0"
        {
            anyhow::bail!("有道 OCR 错误 {err}");
        }
        let mut out = String::new();
        if let Some(regions) = v["Result"]["regions"].as_array() {
            for region in regions {
                if let Some(lines) = region["lines"].as_array() {
                    for line in lines {
                        if let Some(t) = line["text"].as_str() {
                            if !out.is_empty() {
                                out.push('\n');
                            }
                            out.push_str(t);
                        }
                    }
                }
            }
        }
        if out.is_empty() {
            anyhow::bail!("有道 OCR 无文字");
        }
        Ok(out)
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    let n = s.chars().count();
    if n <= max_chars {
        s.to_string()
    } else {
        let prefix: String = s.chars().take(max_chars).collect();
        let suffix: String = s.chars().skip(n - max_chars).collect();
        format!("{prefix}{}", suffix.len())
    }
}
