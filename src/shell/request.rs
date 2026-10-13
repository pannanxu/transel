//! 翻译请求上下文装配。

use crate::app::config::Config;
use crate::services::translate::RequestCtx;

pub fn build_request_ctx(cfg: Config, source: &str, prepared: &str) -> RequestCtx {
    RequestCtx {
        source: source.to_string(),
        prepared: prepared.to_string(),
        source_lang_hint: if cfg.source_lang == "auto" {
            None
        } else {
            Some(cfg.source_lang.clone())
        },
        target_lang: cfg.target_lang.clone(),
        only: None,
    }
}
