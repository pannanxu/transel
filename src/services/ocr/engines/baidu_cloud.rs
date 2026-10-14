//! 百度智能云通用文字识别（高精度/标准）.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context as _, anyhow};
use serde_json::Value;

use crate::app::secrets;
use crate::services::ocr::engine::OcrEngine;
use crate::services::ocr::image;
use crate::services::translate::ureq_agent;

pub struct BaiduCloudOcr;

impl BaiduCloudOcr {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

impl OcrEngine for BaiduCloudOcr {
    fn id(&self) -> &'static str {
        "baidu_cloud"
    }

    fn display_name(&self) -> &'static str {
        "百度 OCR"
    }

    fn recognize(&self, image: &Path, _language_hint: Option<&str>) -> anyhow::Result<String> {
        let ak = secrets::env_or_keyring(
            &["BAIDU_OCR_API_KEY", "BAIDU_OCR_APP_ID"],
            &["baidu_ocr_key", "baidu"],
        )
        .ok_or_else(|| anyhow!("未配置百度 OCR API Key"))?;
        let sk = secrets::env_or_keyring(
            &["BAIDU_OCR_SECRET_KEY", "BAIDU_OCR_SECRET"],
            &["baidu_ocr_secret", "baidu"],
        )
        .ok_or_else(|| anyhow!("未配置百度 OCR Secret"))?;
        let agent = ureq_agent();
        let token_v: Value = agent
            .post("https://aip.baidubce.com/oauth/2.0/token")
            .query("grant_type", "client_credentials")
            .query("client_id", &ak)
            .query("client_secret", &sk)
            .send_empty()
            .context("百度 OCR 取 token 失败")?
            .body_mut()
            .read_json()?;
        let token = token_v["access_token"]
            .as_str()
            .ok_or_else(|| anyhow!("百度 OCR token 失败: {token_v}"))?;
        let b64 = image::read_base64(image)?;
        let v: Value = agent
            .post(format!(
                "https://aip.baidubce.com/rest/2.0/ocr/v1/general_basic?access_token={token}"
            ))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .send_form([("image", b64.as_str())])
            .context("请求百度 OCR 失败")?
            .body_mut()
            .read_json()?;
        if let Some(err) = v["error_msg"].as_str() {
            anyhow::bail!("百度 OCR: {err}");
        }
        let mut out = String::new();
        if let Some(words) = v["words_result"].as_array() {
            for w in words {
                if let Some(t) = w["words"].as_str() {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(t);
                }
            }
        }
        if out.is_empty() {
            anyhow::bail!("百度 OCR 无文字");
        }
        Ok(out)
    }
}
