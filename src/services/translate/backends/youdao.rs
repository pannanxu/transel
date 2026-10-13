//! 有道智云 AI 翻译（文本翻译 API v3）。
//!
//! 文档：https://ai.youdao.com/DOCSIRMA/html/%E6%96%87%E6%9C%AC%E7%BF%BB%E8%AF%91/API%E6%96%87%E6%9C%AC%E7%BF%BB%E8%AF%91%E6%8E%A5%E5%8F%A3/%E6%96%87%E6%9C%AC%E7%BF%BB%E8%AF%91%E6%8E%A5%E5%8F%A3.md
//!
//! 签名：truncate(q) + salt + curtime + key 拼起来 SHA256。

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::services::translate::{SingleResult, Translator};

pub struct YoudaoTranslator {
    app_id: String,
    key: String,
    agent: ureq::Agent,
}

impl YoudaoTranslator {
    pub fn new(app_id: String, key: String, agent: ureq::Agent) -> Self {
        Self { app_id, key, agent }
    }
}

impl Translator for YoudaoTranslator {
    fn name(&self) -> &str {
        "有道翻译"
    }

    fn translate_blocking(
        &self,
        text: &str,
        target: &str,
        _source_hint: Option<&str>,
    ) -> Result<SingleResult> {
        let (from, to) = youdao_lang_pair("auto", target);
        let salt = uuid::Uuid::new_v4().to_string();
        let curtime = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
            .to_string();
        let sign_src = format!(
            "{}{}{}{}{}",
            truncate(text, 10),
            salt,
            curtime,
            self.app_id,
            self.key
        );
        let mut h = Sha256::new();
        h.update(sign_src.as_bytes());
        let sign = format!("{:x}", h.finalize());

        let body = serde_json::json!({
            "q": text,
            "from": from,
            "to": to,
            "appKey": self.app_id,
            "salt": salt,
            "sign": sign,
            "signType": "v3",
            "curtime": curtime,
        });
        let v: Value = self
            .agent
            .post("https://openapi.youdao.com/api")
            .send_json(body)
            .context("请求有道翻译失败")?
            .body_mut()
            .read_json()
            .context("解析有道响应失败")?;
        if let Some(err) = v.get("errorCode").and_then(|e| e.as_str())
            && err != "0"
        {
            anyhow::bail!("有道错误 {err}");
        }
        let mut out = String::new();
        if let Some(arr) = v["translation"].as_array() {
            for s in arr {
                if let Some(t) = s.as_str() {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(t);
                }
            }
        }
        if out.is_empty() {
            anyhow::bail!("有道翻译结果为空");
        }
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

fn truncate(s: &str, max_chars: usize) -> String {
    let n = s.chars().count();
    if n <= max_chars {
        s.to_string()
    } else {
        let prefix: String = s.chars().take(max_chars).collect();
        let suffix: String = s.chars().skip(n - max_chars).collect();
        format!("{prefix}{}", suffix.len())
    }
}

fn youdao_lang_pair(from: &str, to: &str) -> (String, String) {
    let map = |c: &str| -> String {
        let c = c.to_ascii_lowercase();
        match c.as_str() {
            "zh" | "zh-cn" | "zh-hans" => "zh-CHS".into(),
            "zh-tw" | "zh-hant" => "zh-CHT".into(),
            "en" | "en-us" | "en-gb" => "en".into(),
            "ja" => "ja".into(),
            "ko" => "ko".into(),
            "fr" => "fr".into(),
            "de" => "de".into(),
            "es" => "es".into(),
            "ru" => "ru".into(),
            "auto" => "auto".into(),
            other => other.into(),
        }
    };
    (map(from), map(to))
}
