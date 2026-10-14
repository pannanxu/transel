//! Google Gemini generateContent。

use anyhow::{Context as _, Result, anyhow};
use serde_json::{Value, json};

use crate::services::translate::{SingleResult, Translator};

pub struct GeminiTranslator {
    key: String,
    model: String,
    agent: ureq::Agent,
}

impl GeminiTranslator {
    pub fn new(key: String, model: String, agent: ureq::Agent) -> Self {
        Self { key, model, agent }
    }
}

impl Translator for GeminiTranslator {
    fn name(&self) -> &str {
        "Gemini"
    }

    fn translate_blocking(
        &self,
        text: &str,
        target: &str,
        source_hint: Option<&str>,
    ) -> Result<SingleResult> {
        let src = source_hint
            .map(|s| format!(" Source language: {s}."))
            .unwrap_or_default();
        let prompt = format!(
            "Translate into {target}.{src} Output only the translation.\n\n{text}"
        );
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.key
        );
        let v: Value = self
            .agent
            .post(url)
            .send_json(json!({
                "contents": [{"parts": [{"text": prompt}]}]
            }))
            .context("请求 Gemini 失败")?
            .body_mut()
            .read_json()
            .context("解析 Gemini 失败")?;
        if let Some(err) = v["error"]["message"].as_str() {
            anyhow::bail!("Gemini 错误: {err}");
        }
        let out = v["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .ok_or_else(|| anyhow!("Gemini 响应无文本: {v}"))?
            .trim()
            .to_string();
        Ok(SingleResult {
            backend: self.name().to_string(),
            text: out,
            source_lang: None,
            elapsed_ms: 0,
            ok: true,
            error: None,
            pending: false,
        })
    }
}
