//! 小牛翻译（NiuTrans）文本翻译。

use anyhow::{Context as _, Result, anyhow};
use serde_json::Value;

use crate::services::translate::{SingleResult, Translator};

pub struct NiutransTranslator {
    key: String,
    agent: ureq::Agent,
}

impl NiutransTranslator {
    pub fn new(key: String, agent: ureq::Agent) -> Self {
        Self { key, agent }
    }
}

impl Translator for NiutransTranslator {
    fn name(&self) -> &str {
        "小牛翻译"
    }

    fn translate_blocking(
        &self,
        text: &str,
        target: &str,
        source_hint: Option<&str>,
    ) -> Result<SingleResult> {
        let from = map_lang(source_hint.unwrap_or("auto"));
        let to = map_lang(target);
        let v: Value = self
            .agent
            .post("https://api.niutrans.com/NiuTransServer/translation")
            .send_json(serde_json::json!({
                "from": from,
                "to": to,
                "src_text": text,
                "apikey": self.key,
            }))
            .context("请求小牛翻译失败")?
            .body_mut()
            .read_json()
            .context("解析小牛翻译失败")?;
        if let Some(err) = v["error_msg"].as_str().filter(|s| !s.is_empty()) {
            anyhow::bail!("小牛翻译错误: {err}");
        }
        let out = v["tgt_text"]
            .as_str()
            .ok_or_else(|| anyhow!("小牛响应无 tgt_text: {v}"))?
            .to_string();
        Ok(SingleResult {
            backend: self.name().to_string(),
            text: out,
            source_lang: source_hint.map(str::to_string),
            elapsed_ms: 0,
            ok: true,
            error: None,
            pending: false,
        })
    }
}

fn map_lang(code: &str) -> String {
    match code.to_ascii_lowercase().as_str() {
        "zh" | "zh-cn" | "zh-hans" => "zh".into(),
        "zh-tw" | "zh-hant" => "cht".into(),
        "auto" => "auto".into(),
        other => other.to_string(),
    }
}
