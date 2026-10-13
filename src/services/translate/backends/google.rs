//! Google Cloud Translation v2 (REST, API Key)。

use anyhow::{Context as _, Result, anyhow};
use serde_json::{Value, json};

use crate::services::translate::{SingleResult, Translator};

pub struct GoogleTranslator {
    key: String,
    project: Option<String>,
    agent: ureq::Agent,
}

impl GoogleTranslator {
    pub fn new(key: String, project: Option<String>, agent: ureq::Agent) -> Self {
        Self {
            key,
            project,
            agent,
        }
    }
}

impl Translator for GoogleTranslator {
    fn name(&self) -> &str {
        "Google 翻译"
    }

    fn translate_blocking(
        &self,
        text: &str,
        target: &str,
        source_hint: Option<&str>,
    ) -> Result<SingleResult> {
        // q/target 放 JSON body，避免长文本塞进 query 触发 URI TooLong。
        let mut body = json!({
            "q": text,
            "target": target,
            "format": "text",
        });
        if let Some(s) = source_hint {
            body["source"] = json!(s);
        }
        if let Some(p) = &self.project {
            body["project"] = json!(p);
        }
        let v: Value = self
            .agent
            .post("https://translation.googleapis.com/language/translate/v2")
            .query("key", &self.key)
            .header("Content-Type", "application/json")
            .send_json(&body)
            .context("请求 Google 翻译失败")?
            .body_mut()
            .read_json()
            .context("解析 Google 响应失败")?;
        let translated = v["data"]["translations"][0]["translatedText"]
            .as_str()
            .ok_or_else(|| anyhow!("响应中没有 data.translations[0].translatedText: {v}"))?
            .to_string();
        let detected = v["data"]["translations"][0]["detectedSourceLanguage"]
            .as_str()
            .map(|s| s.to_string());
        Ok(SingleResult {
            backend: self.name().to_string(),
            text: translated,
            source_lang: detected,
            elapsed_ms: 0,
            ok: true,
            error: None,
            pending: false,
        })
    }
}
