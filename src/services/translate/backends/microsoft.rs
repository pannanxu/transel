//! Azure Translator 文本翻译 API `2026-06-06`。
//!
//! 文档：<https://learn.microsoft.com/zh-cn/azure/ai-services/translator/text-translation/2026-06-06/translate-api>
//!
//! 全球：`POST https://api.cognitive.microsofttranslator.com/translate?api-version=2026-06-06`
//! 自定义：`POST https://<资源>.cognitiveservices.azure.com/translator/text/translate?api-version=2026-06-06`
//!
//! 请求体 `{ inputs: [{ text, language?, targets: [{ language }] }] }`。
//! 省略 `language` 即自动检测源语。默认 NMT（`deploymentName` 缺省为 general）。

use anyhow::{Context as _, Result, anyhow};
use serde_json::{Map, Value, json};

use crate::services::translate::{SingleResult, Translator};

const API_VERSION: &str = "2026-06-06";
const GLOBAL_HOST: &str = "https://api.cognitive.microsofttranslator.com";

pub struct MicrosoftTranslator {
    key: String,
    region: String,
    endpoint: String,
    deployment: Option<String>,
    agent: ureq::Agent,
}

impl MicrosoftTranslator {
    pub fn new(
        key: String,
        region: String,
        endpoint: String,
        deployment: Option<String>,
        agent: ureq::Agent,
    ) -> Self {
        Self {
            key,
            region,
            endpoint: endpoint.trim().trim_end_matches('/').to_string(),
            deployment: deployment.and_then(|s| {
                let t = s.trim().to_string();
                if t.is_empty() { None } else { Some(t) }
            }),
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
        let mut target_obj = Map::new();
        target_obj.insert("language".into(), json!(ms_lang(target)));
        if let Some(dep) = &self.deployment {
            target_obj.insert("deploymentName".into(), json!(dep));
        }
        let mut input = Map::new();
        input.insert("text".into(), json!(text));
        if let Some(s) = source_hint.filter(|s| !s.eq_ignore_ascii_case("auto")) {
            input.insert("language".into(), json!(ms_lang(s)));
        }
        input.insert("targets".into(), json!([Value::Object(target_obj)]));
        let body = json!({ "inputs": [Value::Object(input)] });

        let url = translate_url(&self.endpoint);
        let mut req = self
            .agent
            .post(&url)
            .header("Ocp-Apim-Subscription-Key", &self.key)
            .header("Content-Type", "application/json; charset=UTF-8");
        if !self.region.trim().is_empty() {
            req = req.header("Ocp-Apim-Subscription-Region", &self.region);
        }
        let v: Value = req
            .send_json(body)
            .context("请求微软翻译失败")?
            .body_mut()
            .read_json()
            .context("解析微软翻译响应失败")?;

        if let Some(err) = v["error"].as_object() {
            let code = err
                .get("code")
                .map(|c| c.to_string())
                .unwrap_or_else(|| "?".into());
            let msg = err
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("未知错误");
            anyhow::bail!("微软翻译错误 {code}: {msg}");
        }

        let item = v["value"]
            .as_array()
            .and_then(|a| a.first())
            .or_else(|| v.as_array().and_then(|a| a.first()))
            .ok_or_else(|| anyhow!("微软响应为空: {v}"))?;
        let translations = item["translations"]
            .as_array()
            .ok_or_else(|| anyhow!("微软响应缺少 translations: {v}"))?;
        let mut out = String::new();
        for t in translations {
            if let Some(s) = t["text"].as_str() {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(s);
            }
        }
        if out.is_empty() {
            anyhow::bail!("微软翻译结果为空");
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

fn translate_url(endpoint: &str) -> String {
    let e = endpoint.trim().trim_end_matches('/');
    let path = if e.is_empty() || e.contains("microsofttranslator.com") {
        format!("{GLOBAL_HOST}/translate")
    } else if e.contains("/translator/text/translate") {
        e.to_string()
    } else if e.contains("cognitiveservices.azure.com") {
        format!("{e}/translator/text/translate")
    } else {
        format!("{e}/translate")
    };
    if path.contains("api-version=") {
        path
    } else {
        format!("{path}?api-version={API_VERSION}")
    }
}

fn ms_lang(code: &str) -> String {
    match code.to_ascii_lowercase().as_str() {
        "zh" | "zh-cn" | "zh-hans" => "zh-Hans".into(),
        "zh-tw" | "zh-hant" => "zh-Hant".into(),
        other => other.to_string(),
    }
}
