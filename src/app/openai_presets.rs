//! OpenAI 兼容通道预设（智谱 / Groq / Ollama 等共用 `/chat/completions`）。

use std::collections::BTreeMap;

use super::config::BackendEntry;

#[derive(Clone, Copy)]
pub struct OpenAiPreset {
    pub id: &'static str,
    pub display_name: &'static str,
    pub base: &'static str,
    pub model: &'static str,
    /// `bearer`（默认）或 `api-key`（Azure OpenAI）。
    pub auth: &'static str,
    pub api_version: Option<&'static str>,
}

pub const OPENAI_PRESETS: &[OpenAiPreset] = &[
    OpenAiPreset {
        id: "openai",
        display_name: "OpenAI",
        base: "https://api.openai.com/v1",
        model: "gpt-4o-mini",
        auth: "bearer",
        api_version: None,
    },
    OpenAiPreset {
        id: "azure_openai",
        display_name: "Azure OpenAI",
        base: "https://YOUR_RESOURCE.openai.azure.com/openai/deployments/YOUR_DEPLOYMENT",
        model: "gpt-4o-mini",
        auth: "api-key",
        api_version: Some("2024-06-01"),
    },
    OpenAiPreset {
        id: "glm",
        display_name: "智谱 GLM",
        base: "https://open.bigmodel.cn/api/paas/v4",
        model: "glm-4-flash",
        auth: "bearer",
        api_version: None,
    },
    OpenAiPreset {
        id: "siliconflow",
        display_name: "硅基流动",
        base: "https://api.siliconflow.cn/v1",
        model: "Qwen/Qwen2.5-7B-Instruct",
        auth: "bearer",
        api_version: None,
    },
    OpenAiPreset {
        id: "aihubmix",
        display_name: "AIHubMix",
        base: "https://aihubmix.com/v1",
        model: "gpt-4o-mini",
        auth: "bearer",
        api_version: None,
    },
    OpenAiPreset {
        id: "sophnet",
        display_name: "SophNet",
        base: "https://www.sophnet.com/api/open-apis/v1",
        model: "DeepSeek-V3",
        auth: "bearer",
        api_version: None,
    },
    OpenAiPreset {
        id: "ai302",
        display_name: "302.AI",
        base: "https://api.302.ai/v1",
        model: "gpt-4o-mini",
        auth: "bearer",
        api_version: None,
    },
    OpenAiPreset {
        id: "groq",
        display_name: "Groq",
        base: "https://api.groq.com/openai/v1",
        model: "llama-3.1-8b-instant",
        auth: "bearer",
        api_version: None,
    },
    OpenAiPreset {
        id: "grok",
        display_name: "Grok",
        base: "https://api.x.ai/v1",
        model: "grok-2-latest",
        auth: "bearer",
        api_version: None,
    },
    OpenAiPreset {
        id: "openrouter",
        display_name: "OpenRouter",
        base: "https://openrouter.ai/api/v1",
        model: "openai/gpt-4o-mini",
        auth: "bearer",
        api_version: None,
    },
    OpenAiPreset {
        id: "ollama",
        display_name: "Ollama",
        base: "http://127.0.0.1:11434/v1",
        model: "llama3.2",
        auth: "bearer",
        api_version: None,
    },
    OpenAiPreset {
        id: "lmstudio",
        display_name: "LM Studio",
        base: "http://127.0.0.1:1234/v1",
        model: "local-model",
        auth: "bearer",
        api_version: None,
    },
    OpenAiPreset {
        id: "deepseek",
        display_name: "DeepSeek",
        base: "https://api.deepseek.com/v1",
        model: "deepseek-chat",
        auth: "bearer",
        api_version: None,
    },
    OpenAiPreset {
        id: "qwen",
        display_name: "通义千问",
        base: "https://dashscope.aliyuncs.com/compatible-mode/v1",
        model: "qwen-turbo",
        auth: "bearer",
        api_version: None,
    },
    OpenAiPreset {
        id: "ernie",
        display_name: "文心一言",
        base: "https://qianfan.baidubce.com/v2",
        model: "ernie-4.0-8k",
        auth: "bearer",
        api_version: None,
    },
    OpenAiPreset {
        id: "doubao",
        display_name: "豆包",
        base: "https://ark.cn-beijing.volces.com/api/v3",
        model: "doubao-pro-32k",
        auth: "bearer",
        api_version: None,
    },
    OpenAiPreset {
        id: "ark",
        display_name: "模力方舟",
        base: "https://ark.cn-beijing.volces.com/api/v3",
        model: "doubao-pro-32k",
        auth: "bearer",
        api_version: None,
    },
    OpenAiPreset {
        id: "hunyuan",
        display_name: "混元",
        base: "https://api.hunyuan.cloud.tencent.com/v1",
        model: "hunyuan-turbo",
        auth: "bearer",
        api_version: None,
    },
    OpenAiPreset {
        id: "kimi",
        display_name: "Kimi",
        base: "https://api.moonshot.cn/v1",
        model: "moonshot-v1-8k",
        auth: "bearer",
        api_version: None,
    },
];

impl OpenAiPreset {
    pub fn to_entry(self, enabled: bool) -> BackendEntry {
        let mut params = BTreeMap::new();
        params.insert("preset".into(), self.id.into());
        params.insert("base".into(), self.base.trim_end_matches('/').into());
        params.insert("model".into(), self.model.into());
        params.insert("auth".into(), self.auth.into());
        if let Some(v) = self.api_version {
            params.insert("api_version".into(), v.into());
        }
        BackendEntry::openai_compat(self.display_name, self.base, params, enabled)
    }
}
