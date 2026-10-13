//! 百度翻译开放平台（通用文本翻译 API）。
//!
//! 文档：https://api.fanyi.baidu.com/doc/21
//! 签名：sign = MD5(appid+q+salt+密钥)
//! appid + 密钥均需配置；密钥走钥匙串。

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result, anyhow};
use md5::{Context, Digest};
use serde_json::Value;

use crate::services::translate::{SingleResult, Translator, ureq_agent};

pub struct BaiduTranslator {
    app_id: String,
    key: String,
    agent: ureq::Agent,
}

impl BaiduTranslator {
    pub fn new(app_id: String, key: String, agent: ureq::Agent) -> Self {
        Self { app_id, key, agent }
    }
}

impl Translator for BaiduTranslator {
    fn name(&self) -> &str {
        "百度翻译"
    }

    fn translate_blocking(
        &self,
        text: &str,
        target: &str,
        _source_hint: Option<&str>,
    ) -> Result<SingleResult> {
        let (from, to) = baidu_lang_pair("auto", target);
        let salt = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
            .to_string();
        let sign_src = format!("{}{}{}{}", self.app_id, text, salt, self.key);
        let mut hasher = Context::new();
        hasher.consume(sign_src.as_bytes());
        let sign = format!("{:x}", hasher.compute());

        // POST 表单：长文本放 body，避免 GET query 触发 URI TooLong panic。
        let v: Value = self
            .agent
            .post("https://fanyi-api.baidu.com/api/trans/vip/translate")
            .send_form([
                ("q", text),
                ("from", from.as_str()),
                ("to", to.as_str()),
                ("appid", self.app_id.as_str()),
                ("salt", salt.as_str()),
                ("sign", sign.as_str()),
            ])
            .context("请求百度翻译失败")?
            .body_mut()
            .read_json()
            .context("解析百度翻译响应失败")?;
        if let Some(err) = v.get("error_code").and_then(|e| e.as_str()) {
            let msg = v.get("error_msg").and_then(|m| m.as_str()).unwrap_or("?");
            anyhow::bail!("百度错误 {err}: {msg}");
        }
        let trans = v["trans_result"]
            .as_array()
            .ok_or_else(|| anyhow!("百度响应缺少 trans_result"))?;
        let mut out = String::new();
        for s in trans {
            if let Some(d) = s["dst"].as_str() {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(d);
            }
        }
        if out.is_empty() {
            anyhow::bail!("百度翻译结果为空");
        }
        let from = v["from"].as_str().map(str::to_string);
        Ok(SingleResult {
            backend: self.name().to_string(),
            text: out,
            source_lang: from,
            elapsed_ms: 0,
            ok: true,
            error: None,
            pending: false,
        })
    }
}

fn baidu_lang_pair(from: &str, to: &str) -> (String, String) {
    let map = |c: &str| -> String {
        let c = c.to_ascii_lowercase();
        match c.as_str() {
            "zh" | "zh-cn" | "zh-hans" => "zh".into(),
            "zh-tw" | "zh-hant" => "cht".into(),
            "en" | "en-us" | "en-gb" => "en".into(),
            "ja" => "jp".into(),
            "ko" => "kor".into(),
            "fr" => "fra".into(),
            "de" => "de".into(),
            "es" => "spa".into(),
            "ru" => "ru".into(),
            "auto" => "auto".into(),
            other => other.into(),
        }
    };
    (map(from), map(to))
}
