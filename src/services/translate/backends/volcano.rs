//! 火山引擎机器翻译 TranslateText。
//!
//! 文档：<https://docs.volcengine.com/docs/MachineTranslation/TextTranslationAPI?lang=zh>
//! （与 <https://www.volcengine.com/docs/4640/65067> 一致）
//!
//! - Host `translate.volcengineapi.com`
//! - `Action=TranslateText` `Version=2020-06-01` `Service=translate` `Region=cn-north-1`
//! - 请求：`TargetLanguage` 必填；`SourceLanguage` 省略即自动检测；`TextList` 最多 16 条、合计 ≤5000 字
//! - 响应：`TranslationList[].Translation`；未指定源语时有 `DetectedSourceLanguage`
//!
//! 参数：`access_key` + `key_ref`(secret)；可选 `region`（默认 cn-north-1）。

use anyhow::{Result, anyhow};
use serde_json::{Value, json};

use crate::services::cloud;
use crate::services::translate::{SingleResult, Translator};

const HOST: &str = "translate.volcengineapi.com";
const SERVICE: &str = "translate";
const ACTION: &str = "TranslateText";
const VERSION: &str = "2020-06-01";
const DEFAULT_REGION: &str = "cn-north-1";
/// 文档：列表长度不超过 16，总文本长度不超过 5000 字符。
const MAX_CHARS: usize = 5000;
const MAX_ITEMS: usize = 16;

pub struct VolcanoTranslator {
    access_key: String,
    secret_key: String,
    region: String,
    agent: ureq::Agent,
}

impl VolcanoTranslator {
    pub fn new(access_key: String, secret_key: String, region: String, agent: ureq::Agent) -> Self {
        Self {
            access_key,
            secret_key,
            region: if region.trim().is_empty() {
                DEFAULT_REGION.into()
            } else {
                region
            },
            agent,
        }
    }
}

impl Translator for VolcanoTranslator {
    fn name(&self) -> &str {
        "火山翻译"
    }

    fn translate_blocking(
        &self,
        text: &str,
        target: &str,
        source_hint: Option<&str>,
    ) -> Result<SingleResult> {
        let target_lang = volcano_lang(target);
        let source_lang = source_hint
            .filter(|s| !s.eq_ignore_ascii_case("auto"))
            .map(volcano_lang);
        let batches = batch_text(text, MAX_CHARS, MAX_ITEMS);
        let mut out = String::new();
        let mut src = None;
        for batch in batches {
            let mut payload = json!({
                "TargetLanguage": target_lang,
                "TextList": batch,
            });
            if let Some(s) = source_lang.as_ref() {
                payload["SourceLanguage"] = json!(s);
            }
            let v: Value = cloud::volcano_json(
                &self.agent,
                &self.access_key,
                &self.secret_key,
                &self.region,
                HOST,
                SERVICE,
                ACTION,
                VERSION,
                &payload.to_string(),
            )?;
            if let Some(err) = v["ResponseMetadata"]["Error"].as_object() {
                let code = err
                    .get("Code")
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "?".into());
                let msg = err
                    .get("Message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("未知错误");
                anyhow::bail!("火山翻译错误 {code}: {msg}");
            }
            let list = v["TranslationList"]
                .as_array()
                .ok_or_else(|| anyhow!("火山响应无 TranslationList: {v}"))?;
            for item in list {
                if let Some(t) = item["Translation"].as_str() {
                    if !out.is_empty() && !out.ends_with('\n') && !t.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(t);
                }
                if src.is_none() {
                    src = item["DetectedSourceLanguage"]
                        .as_str()
                        .map(str::to_string)
                        .or_else(|| source_lang.clone());
                }
            }
        }
        if out.is_empty() {
            anyhow::bail!("火山响应无译文");
        }
        Ok(SingleResult {
            backend: self.name().to_string(),
            text: out,
            source_lang: src,
            elapsed_ms: 0,
            ok: true,
            error: None,
            pending: false,
        })
    }
}

fn batch_text(text: &str, max_chars: usize, max_items: usize) -> Vec<Vec<String>> {
    let pieces = split_chars(text, max_chars);
    pieces.chunks(max_items).map(|c| c.to_vec()).collect()
}

fn split_chars(text: &str, max_chars: usize) -> Vec<String> {
    if text.chars().count() <= max_chars {
        return vec![text.to_string()];
    }
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut n = 0usize;
    for ch in text.chars() {
        if n >= max_chars {
            out.push(std::mem::take(&mut buf));
            n = 0;
        }
        buf.push(ch);
        n += 1;
    }
    if !buf.is_empty() {
        out.push(buf);
    }
    out
}

/// 火山语言代码（见文档语言支持列表）。
fn volcano_lang(code: &str) -> String {
    match code.to_ascii_lowercase().as_str() {
        "zh" | "zh-cn" | "zh-hans" => "zh".into(),
        "zh-tw" | "zh-hant" => "zh-Hant".into(),
        "en" | "en-us" | "en-gb" => "en".into(),
        other => other.to_string(),
    }
}
