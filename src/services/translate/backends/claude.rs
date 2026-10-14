//! Anthropic Claude Messages API。

use anyhow::{Context as _, Result, anyhow};
use serde_json::{Value, json};

use crate::services::translate::{SingleResult, Translator};

pub struct ClaudeTranslator {
    key: String,
    model: String,
    agent: ureq::Agent,
}

impl ClaudeTranslator {
    pub fn new(key: String, model: String, agent: ureq::Agent) -> Self {
        Self { key, model, agent }
    }
}

impl Translator for ClaudeTranslator {
    fn name(&self) -> &str {
        "Claude"
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
        let body = json!({
            "model": self.model,
            "max_tokens": 4096,
            "system": format!(
                "You are a professional translator. Translate into {target}.{src} Output only the translation."
            ),
            "messages": [{"role": "user", "content": text}]
        });
        let v: Value = self
            .agent
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .send_json(body)
            .context("请求 Claude 失败")?
            .body_mut()
            .read_json()
            .context("解析 Claude 失败")?;
        if let Some(err) = v["error"]["message"].as_str() {
            anyhow::bail!("Claude 错误: {err}");
        }
        let mut out = String::new();
        if let Some(arr) = v["content"].as_array() {
            for block in arr {
                if let Some(t) = block["text"].as_str() {
                    out.push_str(t);
                }
            }
        }
        if out.trim().is_empty() {
            return Err(anyhow!("Claude 响应为空: {v}"));
        }
        Ok(SingleResult {
            backend: self.name().to_string(),
            text: out.trim().to_string(),
            source_lang: None,
            elapsed_ms: 0,
            ok: true,
            error: None,
            pending: false,
        })
    }
}
