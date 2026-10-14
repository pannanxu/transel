//! 百度翻译开放平台 · 通用文本翻译。
//!
//! 文档：<https://fanyi-api.baidu.com/product/113>
//!
//! `POST https://fanyi-api.baidu.com/api/trans/vip/translate`
//! `Content-Type: application/x-www-form-urlencoded`
//! 必填：`q` / `from` / `to` / `appid` / `salt` / `sign`
//! `from` 可为 `auto`，`to` 不可为 `auto`。
//! `sign` = MD5(`appid` + `q` + `salt` + 密钥) 的 32 位小写；签名时 `q` 不做 URL 编码。
//! 建议单次 ≤2000 字符，认证后上限 6000 字符。

use anyhow::{Context as _, Result, anyhow};
use md5::Context;
use serde_json::Value;

use crate::services::translate::{SingleResult, Translator};

const ENDPOINT: &str = "https://fanyi-api.baidu.com/api/trans/vip/translate";
/// 文档建议单次 2000 字符以内；上限 6000。
const MAX_CHARS: usize = 2000;

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
        source_hint: Option<&str>,
    ) -> Result<SingleResult> {
        let to = baidu_lang(target);
        if to == "auto" {
            anyhow::bail!("百度翻译目标语言不能为 auto");
        }
        let mut out = String::new();
        let mut detected = None;
        for chunk in split_chars(text, MAX_CHARS) {
            let v = self.translate_chunk(&chunk, source_hint, &to)?;
            if let Some(code) = json_code(&v, "error_code")
                && code != "52000"
            {
                let msg = v.get("error_msg").and_then(|m| m.as_str()).unwrap_or("?");
                anyhow::bail!("百度错误 {code}: {msg}{}", baidu_error_hint(&code));
            }
            let trans = v["trans_result"]
                .as_array()
                .ok_or_else(|| anyhow!("百度响应缺少 trans_result: {v}"))?;
            for s in trans {
                if let Some(d) = s["dst"].as_str() {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(d);
                }
            }
            if detected.is_none() {
                detected = v["from"].as_str().map(str::to_string);
            }
        }
        if out.is_empty() {
            anyhow::bail!("百度翻译结果为空");
        }
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

impl BaiduTranslator {
    fn translate_chunk(&self, text: &str, source_hint: Option<&str>, to: &str) -> Result<Value> {
        let from = baidu_lang(source_hint.unwrap_or("auto"));
        let salt = uuid::Uuid::new_v4().simple().to_string();
        let sign_src = format!("{}{}{}{}", self.app_id, text, salt, self.key);
        let mut hasher = Context::new();
        hasher.consume(sign_src.as_bytes());
        let sign = format!("{:x}", hasher.compute());
        self.agent
            .post(ENDPOINT)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .send_form([
                ("q", text),
                ("from", from.as_str()),
                ("to", to),
                ("appid", self.app_id.as_str()),
                ("salt", salt.as_str()),
                ("sign", sign.as_str()),
            ])
            .context("请求百度翻译失败")?
            .body_mut()
            .read_json()
            .context("解析百度翻译响应失败")
    }
}

fn json_code(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(|e| {
        e.as_str()
            .map(str::to_string)
            .or_else(|| e.as_i64().map(|n| n.to_string()))
    })
}

fn baidu_error_hint(code: &str) -> String {
    let hint = match code {
        "52001" => "请求超时，请缩短文本或核对语种",
        "52002" => "系统错误，请重试",
        "52003" => "未授权，请核对 APP ID 并开通通用翻译",
        "54000" => "必填参数为空",
        "54001" => "签名错误，请核对 APP ID / 密钥",
        "54003" => "访问频率受限",
        "54004" => "账户余额不足",
        "54005" => "长文本请求过频，请稍后再试",
        "58000" => "客户端 IP 非法",
        "58001" => "不支持该语向（标准版仅常见语种）",
        "58002" => "服务已关闭，请在控制台开启",
        "58003" => "此 IP 已被封禁",
        "90107" => "认证未通过或未生效",
        "20003" => "请求内容存在安全风险",
        _ => return String::new(),
    };
    format!("（{hint}）")
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

/// 常见语种代码（文档 §5）。
fn baidu_lang(code: &str) -> String {
    match code.to_ascii_lowercase().as_str() {
        "auto" => "auto".into(),
        "zh" | "zh-cn" | "zh-hans" => "zh".into(),
        "zh-tw" | "zh-hant" | "cht" => "cht".into(),
        "yue" | "zh-hk" | "zh-yue" => "yue".into(),
        "en" | "en-us" | "en-gb" => "en".into(),
        "ja" | "jp" | "ja-jp" => "jp".into(),
        "ko" | "kor" | "ko-kr" => "kor".into(),
        "fr" | "fra" | "fr-fr" => "fra".into(),
        "es" | "spa" | "es-es" => "spa".into(),
        "ru" | "ru-ru" => "ru".into(),
        "de" | "de-de" => "de".into(),
        "pt" | "pt-br" => "pt".into(),
        "it" => "it".into(),
        "th" => "th".into(),
        "ar" | "ara" => "ara".into(),
        "vi" | "vie" => "vie".into(),
        "id" => "id".into(),
        "hi" => "hi".into(),
        "tr" => "tr".into(),
        "nl" => "nl".into(),
        "pl" => "pl".into(),
        other => other.into(),
    }
}
