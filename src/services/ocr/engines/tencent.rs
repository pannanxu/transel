//! 腾讯云通用印刷体识别 / 图片翻译。

use std::path::Path;
use std::sync::Arc;

use anyhow::anyhow;
use serde_json::json;

use crate::app::secrets;
use crate::services::cloud;
use crate::services::ocr::engine::OcrEngine;
use crate::services::ocr::image;
use crate::services::translate::ureq_agent;

pub struct TencentOcr {
    image_translate: bool,
}

impl TencentOcr {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            image_translate: false,
        })
    }

    pub fn image_translate() -> Arc<Self> {
        Arc::new(Self {
            image_translate: true,
        })
    }

    fn creds() -> anyhow::Result<(String, String, String)> {
        let id = secrets::env_or_keyring(
            &["TENCENT_SECRET_ID", "TENCENT_OCR_SECRET_ID"],
            &["tencent_secret_id", "tencent"],
        )
        .ok_or_else(|| anyhow!("未配置腾讯云 SecretId"))?;
        let key = secrets::env_or_keyring(
            &["TENCENT_SECRET_KEY", "TENCENT_OCR_SECRET_KEY"],
            &["tencent_secret_key", "tencent"],
        )
        .ok_or_else(|| anyhow!("未配置腾讯云 SecretKey"))?;
        let region = secrets::env_or_keyring(&["TENCENT_REGION"], &["tencent_region"])
            .unwrap_or_else(|| "ap-guangzhou".into());
        Ok((id, key, region))
    }
}

impl OcrEngine for TencentOcr {
    fn id(&self) -> &'static str {
        if self.image_translate {
            "tencent_image"
        } else {
            "tencent_ocr"
        }
    }

    fn display_name(&self) -> &'static str {
        if self.image_translate {
            "腾讯图片翻译"
        } else {
            "腾讯 OCR"
        }
    }

    fn recognize(&self, image: &Path, language_hint: Option<&str>) -> anyhow::Result<String> {
        let (id, key, region) = Self::creds()?;
        let b64 = image::read_base64(image)?;
        let agent = ureq_agent();
        if self.image_translate {
            let source = language_hint.unwrap_or("auto");
            let payload = json!({
                "SessionUuid": uuid::Uuid::new_v4().to_string(),
                "Scene": "doc",
                "Data": b64,
                "Source": map_tmt(source),
                "Target": "zh",
                "ProjectId": 0,
            });
            let body = serde_json::to_string(&payload)?;
            let v = cloud::tencent_json(
                &agent,
                &id,
                &key,
                &region,
                "tmt.tencentcloudapi.com",
                "tmt",
                "ImageTranslate",
                "2018-03-21",
                &body,
            )?;
            if let Some(err) = v["Response"]["Error"]["Message"].as_str() {
                anyhow::bail!("腾讯图片翻译: {err}");
            }
            let mut out = String::new();
            if let Some(arr) = v["Response"]["ImageRecord"]["Value"].as_array() {
                for item in arr {
                    let t = item["TargetText"].as_str().or(item["SourceText"].as_str());
                    if let Some(t) = t {
                        if !out.is_empty() {
                            out.push('\n');
                        }
                        out.push_str(t);
                    }
                }
            }
            if out.is_empty() {
                anyhow::bail!("腾讯图片翻译无文字");
            }
            return Ok(out);
        }

        let payload = json!({ "ImageBase64": b64 });
        let body = serde_json::to_string(&payload)?;
        let v = cloud::tencent_json(
            &agent,
            &id,
            &key,
            &region,
            "ocr.tencentcloudapi.com",
            "ocr",
            "GeneralBasicOCR",
            "2018-11-19",
            &body,
        )?;
        if let Some(err) = v["Response"]["Error"]["Message"].as_str() {
            anyhow::bail!("腾讯 OCR: {err}");
        }
        let mut out = String::new();
        if let Some(arr) = v["Response"]["TextDetections"].as_array() {
            for item in arr {
                if let Some(t) = item["DetectedText"].as_str() {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(t);
                }
            }
        }
        if out.is_empty() {
            anyhow::bail!("腾讯 OCR 无文字");
        }
        Ok(out)
    }
}

fn map_tmt(code: &str) -> &'static str {
    match code.to_ascii_lowercase().as_str() {
        "zh" | "zh-cn" | "zh-hans" => "zh",
        "en" | "en-us" => "en",
        "ja" => "ja",
        "ko" => "ko",
        _ => "auto",
    }
}
