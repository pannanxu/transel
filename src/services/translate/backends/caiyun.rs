//! 彩云小译 API。

use anyhow::{Context as _, Result, anyhow};
use serde_json::{Value, json};

use crate::services::translate::{SingleResult, Translator};

pub struct CaiyunTranslator {
    token: String,
    agent: ureq::Agent,
}

impl CaiyunTranslator {
    pub fn new(token: String, agent: ureq::Agent) -> Self {
        Self { token, agent }
    }
}

impl Translator for CaiyunTranslator {
    fn name(&self) -> &str {
        "彩云小译"
    }

    fn translate_blocking(
        &self,
        text: &str,
        target: &str,
        source_hint: Option<&str>,
    ) -> Result<SingleResult> {
        let src = caiyun_lang(source_hint.unwrap_or("auto"));
        let tgt = caiyun_lang(target);
        let trans_type = format!("{src}2{tgt}");
        let body = json!({
            "source": text,
            "trans_type": trans_type,
            "request_id": "transel",
            "detect": true,
        });
        let v: Value = self
            .agent
            .post("https://api.interpreter.caiyunai.com/v1/translator")
            .header("Content-Type", "application/json")
            .header("X-Authorization", format!("token {}", self.token))
            .send_json(body)
            .context("请求彩云小译失败")?
            .body_mut()
            .read_json()
            .context("解析彩云小译响应失败")?;
        let text = v["target"]
            .as_str()
            .ok_or_else(|| anyhow!("彩云响应无 target: {v}"))?
            .to_string();
        Ok(SingleResult {
            backend: self.name().to_string(),
            text,
            source_lang: v["detect"]["language"].as_str().map(|s| s.to_string()),
            elapsed_ms: 0,
            ok: true,
            error: None,
            pending: false,
        })
    }
}

fn caiyun_lang(code: &str) -> &'static str {
    match code.to_ascii_lowercase().as_str() {
        "auto" => "auto",
        "zh" | "zh-cn" | "zh-hans" | "zh-tw" | "zh-hant" => "zh",
        "en" | "en-us" | "en-gb" => "en",
        "ja" => "ja",
        "ko" => "ko",
        "fr" => "fr",
        "de" => "de",
        "es" => "es",
        "ru" => "ru",
        _ => "auto",
    }
}
