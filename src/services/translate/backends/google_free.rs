//! Google 翻译非官方免费接口（translate_a/single?client=gtx）。
//! 不需要 API Key；可能在某些网络环境受限。
//!
//! 长文本必须走 POST 表单：GET 把 `q` 放进 query 时 URL 过长会触发
//! `ureq`/`http` 的 `InvalidUri(TooLong)`（且以 panic 而非 Result 暴露）。

use anyhow::{Context as _, Result, anyhow};

use crate::services::translate::{SingleResult, Translator};

pub struct GoogleFreeTranslator {
    agent: ureq::Agent,
}

impl GoogleFreeTranslator {
    pub fn new(agent: ureq::Agent) -> Self {
        Self { agent }
    }
}

impl Translator for GoogleFreeTranslator {
    fn name(&self) -> &str {
        "Google 免费"
    }

    fn translate_blocking(
        &self,
        text: &str,
        target: &str,
        _source_hint: Option<&str>,
    ) -> Result<SingleResult> {
        // POST + x-www-form-urlencoded：正文放 body，避免 URI TooLong。
        let v: serde_json::Value = self
            .agent
            .post("https://translate.googleapis.com/translate_a/single")
            .send_form([
                ("client", "gtx"),
                ("sl", "auto"),
                ("tl", target),
                ("dt", "t"),
                ("q", text),
            ])
            .context("请求 Google 免费接口失败")?
            .body_mut()
            .read_json()
            .context("解析 Google 响应失败")?;
        let mut out = String::new();
        for seg in v[0].as_array().ok_or_else(|| anyhow!("意外的响应格式"))? {
            if let Some(s) = seg[0].as_str() {
                out.push_str(s);
            }
        }
        if out.is_empty() {
            anyhow::bail!("翻译结果为空");
        }
        Ok(SingleResult {
            backend: self.name().to_string(),
            text: out,
            source_lang: v[2].as_str().map(str::to_string),
            elapsed_ms: 0,
            ok: true,
            error: None,
            pending: false,
        })
    }
}
