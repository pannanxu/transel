//! 火山引擎机器翻译（TranslateText）。
//!
//! 参数：`access_key` + `key_ref`(secret) ；可选 `region`（默认 cn-north-1）。

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result, anyhow};
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::services::translate::{SingleResult, Translator};

type HmacSha256 = Hmac<Sha256>;

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
            region,
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
        let host = "translate.volcengineapi.com";
        let action = "TranslateText";
        let version = "2020-06-01";
        let service = "translate";
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let x_date = format_x_date(now);
        let short_date = &x_date[..8];

        let mut payload = json!({
            "TargetLanguage": volcano_lang(target),
            "TextList": [text],
        });
        if let Some(s) = source_hint.filter(|s| *s != "auto") {
            payload["SourceLanguage"] = json!(volcano_lang(s));
        }
        let body = serde_json::to_string(&payload)?;

        let content_type = "application/json; charset=utf-8";
        let x_content_sha256 = hex_lower(&Sha256::digest(body.as_bytes()));
        let canonical_headers = format!(
            "content-type:{content_type}\nhost:{host}\nx-content-sha256:{x_content_sha256}\nx-date:{x_date}\n"
        );
        let signed_headers = "content-type;host;x-content-sha256;x-date";
        let canonical_request = format!(
            "POST\n/\nAction={action}&Version={version}\n{canonical_headers}\n{signed_headers}\n{x_content_sha256}"
        );
        let credential_scope = format!("{short_date}/{}/{service}/request", self.region);
        let string_to_sign = format!(
            "HMAC-SHA256\n{x_date}\n{credential_scope}\n{}",
            hex_lower(&Sha256::digest(canonical_request.as_bytes()))
        );
        let k_date = hmac_sha256(self.secret_key.as_bytes(), short_date.as_bytes());
        let k_region = hmac_sha256(&k_date, self.region.as_bytes());
        let k_service = hmac_sha256(&k_region, service.as_bytes());
        let k_signing = hmac_sha256(&k_service, b"request");
        let signature = hex_lower(&hmac_sha256(&k_signing, string_to_sign.as_bytes()));
        let authorization = format!(
            "HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
            self.access_key, credential_scope, signed_headers, signature
        );

        let url = format!("https://{host}/?Action={action}&Version={version}");
        let v: Value = self
            .agent
            .post(url)
            .header("Content-Type", content_type)
            .header("Host", host)
            .header("X-Date", &x_date)
            .header("X-Content-Sha256", &x_content_sha256)
            .header("Authorization", authorization)
            .send(body.as_bytes())
            .context("请求火山翻译失败")?
            .body_mut()
            .read_json()
            .context("解析火山翻译响应失败")?;

        if let Some(msg) = v["ResponseMetadata"]["Error"]["Message"].as_str() {
            anyhow::bail!("火山翻译错误: {msg}");
        }
        let text = v["TranslationList"][0]["Translation"]
            .as_str()
            .ok_or_else(|| anyhow!("火山响应无译文: {v}"))?
            .to_string();
        let src = v["TranslationList"][0]["DetectedSourceLanguage"]
            .as_str()
            .map(|s| s.to_string());
        Ok(SingleResult {
            backend: self.name().to_string(),
            text,
            source_lang: src,
            elapsed_ms: 0,
            ok: true,
            error: None,
            pending: false,
        })
    }
}

fn volcano_lang(code: &str) -> String {
    match code.to_ascii_lowercase().as_str() {
        "zh" | "zh-cn" | "zh-hans" => "zh".into(),
        "zh-tw" | "zh-hant" => "zh-Hant".into(),
        "en" | "en-us" | "en-gb" => "en".into(),
        "ja" => "ja".into(),
        "ko" => "ko".into(),
        "fr" => "fr".into(),
        "de" => "de".into(),
        "es" => "es".into(),
        "ru" => "ru".into(),
        other => other.to_string(),
    }
}

fn format_x_date(ts: i64) -> String {
    // YYYYMMDD'T'HHMMSS'Z'
    let days = ts / 86400;
    let rem = ts % 86400;
    let (y, m, d) = civil_from_days(days as i32);
    let hh = rem / 3600;
    let mm = (rem % 3600) / 60;
    let ss = rem % 60;
    format!("{y:04}{m:02}{d:02}T{hh:02}{mm:02}{ss:02}Z")
}

fn civil_from_days(z: i32) -> (i32, u32, u32) {
    // Howard Hinnant civil_from_days
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

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC key");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
