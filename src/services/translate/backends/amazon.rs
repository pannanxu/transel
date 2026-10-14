//! Amazon Translate（AWS SigV4）。

use anyhow::{Result, anyhow};
use serde_json::{Value, json};

use crate::services::cloud;
use crate::services::translate::{SingleResult, Translator};

pub struct AmazonTranslator {
    access_key: String,
    secret_key: String,
    region: String,
    agent: ureq::Agent,
}

impl AmazonTranslator {
    pub fn new(access_key: String, secret_key: String, region: String, agent: ureq::Agent) -> Self {
        Self {
            access_key,
            secret_key,
            region,
            agent,
        }
    }
}

impl Translator for AmazonTranslator {
    fn name(&self) -> &str {
        "Amazon 翻译"
    }

    fn translate_blocking(
        &self,
        text: &str,
        target: &str,
        source_hint: Option<&str>,
    ) -> Result<SingleResult> {
        let host = format!("translate.{}.amazonaws.com", self.region);
        let mut payload = json!({
            "Text": text,
            "SourceLanguageCode": map_lang(source_hint.unwrap_or("auto")),
            "TargetLanguageCode": map_lang(target),
        });
        if payload["SourceLanguageCode"] == "auto" {
            payload["SourceLanguageCode"] = json!("auto");
        }
        let body = serde_json::to_string(&payload)?;
        let v: Value = cloud::aws_json(
            &self.agent,
            &self.access_key,
            &self.secret_key,
            &self.region,
            &host,
            "translate",
            "AWSShineFrontendService_20170701.TranslateText",
            &body,
        )?;
        if let Some(msg) = v["Message"].as_str().or_else(|| v["message"].as_str()) {
            anyhow::bail!("Amazon 翻译错误: {msg}");
        }
        let out = v["TranslatedText"]
            .as_str()
            .ok_or_else(|| anyhow!("Amazon 响应无 TranslatedText: {v}"))?
            .to_string();
        Ok(SingleResult {
            backend: self.name().to_string(),
            text: out,
            source_lang: v["SourceLanguageCode"].as_str().map(str::to_string),
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
        "zh-tw" | "zh-hant" => "zh-TW".into(),
        "en" | "en-us" | "en-gb" => "en".into(),
        "auto" => "auto".into(),
        other => other.to_string(),
    }
}
