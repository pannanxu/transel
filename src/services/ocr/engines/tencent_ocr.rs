//! 腾讯云通用印刷体识别 / 图片翻译。

use std::path::Path;
use std::sync::Arc;

use anyhow::anyhow;
use serde_json::Value;
use uuid::Uuid;

use crate::app::secrets;
use crate::services::cloud;
use crate::services::ocr::engine::OcrEngine;
use crate::services::ocr::image;
use crate::services::translate::ureq_agent;

pub struct TencentOcr;

impl TencentOcr {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

impl OcrEngine for TencentOcr {
    fn id(&self) -> &'static str {
        "tencent_ocr"
    }

    fn display_name(&self) -> &'static str {
        "腾讯 OCR"
    }

    fn recognize(&self, image: &Path, _language_hint: Option<&str>) -> anyhow::Result<String> {
        let (id, key, region) = tencent_creds()?;
        let b64 = image::read_base64(image)?;
        let payload = serde_json::json!({ "ImageBase64": b64 }).to_string();
        let v: Value = cloud::tencent_json(
            &ureq_agent(),
            &id,
            &key,
            &region,
            "ocr.tencentcloudapi.com",
            "ocr",
            "GeneralBasicOCR",
            "2018-11-19",
            &payload,
        )?;
        tencent_ocr_error(&v)?;
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

pub struct TencentImageTranslate;

impl TencentImageTranslate {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

impl OcrEngine for TencentImageTranslate {
    fn id(&self) -> &'static str {
        "tencent_image"
    }

    fn display_name(&self) -> &'static str {
        "腾讯图片翻译"
    }

    fn recognize(&self, image: &Path, language_hint: Option<&str>) -> anyhow::Result<String> {
        let (id, key, region) = tencent_creds()?;
        let b64 = image::read_base64(image)?;
        let source = language_hint.unwrap_or("auto");
        let target = if source.eq_ignore_ascii_case("zh") || source.eq_ignore_ascii_case("zh-cn") {
            "en"
        } else {
            "zh"
        };
        let payload = serde_json::json!({
            "SessionUuid": Uuid::new_v4().to_string(),
            "Scene": "doc",
            "Data": b64,
            "Source": map_tmt(source),
            "Target": map_tmt(target),
            "ProjectId": 0,
        })
        .to_string();
        let v: Value = cloud::tencent_json(
            &ureq_agent(),
            &id,
            &key,
            &region,
            "tmt.tencentcloudapi.com",
            "tmt",
            "ImageTranslate",
            "2018-03-21",
            &payload,
        )?;
        tencent_ocr_error(&v)?;
        if let Some(t) = v["Response"]["TargetText"].as_str().filter(|s| !s.is_empty()) {
            return Ok(t.to_string());
        }
        let mut out = String::new();
        if let Some(arr) = v["Response"]["ImageRecord"]["Value"].as_array() {
            for item in arr {
                let t = item["TargetText"]
                    .as_str()
                    .or_else(|| item["SourceText"].as_str())
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
            anyhow::bail!("腾讯图片翻译无文字: {v}");
        }
        Ok(out)
    }
}

pub(crate) fn tencent_creds() -> anyhow::Result<(String, String, String)> {
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

fn tencent_ocr_error(v: &Value) -> anyhow::Result<()> {
    if let Some(err) = v["Response"]["Error"]["Message"].as_str() {
        anyhow::bail!("腾讯云错误: {err}");
    }
    Ok(())
}

fn map_tmt(code: &str) -> &'static str {
    match code.to_ascii_lowercase().as_str() {
        "zh" | "zh-cn" | "zh-hans" => "zh",
        "en" | "en-us" | "en-gb" => "en",
        "ja" => "ja",
        "ko" => "ko",
        _ => "auto",
    }
}
