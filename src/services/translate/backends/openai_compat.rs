//! OpenAI 兼容 /chat/completions（含 SSE 流式）。
//!
//! 兼容 OpenAI、DeepSeek、通义千问、Ollama、vLLM 等所有 `POST {base}/chat/completions`
//! 接口。默认以 `stream: true` 拉取增量，供 UI 边收边显。

use std::io::{BufRead, BufReader};

use anyhow::{Context as _, Result, anyhow};
use serde_json::{Value, json};

use crate::services::translate::{SingleResult, StreamCallback, Translator};

pub struct OpenAiCompatTranslator {
    display_name: String,
    base: String,
    key: Option<String>,
    model: String,
    temperature: f64,
    system_prompt: Option<String>,
    agent: ureq::Agent,
}

impl OpenAiCompatTranslator {
    pub fn new(
        display_name: String,
        base: String,
        key: Option<String>,
        model: String,
        temperature: f64,
        system_prompt: Option<String>,
        agent: ureq::Agent,
    ) -> Self {
        Self {
            display_name,
            base: base.trim_end_matches('/').to_string(),
            key,
            model,
            temperature,
            system_prompt,
            agent,
        }
    }

    fn system_message(&self, target: &str, source_hint: Option<&str>) -> String {
        if let Some(p) = &self.system_prompt {
            return p.clone();
        }
        let src_part = source_hint
            .map(|s| format!(" The source language is {s};"))
            .unwrap_or_default();
        format!(
            "You are a professional translator. Translate the user's text into {target}.{src_part} \
             If the text is already in {target}, translate it into English instead. \
             Output only the translation, no explanations or quotes."
        )
    }
}

impl Translator for OpenAiCompatTranslator {
    fn name(&self) -> &str {
        &self.display_name
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn translate_blocking(
        &self,
        text: &str,
        target: &str,
        source_hint: Option<&str>,
    ) -> Result<SingleResult> {
        self.translate_streaming(text, target, source_hint, &|_| true)
    }

    fn translate_streaming(
        &self,
        text: &str,
        target: &str,
        source_hint: Option<&str>,
        on_delta: StreamCallback<'_>,
    ) -> Result<SingleResult> {
        let body = json!({
            "model": self.model,
            "temperature": self.temperature,
            "stream": true,
            "messages": [
                {"role": "system", "content": self.system_message(target, source_hint)},
                {"role": "user", "content": text}
            ]
        });
        let url = format!("{}/chat/completions", self.base);
        let mut req = self.agent.post(url);
        if let Some(key) = &self.key {
            req = req.header("Authorization", format!("Bearer {key}"));
        }
        let resp = req.send_json(body).context("请求 OpenAI 兼容接口失败")?;

        let status = resp.status();
        let (_, body_part) = resp.into_parts();
        let reader = BufReader::new(body_part.into_reader());

        if !(200..300).contains(&status.as_u16()) {
            let err_body: String = reader
                .lines()
                .take(40)
                .filter_map(|l| l.ok())
                .collect::<Vec<_>>()
                .join("\n");
            return Err(anyhow!("HTTP {status}: {err_body}"));
        }

        let mut accumulated = String::new();
        for line in reader.lines() {
            let line = line.context("读取 SSE 流失败")?;
            let line = line.trim();
            if line.is_empty() || line.starts_with(':') {
                continue;
            }
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data == "[DONE]" {
                break;
            }
            let Ok(v) = serde_json::from_str::<Value>(data) else {
                continue;
            };
            // OpenAI：choices[0].delta.content；少数网关可能一次给出 message.content
            if let Some(piece) = v["choices"][0]["delta"]["content"].as_str()
                && !piece.is_empty()
            {
                accumulated.push_str(piece);
                if !on_delta(&accumulated) {
                    return Err(anyhow!("已取消"));
                }
            } else if let Some(full) = v["choices"][0]["message"]["content"].as_str()
                && !full.is_empty()
            {
                accumulated = full.to_string();
                if !on_delta(&accumulated) {
                    return Err(anyhow!("已取消"));
                }
            }
        }

        let text = accumulated.trim().to_string();
        if text.is_empty() {
            return Err(anyhow!("流式响应为空"));
        }
        Ok(SingleResult {
            backend: self.name().to_string(),
            text,
            source_lang: None,
            elapsed_ms: 0,
            ok: true,
            error: None,
            pending: false,
        })
    }
}
