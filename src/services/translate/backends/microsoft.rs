//! 微软 Azure 翻译（Translator Text REST API v3）。

use anyhow::{Context as _, Result, anyhow};
use serde_json::Value;

use crate::services::translate::{SingleResult, Translator};

pub struct MicrosoftTranslator {
    key: String,
    region: String,
    endpoint: String,
    agent: ureq::Agent,
}

impl MicrosoftTranslator {
    pub fn new(key: String, region: String, endpoint: String, agent: ureq::Agent) -> Self {
        let endpoint = endpoint.trim_end_matches('/').to_string();
        Self {
            key,
            region,
            endpoint,
            agent,
        }
    }
}

impl Translator for MicrosoftTranslator {
    fn name(&self) -> &str {
        "微软翻译"
    }

    fn translate_blocking(
        &self,
        text: &str,
        target: &str,
        source_hint: Option<&str>,
    ) -> Result<SingleResult> {
        let body = serde_json::json!([{"Text": text}]);
        let mut url = format!(
            "{}/translate?api-version=3.0&to={}",
            self.endpoint,
            urlencoding::encode(target)
        );
        if let Some(s) = source_hint {
            url.push_str("&from=");
            url.push_str(&urlencoding::encode(s));
        }
        let v: Value = self
            .agent
            .post(&url)
            .header("Ocp-Apim-Subscription-Key", &self.key)
            .header("Ocp-Apim-Subscription-Region", &self.region)
            .header("Content-Type", "application/json")
            .send_json(body)
            .context("请求微软翻译失败")?
            .body_mut()
            .read_json()
            .context("解析微软翻译响应失败")?;
        let arr = v.as_array().ok_or_else(|| anyhow!("微软响应不是数组"))?;
        let item = arr.first().ok_or_else(|| anyhow!("微软响应为空"))?;
        let translations = item["translations"]
            .as_array()
            .ok_or_else(|| anyhow!("微软响应缺少 translations"))?;
        let mut out = String::new();
        for t in translations {
            if let Some(s) = t["text"].as_str() {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(s);
            }
        }
        let detected = item["detectedLanguage"]["language"]
            .as_str()
            .map(str::to_string);
        Ok(SingleResult {
            backend: self.name().to_string(),
            text: out,
            source_lang: detected,
            elapsed_ms: 0,
            ok: true,
            error: None,
            pending: false,
        })
    }
}
