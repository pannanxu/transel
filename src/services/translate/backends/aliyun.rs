//! 阿里云机器翻译（TranslateGeneral）。
//!
//! 参数：`access_key_id` + `key_ref`(secret)。

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result, anyhow};
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha1::Sha1;
use urlencoding::encode;

use crate::services::translate::{SingleResult, Translator};

type HmacSha1 = Hmac<Sha1>;

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
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let timestamp = format_iso8601(now as i64);
        let nonce = format!("{now}");
        let source = aliyun_lang(source_hint.unwrap_or("auto"));
        let target = aliyun_lang(target);

        let mut params: Vec<(String, String)> = vec![
            ("AccessKeyId".into(), self.access_key_id.clone()),
            ("Action".into(), "TranslateGeneral".into()),
            ("Format".into(), "JSON".into()),
            ("FormatType".into(), "text".into()),
            ("SignatureMethod".into(), "HMAC-SHA1".into()),
            ("SignatureNonce".into(), nonce),
            ("SignatureVersion".into(), "1.0".into()),
            ("SourceLanguage".into(), source.into()),
            ("SourceText".into(), text.to_string()),
            ("TargetLanguage".into(), target.into()),
            ("Timestamp".into(), timestamp),
            ("Version".into(), "2018-10-12".into()),
        ];
        params.sort_by(|a, b| a.0.cmp(&b.0));
        let canonical: String = params
            .iter()
            .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
            .collect::<Vec<_>>()
            .join("&");
        let string_to_sign = format!("GET&{}&{}", percent_encode("/"), percent_encode(&canonical));
        let key = format!("{}&", self.access_key_secret);
        let mut mac = HmacSha1::new_from_slice(key.as_bytes()).expect("HMAC");
        mac.update(string_to_sign.as_bytes());
        let sig = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            mac.finalize().into_bytes(),
        );

        let url = format!(
            "https://mt.aliyuncs.com/?{}&Signature={}",
            canonical,
            percent_encode(&sig)
        );
        let v: Value = self
            .agent
            .get(url)
            .call()
            .context("请求阿里翻译失败")?
            .body_mut()
            .read_json()
            .context("解析阿里翻译响应失败")?;
        if let Some(msg) = v["Message"].as_str().filter(|s| !s.is_empty()) {
            if v["Code"].as_str().unwrap_or("") != "200" && v["Data"].is_null() {
                anyhow::bail!("阿里翻译错误: {msg}");
            }
        }
        let text = v["Data"]["Translated"]
            .as_str()
            .ok_or_else(|| anyhow!("阿里响应无译文: {v}"))?
            .to_string();
        Ok(SingleResult {
            backend: self.name().to_string(),
            text,
            source_lang: v["Data"]["DetectedLanguage"].as_str().map(|s| s.to_string()),
            elapsed_ms: 0,
            ok: true,
            error: None,
            pending: false,
        })
    }
}

fn aliyun_lang(code: &str) -> &'static str {
    match code.to_ascii_lowercase().as_str() {
        "auto" => "auto",
        "zh" | "zh-cn" | "zh-hans" => "zh",
        "zh-tw" | "zh-hant" => "zh-tw",
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
