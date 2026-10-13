//! 腾讯云机器翻译（TMT）v3，TC3-HMAC-SHA256 签名。

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result, anyhow};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::services::translate::{SingleResult, Translator};

pub struct TencentTranslator {
    secret_id: String,
    secret_key: String,
    region: String,
    agent: ureq::Agent,
}

impl TencentTranslator {
    pub fn new(secret_id: String, secret_key: String, region: String, agent: ureq::Agent) -> Self {
        Self {
            secret_id,
            secret_key,
            region,
            agent,
        }
    }

    fn sign(&self, payload: &str, ts: i64) -> String {
        // 文档：https://cloud.tencent.com/document/api/551/16619
        let service = "tmt";
        let host = "tmt.tencentcloudapi.com";
        let algorithm = "TC3-HMAC-SHA256";
        let date = format_utc_date(ts);

        // 1) canonical request
        let http_request_method = "POST";
        let canonical_uri = "/";
        let canonical_querystring = "";
        let canonical_headers = format!(
            "content-type:application/json; charset=utf-8\nhost:{host}\nx-tc-action:texttranslate\n"
        );
        let signed_headers = "content-type;host;x-tc-action";
        let hashed_request_payload = sha256_hex(payload.as_bytes());
        let canonical_request = format!(
            "{http_request_method}\n{canonical_uri}\n{canonical_querystring}\n{canonical_headers}\n{signed_headers}\n{hashed_request_payload}"
        );

        // 2) string to sign
        let credential_scope = format!("{date}/{service}/tc3_request");
        let sts = format!(
            "{algorithm}\n{ts}\n{credential_scope}\n{}",
            sha256_hex(canonical_request.as_bytes())
        );

        // 3) signature
        let secret_date = hmac_sha256(
            format!("TC3{}", self.secret_key).as_bytes(),
            date.as_bytes(),
        );
        let secret_service = hmac_sha256(&secret_date, service.as_bytes());
        let secret_signing = hmac_sha256(&secret_service, b"tc3_request");
        let sig = hex_lower(&hmac_sha256(&secret_signing, sts.as_bytes()));

        // 4) authorization header
        format!(
            "{algorithm} Credential={secret_id}/{credential_scope}, SignedHeaders={signed_headers}, Signature={sig}",
            secret_id = self.secret_id
        )
    }
}

impl Translator for TencentTranslator {
    fn name(&self) -> &str {
        "腾讯翻译"
    }

    fn translate_blocking(
        &self,
        text: &str,
        target: &str,
        source_hint: Option<&str>,
    ) -> Result<SingleResult> {
        let (source, target) = tencent_lang_pair(source_hint.unwrap_or("auto"), target);
        let payload = serde_json::json!({
            "SourceText": text,
            "Source": source,
            "Target": target,
            "ProjectId": 0,
        });
        let body = serde_json::to_string(&payload)?;
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let auth = self.sign(&body, ts);
        let host = "tmt.tencentcloudapi.com";
        let v: Value = self
            .agent
            .post(format!("https://{host}"))
            .header("Authorization", auth)
            .header("Content-Type", "application/json; charset=utf-8")
            .header("Host", host)
            .header("X-TC-Action", "TextTranslate")
            .header("X-TC-Version", "2018-03-21")
            .header("X-TC-Timestamp", ts.to_string())
            .header("X-TC-Region", &self.region)
            .send(body.as_bytes())
            .context("请求腾讯翻译失败")?
            .body_mut()
            .read_json()
            .context("解析腾讯翻译响应失败")?;
        if let Some(err) = v.get("Error") {
            anyhow::bail!("腾讯错误: {err}");
        }
        let text = v["Response"]["TargetText"]
            .as_str()
            .ok_or_else(|| anyhow!("腾讯响应缺少 TargetText"))?
            .to_string();
        let src = v["Response"]["Source"].as_str().map(str::to_string);
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

fn format_utc_date(ts: i64) -> String {
    use chrono::{TimeZone, Utc};
    Utc.timestamp_opt(ts, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex_lower(&h.finalize())
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    let mut m = <Hmac<Sha256> as Mac>::new_from_slice(key).expect("hmac key");
    m.update(data);
    m.finalize().into_bytes().to_vec()
}

fn hex_lower(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        s.push_str(&format!("{:02x}", x));
    }
    s
}

fn tencent_lang_pair(from: &str, to: &str) -> (String, String) {
    let map = |c: &str| -> String {
        let c = c.to_ascii_lowercase();
        match c.as_str() {
            "zh" | "zh-cn" | "zh-hans" => "zh".into(),
            "zh-tw" | "zh-hant" => "zh-TW".into(),
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
