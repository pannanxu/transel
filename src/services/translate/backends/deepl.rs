//! DeepL REST API。

use anyhow::{Context as _, Result, anyhow};
use serde_json::{Value, json};

use crate::services::translate::{SingleResult, Translator};

pub struct DeepLTranslator {
    key: String,
    endpoint: String, // "pro" or "free"
    agent: ureq::Agent,
}

impl DeepLTranslator {
    pub fn new(key: String, endpoint: String, agent: ureq::Agent) -> Self {
        let endpoint = match endpoint.as_str() {
            "free" | "api-free" => "https://api-free.deepl.com".into(),
            _ => "https://api.deepl.com".into(),
        };
        Self {
            key,
            endpoint,
            agent,
        }
    }
}

impl Translator for DeepLTranslator {
    fn name(&self) -> &str {
        "DeepL"
    }

    fn translate_blocking(
        &self,
        text: &str,
        target: &str,
        _source_hint: Option<&str>,
    ) -> Result<SingleResult> {
        // DeepL 接受 zh / en / ja 等；中文用 ZH
        let target = deepl_lang(target);
        let body = json!({
            "text": [text],
            "target_lang": target,
            "source_lang": "auto",
        });
        let v: Value = self
            .agent
            .post(format!("{}/v2/translate", self.endpoint))
            .header("Authorization", format!("DeepL-Auth-Key {}", self.key))
            .send_json(body)
            .context("请求 DeepL 失败")?
            .body_mut()
            .read_json()
            .context("解析 DeepL 响应失败")?;
        let text = v["translations"][0]["text"]
            .as_str()
            .ok_or_else(|| anyhow!("响应中没有 translations[0].text: {v}"))?
            .to_string();
        let src = v["translations"][0]["detected_source_language"]
            .as_str()
            .map(|s| s.to_ascii_lowercase());
        Ok(SingleResult {
            backend: self.name().to_string(),
            text,
            source_lang: src,
            elapsed_ms: 0,
            ok: true,
            error: None,
            pending: false,
        })
    }
}

fn deepl_lang(code: &str) -> String {
    let c = code.to_ascii_uppercase();
    match c.as_str() {
        "ZH-CN" | "ZH" | "ZH-HANS" => "ZH".into(),
        "ZH-TW" | "ZH-HANT" => "ZH-HANT".into(),
        "EN" | "EN-US" | "EN-GB" => "EN".into(),
        "JA" => "JA".into(),
        "KO" => "KO".into(),
        "FR" => "FR".into(),
        "DE" => "DE".into(),
        "ES" => "ES".into(),
        "RU" => "RU".into(),
        other => other.into(),
    }
}
