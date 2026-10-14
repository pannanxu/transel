//! DeepL REST API。
//!
//! 只需 Auth Key。`:fx` 结尾走免费地址 `api-free.deepl.com`，否则走 `api.deepl.com`。

use anyhow::{Context as _, Result, anyhow};
use serde_json::{Map, Value, json};

use crate::services::translate::{SingleResult, Translator};

pub struct DeepLTranslator {
    key: String,
    host: String,
    agent: ureq::Agent,
}

impl DeepLTranslator {
    pub fn new(key: String, agent: ureq::Agent) -> Self {
        let host = deepl_host(&key).into();
        Self { key, host, agent }
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
        source_hint: Option<&str>,
    ) -> Result<SingleResult> {
        let mut body = Map::new();
        body.insert("text".into(), json!([text]));
        body.insert("target_lang".into(), json!(deepl_lang(target)));
        if let Some(src) = source_hint
            .map(deepl_lang)
            .filter(|s| !s.is_empty() && s != "AUTO")
        {
            body.insert("source_lang".into(), json!(src));
        }
        let v: Value = self
            .agent
            .post(format!("{}/v2/translate", self.host))
            .header(
                "Authorization",
                format!("DeepL-Auth-Key {}", self.key.trim()),
            )
            .header("Content-Type", "application/json")
            .send_json(Value::Object(body))
            .context("请求 DeepL 失败")?
            .body_mut()
            .read_json()
            .context("解析 DeepL 响应失败")?;
        if let Some(msg) = v["message"].as_str() {
            anyhow::bail!("DeepL: {msg}");
        }
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

fn deepl_host(key: &str) -> &'static str {
    if key.trim().ends_with(":fx") {
        "https://api-free.deepl.com"
    } else {
        "https://api.deepl.com"
    }
}

fn deepl_lang(code: &str) -> String {
    let c = code.to_ascii_uppercase();
    match c.as_str() {
        "ZH-CN" | "ZH" | "ZH-HANS" => "ZH".into(),
        "ZH-TW" | "ZH-HANT" => "ZH-HANT".into(),
        "EN" | "EN-US" | "EN-GB" => "EN".into(),
        "AUTO" => "AUTO".into(),
        other => other.into(),
    }
}
