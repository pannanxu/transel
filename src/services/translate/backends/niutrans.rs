//! 小牛翻译云平台文本 API v2。
//!
//! - Flash：<https://niutrans.com/documents/contents/transapi_text_v2>
//!   `POST https://api.niutrans.com/v2/text/translate`
//!   参数 `from` / `to` / `appId` / `srcText` / `timestamp` / `authStr`。
//!   `authStr` = MD5(按参数名 ASCII 排序的 `k=v` 用 `&` 拼接，含 apikey，不含空字段和 authStr)。
//! - Pro：<https://niutrans.com/documents/contents/transapi_text_v2_pro#accessMode>
//!   `POST https://api.niutrans.com/v2/text/translate/llm`
//!   参数 `from` / `to` / `apikey` / `srcText`（JSON）。

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result, anyhow};
use md5::Context;
use serde_json::{Map, Value, json};

use crate::services::translate::{SingleResult, Translator};

const FLASH_URL: &str = "https://api.niutrans.com/v2/text/translate";
const PRO_URL: &str = "https://api.niutrans.com/v2/text/translate/llm";

#[derive(Clone, Copy)]
pub enum NiutransEdition {
    Flash,
    Pro,
}

pub struct NiutransTranslator {
    name: String,
    edition: NiutransEdition,
    app_id: String,
    key: String,
    agent: ureq::Agent,
}

impl NiutransTranslator {
    pub fn flash(app_id: String, key: String, agent: ureq::Agent) -> Self {
        Self {
            name: "小牛翻译 Flash".into(),
            edition: NiutransEdition::Flash,
            app_id,
            key,
            agent,
        }
    }

    pub fn pro(key: String, agent: ureq::Agent) -> Self {
        Self {
            name: "小牛翻译 Pro".into(),
            edition: NiutransEdition::Pro,
            app_id: String::new(),
            key,
            agent,
        }
    }
}

impl Translator for NiutransTranslator {
    fn name(&self) -> &str {
        &self.name
    }

    fn translate_blocking(
        &self,
        text: &str,
        target: &str,
        source_hint: Option<&str>,
    ) -> Result<SingleResult> {
        let from = map_lang(source_hint.unwrap_or("auto"));
        let to = map_lang(target);
        let v = match self.edition {
            NiutransEdition::Flash => self.post_flash(text, &from, &to)?,
            NiutransEdition::Pro => self.post_pro(text, &from, &to)?,
        };
        if let Some(code) = error_code(&v) {
            let msg = v["errorMsg"]
                .as_str()
                .or_else(|| v["error_msg"].as_str())
                .unwrap_or("?");
            anyhow::bail!("小牛翻译错误 {code}: {msg}");
        }
        let out = v["tgtText"]
            .as_str()
            .or_else(|| v["tgt_text"].as_str())
            .ok_or_else(|| anyhow!("小牛响应无 tgtText: {v}"))?
            .to_string();
        Ok(SingleResult {
            backend: self.name().to_string(),
            text: out,
            source_lang: source_hint.map(str::to_string),
            elapsed_ms: 0,
            ok: true,
            error: None,
            pending: false,
        })
    }
}

impl NiutransTranslator {
    fn post_flash(&self, text: &str, from: &str, to: &str) -> Result<Value> {
        if self.app_id.trim().is_empty() {
            anyhow::bail!("小牛 Flash 需要填写 App ID");
        }
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
            .to_string();
        let mut sign_params = BTreeMap::new();
        sign_params.insert("from".into(), from.to_string());
        sign_params.insert("to".into(), to.to_string());
        sign_params.insert("appId".into(), self.app_id.clone());
        sign_params.insert("srcText".into(), text.to_string());
        sign_params.insert("timestamp".into(), ts.clone());
        sign_params.insert("apikey".into(), self.key.clone());
        let auth = auth_str(&sign_params);
        let body = json!({
            "from": from,
            "to": to,
            "appId": self.app_id,
            "srcText": text,
            "timestamp": ts,
            "authStr": auth,
        });
        self.agent
            .post(FLASH_URL)
            .header("Content-Type", "application/json")
            .send_json(body)
            .context("请求小牛翻译 Flash 失败")?
            .body_mut()
            .read_json()
            .context("解析小牛翻译 Flash 失败")
    }

    fn post_pro(&self, text: &str, from: &str, to: &str) -> Result<Value> {
        let mut body = Map::new();
        body.insert("from".into(), json!(from));
        body.insert("to".into(), json!(to));
        body.insert("apikey".into(), json!(self.key));
        body.insert("srcText".into(), json!(text));
        self.agent
            .post(PRO_URL)
            .header("Content-Type", "application/json")
            .send_json(Value::Object(body))
            .context("请求小牛翻译 Pro 失败")?
            .body_mut()
            .read_json()
            .context("解析小牛翻译 Pro 失败")
    }
}

fn auth_str(params: &BTreeMap<String, String>) -> String {
    let joined = params
        .iter()
        .filter(|(k, v)| k.as_str() != "authStr" && !v.is_empty())
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    let mut hasher = Context::new();
    hasher.consume(joined.as_bytes());
    format!("{:x}", hasher.compute())
}

fn error_code(v: &Value) -> Option<String> {
    v.get("errorCode")
        .or_else(|| v.get("error_code"))
        .and_then(|c| {
            c.as_str()
                .map(str::to_string)
                .or_else(|| c.as_i64().map(|n| n.to_string()))
        })
        .filter(|s| !s.is_empty() && s != "0")
}

fn map_lang(code: &str) -> String {
    match code.to_ascii_lowercase().as_str() {
        "zh" | "zh-cn" | "zh-hans" => "zh".into(),
        "zh-tw" | "zh-hant" | "cht" => "cht".into(),
        "auto" => "auto".into(),
        other => other.to_string(),
    }
}
