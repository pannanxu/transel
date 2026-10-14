//! 云厂商签名请求（腾讯云 TC3 / 火山引擎 / AWS SigV4）。

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result};
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

pub fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC key");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex_lower(&Sha256::digest(data))
}

pub fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn utc_ymd(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "19700101".into())
}

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 腾讯云 TC3：`POST https://{host}/` JSON。
#[allow(clippy::too_many_arguments)]
pub fn tencent_json(
    agent: &ureq::Agent,
    secret_id: &str,
    secret_key: &str,
    region: &str,
    host: &str,
    service: &str,
    action: &str,
    version: &str,
    payload: &str,
) -> Result<Value> {
    let ts = now_unix();
    let date = utc_ymd(ts);
    let algorithm = "TC3-HMAC-SHA256";
    let hashed = sha256_hex(payload.as_bytes());
    let canonical_headers = format!(
        "content-type:application/json; charset=utf-8\nhost:{host}\nx-tc-action:{}\n",
        action.to_ascii_lowercase()
    );
    let signed_headers = "content-type;host;x-tc-action";
    let canonical_request = format!("POST\n/\n\n{canonical_headers}\n{signed_headers}\n{hashed}");
    let credential_scope = format!("{date}/{service}/tc3_request");
    let sts = format!(
        "{algorithm}\n{ts}\n{credential_scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );
    let secret_date = hmac_sha256(format!("TC3{secret_key}").as_bytes(), date.as_bytes());
    let secret_service = hmac_sha256(&secret_date, service.as_bytes());
    let secret_signing = hmac_sha256(&secret_service, b"tc3_request");
    let sig = hex_lower(&hmac_sha256(&secret_signing, sts.as_bytes()));
    let auth = format!(
        "{algorithm} Credential={secret_id}/{credential_scope}, SignedHeaders={signed_headers}, Signature={sig}"
    );
    agent
        .post(format!("https://{host}"))
        .header("Authorization", auth)
        .header("Content-Type", "application/json; charset=utf-8")
        .header("Host", host)
        .header("X-TC-Action", action)
        .header("X-TC-Version", version)
        .header("X-TC-Timestamp", ts.to_string())
        .header("X-TC-Region", region)
        .send(payload.as_bytes())
        .with_context(|| format!("请求腾讯云 {action} 失败"))?
        .body_mut()
        .read_json()
        .with_context(|| format!("解析腾讯云 {action} 失败"))
}

fn amz_date(ts: i64) -> (String, String) {
    let dt = chrono::DateTime::from_timestamp(ts, 0)
        .unwrap_or_else(|| chrono::DateTime::from_timestamp(0, 0).unwrap());
    (
        dt.format("%Y%m%dT%H%M%SZ").to_string(),
        dt.format("%Y%m%d").to_string(),
    )
}

/// AWS SigV4 JSON（Amazon Translate 等）。
#[allow(clippy::too_many_arguments)]
pub fn aws_json(
    agent: &ureq::Agent,
    access_key: &str,
    secret_key: &str,
    region: &str,
    host: &str,
    service: &str,
    amz_target: &str,
    payload: &str,
) -> Result<Value> {
    let ts = now_unix();
    let (amz, short) = amz_date(ts);
    let hashed = sha256_hex(payload.as_bytes());
    let canonical_headers = format!(
        "content-type:application/x-amz-json-1.1\nhost:{host}\nx-amz-date:{amz}\nx-amz-target:{amz_target}\n"
    );
    let signed_headers = "content-type;host;x-amz-date;x-amz-target";
    let canonical_request = format!("POST\n/\n\n{canonical_headers}\n{signed_headers}\n{hashed}");
    let scope = format!("{short}/{region}/{service}/aws4_request");
    let sts = format!(
        "AWS4-HMAC-SHA256\n{amz}\n{scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );
    let k_date = hmac_sha256(format!("AWS4{secret_key}").as_bytes(), short.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    let k_signing = hmac_sha256(&k_service, b"aws4_request");
    let sig = hex_lower(&hmac_sha256(&k_signing, sts.as_bytes()));
    let auth = format!(
        "AWS4-HMAC-SHA256 Credential={access_key}/{scope}, SignedHeaders={signed_headers}, Signature={sig}"
    );
    agent
        .post(format!("https://{host}/"))
        .header("Authorization", auth)
        .header("Content-Type", "application/x-amz-json-1.1")
        .header("Host", host)
        .header("X-Amz-Date", amz)
        .header("X-Amz-Target", amz_target)
        .send(payload.as_bytes())
        .context("请求 AWS 失败")?
        .body_mut()
        .read_json()
        .context("解析 AWS 响应失败")
}

/// 火山引擎 OpenAPI（Translate / Visual / TTS）。
#[allow(clippy::too_many_arguments)]
pub fn volcano_json(
    agent: &ureq::Agent,
    access_key: &str,
    secret_key: &str,
    region: &str,
    host: &str,
    service: &str,
    action: &str,
    version: &str,
    payload: &str,
) -> Result<Value> {
    let ts = now_unix();
    let (x_date, short_date) = amz_date(ts);
    let content_type = "application/json; charset=utf-8";
    let x_content_sha256 = sha256_hex(payload.as_bytes());
    let canonical_headers = format!(
        "content-type:{content_type}\nhost:{host}\nx-content-sha256:{x_content_sha256}\nx-date:{x_date}\n"
    );
    let signed_headers = "content-type;host;x-content-sha256;x-date";
    let canonical_request = format!(
        "POST\n/\nAction={action}&Version={version}\n{canonical_headers}\n{signed_headers}\n{x_content_sha256}"
    );
    let credential_scope = format!("{short_date}/{region}/{service}/request");
    let string_to_sign = format!(
        "HMAC-SHA256\n{x_date}\n{credential_scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );
    let k_date = hmac_sha256(secret_key.as_bytes(), short_date.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    let k_signing = hmac_sha256(&k_service, b"request");
    let signature = hex_lower(&hmac_sha256(&k_signing, string_to_sign.as_bytes()));
    let authorization = format!(
        "HMAC-SHA256 Credential={access_key}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}"
    );
    let url = format!("https://{host}/?Action={action}&Version={version}");
    agent
        .post(url)
        .header("Content-Type", content_type)
        .header("Host", host)
        .header("X-Date", &x_date)
        .header("X-Content-Sha256", &x_content_sha256)
        .header("Authorization", authorization)
        .send(payload.as_bytes())
        .with_context(|| format!("请求火山 {action} 失败"))?
        .body_mut()
        .read_json()
        .with_context(|| format!("解析火山 {action} 失败"))
}
