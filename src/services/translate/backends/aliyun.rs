//! 阿里云机器翻译 TranslateGeneral（通用版）。
//!
//! 文档：<https://help.aliyun.com/zh/machine-translation/developer-reference/api-alimt-2018-10-12-translategeneral>
//!
//! RPC：`POST https://mt.aliyuncs.com/`，`Action=TranslateGeneral`，`Version=2018-10-12`。
//! 必填：`FormatType=text`、`SourceLanguage`、`TargetLanguage`、`SourceText`、`Scene=general`。
//! 单次不超过 5000 字符。成功时 `Code` 为整数 `200`，译文在 `Data.Translated`。

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result};
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha1::Sha1;
use urlencoding::encode;

use crate::services::translate::{SingleResult, Translator};

type HmacSha1 = Hmac<Sha1>;

const ENDPOINT: &str = "https://mt.aliyuncs.com/";
const MAX_CHARS: usize = 5000;

pub struct AliyunTranslator {
    access_key_id: String,
    access_key_secret: String,
    agent: ureq::Agent,
}

impl AliyunTranslator {
    pub fn new(access_key_id: String, access_key_secret: String, agent: ureq::Agent) -> Self {
        Self {
            access_key_id,
            access_key_secret,
            agent,
        }
    }
}

impl Translator for AliyunTranslator {
    fn name(&self) -> &str {
        "阿里翻译"
    }

    fn translate_blocking(
        &self,
        text: &str,
        target: &str,
        source_hint: Option<&str>,
    ) -> Result<SingleResult> {
        let source = aliyun_lang(source_hint.unwrap_or("auto"));
        let target = aliyun_lang(target);
        if target == "auto" {
            anyhow::bail!("阿里翻译目标语言不能为 auto");
        }
        let mut out = String::new();
        let mut detected = None;
        for chunk in split_chars(text, MAX_CHARS) {
            let v = self.translate_chunk(&chunk, source, target)?;
            let code = json_code(&v);
            if code.as_deref() != Some("200") {
                let msg = v["Message"].as_str().unwrap_or("未知错误");
                anyhow::bail!("阿里翻译错误 {}: {msg}", code.unwrap_or_else(|| "?".into()));
            }
            if let Some(t) = v["Data"]["Translated"].as_str() {
                if !out.is_empty() && !out.ends_with('\n') && !t.is_empty() {
                    out.push('\n');
                }
                out.push_str(t);
            }
            if detected.is_none() {
                detected = v["Data"]["DetectedLanguage"].as_str().map(str::to_string);
            }
        }
        if out.is_empty() {
            anyhow::bail!("阿里响应无译文");
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

impl AliyunTranslator {
    fn translate_chunk(&self, text: &str, source: &str, target: &str) -> Result<Value> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let timestamp = format_iso8601(now as i64);
        let nonce = uuid::Uuid::new_v4().to_string();
        let mut params: Vec<(String, String)> = vec![
            ("AccessKeyId".into(), self.access_key_id.clone()),
            ("Action".into(), "TranslateGeneral".into()),
            ("Format".into(), "JSON".into()),
            ("FormatType".into(), "text".into()),
            ("Scene".into(), "general".into()),
            ("SignatureMethod".into(), "HMAC-SHA1".into()),
            ("SignatureNonce".into(), nonce),
            ("SignatureVersion".into(), "1.0".into()),
            ("SourceLanguage".into(), source.to_string()),
            ("SourceText".into(), text.to_string()),
            ("TargetLanguage".into(), target.to_string()),
            ("Timestamp".into(), timestamp),
            ("Version".into(), "2018-10-12".into()),
        ];
        params.sort_by(|a, b| a.0.cmp(&b.0));
        let canonical: String = params
            .iter()
            .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
            .collect::<Vec<_>>()
            .join("&");
        let string_to_sign = format!(
            "POST&{}&{}",
            percent_encode("/"),
            percent_encode(&canonical)
        );
        let key = format!("{}&", self.access_key_secret);
        let mut mac = HmacSha1::new_from_slice(key.as_bytes()).expect("HMAC");
        mac.update(string_to_sign.as_bytes());
        let sig = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            mac.finalize().into_bytes(),
        );
        params.push(("Signature".into(), sig));
        let form: Vec<(&str, &str)> = params
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        self.agent
            .post(ENDPOINT)
            .send_form(form)
            .context("请求阿里翻译失败")?
            .body_mut()
            .read_json()
            .context("解析阿里翻译响应失败")
    }
}

fn json_code(v: &Value) -> Option<String> {
    v.get("Code").and_then(|c| {
        c.as_i64()
            .map(|n| n.to_string())
            .or_else(|| c.as_str().map(str::to_string))
    })
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

fn aliyun_lang(code: &str) -> &'static str {
    match code.to_ascii_lowercase().as_str() {
        "auto" => "auto",
        "zh" | "zh-cn" | "zh-hans" => "zh",
        "zh-tw" | "zh-hant" => "zh-tw",
        "en" | "en-us" | "en-gb" => "en",
        "ja" | "ja-jp" => "ja",
        "ko" | "ko-kr" => "ko",
        "fr" | "fr-fr" => "fr",
        "de" | "de-de" => "de",
        "es" | "es-es" => "es",
        "ru" | "ru-ru" => "ru",
        "pt" | "pt-br" => "pt",
        "it" => "it",
        "vi" => "vi",
        "th" => "th",
        "id" => "id",
        "ar" => "ar",
        "tr" => "tr",
        "ms" => "ms",
        "hi" => "hi",
        _ => "auto",
    }
}

fn percent_encode(s: &str) -> String {
    encode(s)
        .replace('+', "%20")
        .replace('*', "%2A")
        .replace("%7E", "~")
}

fn format_iso8601(ts: i64) -> String {
    let days = ts / 86400;
    let rem = ts % 86400;
    let (y, m, d) = civil_from_days(days as i32);
    let hh = rem / 3600;
    let mm = (rem % 3600) / 60;
    let ss = rem % 60;
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

fn civil_from_days(z: i32) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i32 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
