//! 有道智云文本翻译 API。
//!
//! 文档：https://ai.youdao.com/DOCSIRMA/html/trans/api/wbfy/
//!
//! `POST https://openapi.youdao.com/api`，表单字段。
//! 签名 v3：`sha256(应用ID + input + salt + curtime + 应用密钥)`。
//! `input`：`q` 长度 ≤20 为全文；否则 `前10字 + 总长度 + 后10字`。

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
        source_hint: Option<&str>,
    ) -> Result<SingleResult> {
        let from = youdao_lang(source_hint.unwrap_or("auto"));
        let to = youdao_lang(target);
        let salt = uuid::Uuid::new_v4().to_string();
        let curtime = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
            .to_string();
        let input = sign_input(text);
        let sign_src = format!("{}{}{}{}{}", self.app_id, input, salt, curtime, self.key);
        let mut h = Sha256::new();
        h.update(sign_src.as_bytes());
        let sign = format!("{:x}", h.finalize());

        let v: Value = self
            .agent
            .post("https://openapi.youdao.com/api")
            .send_form([
                ("q", text),
                ("from", from.as_str()),
                ("to", to.as_str()),
                ("appKey", self.app_id.as_str()),
                ("salt", salt.as_str()),
                ("sign", sign.as_str()),
                ("signType", "v3"),
                ("curtime", curtime.as_str()),
            ])
            .context("请求有道翻译失败")?
            .body_mut()
            .read_json()
            .context("解析有道响应失败")?;

        let err = error_code(&v);
        if err != "0" {
            anyhow::bail!("有道错误 {err}{}", youdao_error_hint(&err));
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
        let src = v["l"].as_str().and_then(|l| {
            l.split('2')
                .next()
                .filter(|s| !s.is_empty() && *s != "auto")
        });
        Ok(SingleResult {
            backend: self.name().to_string(),
            text: out,
            source_lang: src.map(|s| s.to_string()),
            elapsed_ms: 0,
            ok: true,
            error: None,
            pending: false,
        })
    }
}

/// 文档：q 长度 ≤20 用全文；否则 前10字符 + 字符数 + 后10字符。
fn sign_input(q: &str) -> String {
    let n = q.chars().count();
    if n <= 20 {
        q.to_string()
    } else {
        let prefix: String = q.chars().take(10).collect();
        let suffix: String = q.chars().skip(n - 10).collect();
        format!("{prefix}{n}{suffix}")
    }
}

fn error_code(v: &Value) -> String {
    v.get("errorCode")
        .and_then(|e| {
            e.as_str()
                .map(|s| s.to_string())
                .or_else(|| e.as_u64().map(|n| n.to_string()))
        })
        .unwrap_or_else(|| "unknown".into())
}

fn youdao_error_hint(code: &str) -> String {
    let hint = match code {
        "101" => "缺少必填参数",
        "102" => "不支持的语言类型",
        "108" => "应用 ID 无效",
        "110" => "应用未绑定文本翻译服务",
        "113" => "待译文本为空",
        "202" => "签名校验失败，请核对应用 ID / 密钥，并确保文本为 UTF-8",
        "206" => "时间戳无效",
        "207" => "重复请求",
        "401" => "账户欠费",
        "411" => "访问频率受限",
        _ => return String::new(),
    };
    format!("（{hint}）")
}

fn youdao_lang(code: &str) -> String {
    match code.to_ascii_lowercase().as_str() {
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
}

#[cfg(test)]
mod tests {
    use super::sign_input;

    #[test]
    fn sign_input_short() {
        assert_eq!(sign_input("hello"), "hello");
    }

    #[test]
    fn sign_input_long() {
        let q = "abcdefghijklmnopqrstuvwxyz"; // 26
        assert_eq!(sign_input(q), "abcdefghij26qrstuvwxyz");
    }
}
