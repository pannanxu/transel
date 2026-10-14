//! 后端注册表：根据 [`BackendKind`] 构造 [`Translator`]。
//!
//! 集中放在这里方便单测时 mock。

use std::collections::HashMap;
use std::sync::Arc;

use crate::app::config::{BackendEntry, BackendKind};
use crate::app::secrets;
use crate::services::translate::{Translator, ureq_agent};

use super::aliyun::AliyunTranslator;
use super::amazon::AmazonTranslator;
use super::apple::AppleTranslator;
use super::baidu::BaiduTranslator;
use super::caiyun::CaiyunTranslator;
use super::claude::ClaudeTranslator;
use super::deepl::DeepLTranslator;
use super::dictionary::DictionaryTranslator;
use super::gemini::GeminiTranslator;
use super::google::GoogleTranslator;
use super::google_free::GoogleFreeTranslator;
use super::microsoft::MicrosoftTranslator;
use super::niutrans::NiutransTranslator;
use super::openai_compat::OpenAiCompatTranslator;
use super::volcano::VolcanoTranslator;
use super::youdao::YoudaoTranslator;

type BackendBuilder = Arc<dyn Fn(&BackendEntry) -> Option<Arc<dyn Translator>> + Send + Sync>;

pub struct BackendRegistry {
    /// 单后端构造器；为 None 表示该 kind 未实现
    builders: HashMap<BackendKind, BackendBuilder>,
}

impl BackendRegistry {
    pub fn new() -> Self {
        Self {
            builders: HashMap::new(),
        }
    }

    pub fn with_defaults() -> Self {
        let mut r = Self::new();
        r.register(BackendKind::OpenAiCompat, openai_compat_builder);
        r.register(BackendKind::DeepL, deepl_builder);
        r.register(BackendKind::Google, google_builder);
        r.register(BackendKind::Baidu, baidu_builder);
        r.register(BackendKind::Youdao, youdao_builder);
        r.register(BackendKind::Microsoft, microsoft_builder);
        r.register(BackendKind::GoogleFree, google_free_builder);
        r.register(BackendKind::Caiyun, caiyun_builder);
        r.register(BackendKind::Volcano, volcano_builder);
        r.register(BackendKind::Aliyun, aliyun_builder);
        r.register(BackendKind::Apple, apple_builder);
        r.register(BackendKind::Dictionary, dictionary_builder);
        r.register(BackendKind::Niutrans, niutrans_flash_builder);
        r.register(BackendKind::NiutransPro, niutrans_pro_builder);
        r.register(BackendKind::Amazon, amazon_builder);
        r.register(BackendKind::Claude, claude_builder);
        r.register(BackendKind::Gemini, gemini_builder);
        r
    }

    pub fn register(
        &mut self,
        kind: BackendKind,
        f: impl Fn(&BackendEntry) -> Option<Arc<dyn Translator>> + Send + Sync + 'static,
    ) {
        self.builders.insert(kind, Arc::new(f));
    }

    pub fn build(&self, entry: &BackendEntry) -> Option<Arc<dyn Translator>> {
        self.builders.get(&entry.kind).and_then(|f| f(entry))
    }
}

impl Default for BackendRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

pub fn default_registry() -> BackendRegistry {
    BackendRegistry::with_defaults()
}

// ---------- builders ----------

fn openai_compat_builder(e: &BackendEntry) -> Option<Arc<dyn Translator>> {
    let base = e.params.get("base")?.clone();
    if base.trim().is_empty() {
        return None;
    }
    let model = e
        .params
        .get("model")
        .cloned()
        .unwrap_or_else(|| "gpt-4o-mini".into());
    let temperature = e
        .params
        .get("temperature")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.2);
    let system_prompt = e.params.get("prompt").cloned().filter(|s| !s.is_empty());
    let key = secrets::lookup_key(&e.params, &e.secret_account());
    let display_name = if e.display_name.is_empty() {
        BackendKind::OpenAiCompat.default_display_name().into()
    } else {
        e.display_name.clone()
    };
    // 流式翻译可能较慢，单独给更长超时
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(90)))
        .build()
        .into();
    let auth = e
        .params
        .get("auth")
        .cloned()
        .unwrap_or_else(|| "bearer".into());
    let api_version = e
        .params
        .get("api_version")
        .cloned()
        .filter(|s| !s.is_empty());
    Some(Arc::new(OpenAiCompatTranslator::with_auth(
        display_name,
        base,
        key,
        model,
        temperature,
        system_prompt,
        auth,
        api_version,
        agent,
    )))
}

fn deepl_builder(e: &BackendEntry) -> Option<Arc<dyn Translator>> {
    let key = secrets::lookup_key(&e.params, "deepl")?;
    Some(Arc::new(DeepLTranslator::new(key, ureq_agent())))
}

fn google_builder(e: &BackendEntry) -> Option<Arc<dyn Translator>> {
    let key = secrets::lookup_key(&e.params, "google")?;
    let project = e.params.get("project").cloned();
    Some(Arc::new(GoogleTranslator::new(key, project, ureq_agent())))
}

fn baidu_builder(e: &BackendEntry) -> Option<Arc<dyn Translator>> {
    let app_id = e.params.get("app_id")?.clone();
    let key = secrets::lookup_key(&e.params, &e.secret_account())
        .or_else(|| secrets::lookup_key(&e.params, "baidu"))?;
    Some(Arc::new(BaiduTranslator::new(app_id, key, ureq_agent())))
}

fn youdao_builder(e: &BackendEntry) -> Option<Arc<dyn Translator>> {
    let app_id = e.params.get("app_id")?.clone();
    let key = secrets::lookup_key(&e.params, &e.secret_account())
        .or_else(|| secrets::lookup_key(&e.params, "youdao"))?;
    Some(Arc::new(YoudaoTranslator::new(app_id, key, ureq_agent())))
}

fn microsoft_builder(e: &BackendEntry) -> Option<Arc<dyn Translator>> {
    let key = secrets::lookup_key(&e.params, &e.secret_account())
        .or_else(|| secrets::lookup_key(&e.params, "microsoft"))?;
    let region = e
        .params
        .get("region")
        .cloned()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "eastasia".into());
    let endpoint = e.params.get("endpoint").cloned().unwrap_or_default();
    let deployment = e.params.get("deployment").cloned();
    Some(Arc::new(MicrosoftTranslator::new(
        key,
        region,
        endpoint,
        deployment,
        ureq_agent(),
    )))
}

fn google_free_builder(_e: &BackendEntry) -> Option<Arc<dyn Translator>> {
    Some(Arc::new(GoogleFreeTranslator::new(ureq_agent())))
}

fn caiyun_builder(e: &BackendEntry) -> Option<Arc<dyn Translator>> {
    let key = secrets::lookup_key(&e.params, &e.secret_account())
        .or_else(|| secrets::lookup_key(&e.params, "caiyun"))?;
    Some(Arc::new(CaiyunTranslator::new(key, ureq_agent())))
}

fn volcano_builder(e: &BackendEntry) -> Option<Arc<dyn Translator>> {
    let access_key = e
        .params
        .get("access_key")
        .cloned()
        .or_else(|| e.params.get("access_key_id").cloned())?;
    let secret = secrets::lookup_key(&e.params, &e.secret_account())
        .or_else(|| secrets::lookup_key(&e.params, "volcano"))?;
    let region = e
        .params
        .get("region")
        .cloned()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "cn-north-1".into());
    Some(Arc::new(VolcanoTranslator::new(
        access_key,
        secret,
        region,
        ureq_agent(),
    )))
}

fn aliyun_builder(e: &BackendEntry) -> Option<Arc<dyn Translator>> {
    let access_key_id = e
        .params
        .get("access_key_id")
        .cloned()
        .or_else(|| e.params.get("access_key").cloned())?;
    let secret = secrets::lookup_key(&e.params, &e.secret_account())
        .or_else(|| secrets::lookup_key(&e.params, "aliyun"))?;
    Some(Arc::new(AliyunTranslator::new(
        access_key_id,
        secret,
        ureq_agent(),
    )))
}

fn apple_builder(_e: &BackendEntry) -> Option<Arc<dyn Translator>> {
    Some(Arc::new(AppleTranslator::new()))
}

fn dictionary_builder(_e: &BackendEntry) -> Option<Arc<dyn Translator>> {
    Some(Arc::new(DictionaryTranslator::new()))
}

fn niutrans_flash_builder(e: &BackendEntry) -> Option<Arc<dyn Translator>> {
    let key = secrets::lookup_key(&e.params, &e.secret_account())
        .or_else(|| secrets::lookup_key(&e.params, "niutrans"))?;
    let app_id = e.params.get("app_id").cloned().unwrap_or_default();
    Some(Arc::new(NiutransTranslator::flash(
        app_id,
        key,
        ureq_agent(),
    )))
}

fn niutrans_pro_builder(e: &BackendEntry) -> Option<Arc<dyn Translator>> {
    let key = secrets::lookup_key(&e.params, &e.secret_account())
        .or_else(|| secrets::lookup_key(&e.params, "niutrans"))?;
    Some(Arc::new(NiutransTranslator::pro(key, ureq_agent())))
}

fn amazon_builder(e: &BackendEntry) -> Option<Arc<dyn Translator>> {
    let access_key = e
        .params
        .get("access_key")
        .cloned()
        .or_else(|| e.params.get("access_key_id").cloned())?;
    let secret = secrets::lookup_key(&e.params, "amazon")?;
    let region = e
        .params
        .get("region")
        .cloned()
        .unwrap_or_else(|| "us-east-1".into());
    Some(Arc::new(AmazonTranslator::new(
        access_key,
        secret,
        region,
        ureq_agent(),
    )))
}

fn claude_builder(e: &BackendEntry) -> Option<Arc<dyn Translator>> {
    let key = secrets::lookup_key(&e.params, "claude")?;
    let model = e
        .params
        .get("model")
        .cloned()
        .unwrap_or_else(|| "claude-3-5-haiku-latest".into());
    Some(Arc::new(ClaudeTranslator::new(key, model, ureq_agent())))
}

fn gemini_builder(e: &BackendEntry) -> Option<Arc<dyn Translator>> {
    let key = secrets::lookup_key(&e.params, "gemini")?;
    let model = e
        .params
        .get("model")
        .cloned()
        .unwrap_or_else(|| "gemini-2.0-flash".into());
    Some(Arc::new(GeminiTranslator::new(key, model, ureq_agent())))
}
