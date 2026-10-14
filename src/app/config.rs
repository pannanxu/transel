//! 应用配置：从 JSON 文件加载，落盘时合并。
//!
//! 向后兼容：旧的 `TRANSLATE_API_BASE / API_KEY / MODEL / TARGET_LANG / FREE_FALLBACK`
//! 环境变量仍可生效，作为首次启动的种子。

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::openai_presets::OPENAI_PRESETS;
use super::secrets;

const SCHEMA_VERSION: u32 = 7;

/// 顶层配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_schema")]
    pub schema: u32,
    /// 划词翻译热键：默认 Alt+D
    #[serde(default = "default_translate_hotkey")]
    pub translate_hotkey: HotkeySpec,
    /// 切换"弹窗固定"的全局热键（默认 Ctrl/Cmd+Shift+P）
    #[serde(default = "default_pin_hotkey")]
    pub pin_hotkey: HotkeySpec,
    /// 截图 OCR 翻译热键（默认 Ctrl/Cmd+Shift+D）
    #[serde(default = "default_ocr_hotkey")]
    pub ocr_hotkey: HotkeySpec,
    /// 输入翻译窗口热键（默认 Ctrl/Cmd+Shift+I）
    #[serde(default = "default_input_hotkey")]
    pub input_hotkey: HotkeySpec,
    /// 朗读选中文本热键（默认 Ctrl/Cmd+Shift+R）
    #[serde(default = "default_speak_hotkey")]
    pub speak_hotkey: HotkeySpec,
    /// 显示历史窗口热键（默认 Ctrl/Cmd+Shift+H）
    #[serde(default = "default_history_hotkey")]
    pub history_hotkey: HotkeySpec,
    /// 显示设置窗口热键（默认 Ctrl/Cmd+,）
    #[serde(default = "default_settings_hotkey")]
    pub settings_hotkey: HotkeySpec,
    /// 独立截图 OCR（不翻译）热键：默认 Shift+Alt+S
    #[serde(default = "default_ocr_only_hotkey")]
    pub ocr_only_hotkey: HotkeySpec,
    /// 静默截图 OCR（识别后只复制到剪贴板）热键：默认 Alt+C；key 为空表示不注册
    #[serde(default = "default_ocr_silent_hotkey")]
    pub ocr_silent_hotkey: HotkeySpec,
    /// 文件管理器选图 OCR 热键：默认不注册（key 为空）
    #[serde(default = "default_ocr_finder_hotkey")]
    pub ocr_finder_hotkey: HotkeySpec,
    /// 静默划词翻译（译文替换选区）：默认不注册
    #[serde(default = "default_silent_replace_hotkey")]
    pub silent_replace_hotkey: HotkeySpec,
    /// 静默输入框翻译（译文替换输入框全文）：默认不注册
    #[serde(default = "default_silent_input_hotkey")]
    pub silent_input_hotkey: HotkeySpec,
    /// 原图翻译热键：默认 Alt+Shift+O
    #[serde(default = "default_overlay_hotkey")]
    pub overlay_hotkey: HotkeySpec,
    /// 打开收藏夹：默认 Cmd+Shift+F
    #[serde(default = "default_favorites_hotkey")]
    pub favorites_hotkey: HotkeySpec,
    /// OCR 连续识别：新结果追加到 OCR 窗口文本框
    #[serde(default)]
    pub ocr_continuous: bool,
    /// OCR 识别成功后自动复制到剪贴板
    #[serde(default = "default_true")]
    pub ocr_auto_copy: bool,
    /// OCR 智能分段（按段落合并断行）
    #[serde(default = "default_true")]
    pub ocr_smart_paragraph: bool,
    /// 目标语言（ISO 代码，如 zh-CN / en / ja）
    #[serde(default = "default_target_lang")]
    pub target_lang: String,
    /// 源语言：`auto` 表示自动检测
    #[serde(default = "default_source_lang")]
    pub source_lang: String,
    /// 是否启用"若源 == 目标自动反向翻译"
    #[serde(default = "default_true")]
    pub auto_swap: bool,
    /// 弹窗是否默认固定（不因失焦自动关闭）
    #[serde(default)]
    pub pin_popup_by_default: bool,
    /// 弹窗是否跟随鼠标；关闭时固定在屏幕中上方（默认关闭）
    #[serde(default)]
    pub popup_follow_mouse: bool,
    /// 启用的翻译后端（顺序即优先级，重复启用会全部并排展示）
    #[serde(default = "default_backends")]
    pub backends: Vec<BackendEntry>,
    /// OCR 后端：`auto` / `rapidocr` / `volcano_ocr` / `tencent_ocr` / `baidu_cloud` 等
    #[serde(default = "default_ocr_backend")]
    pub ocr_backend: String,
    /// TTS 后端：`auto` / `say` / `powershell` / `spd-say` 等
    #[serde(default = "default_tts_backend")]
    pub tts_backend: String,
    /// 是否在启动时自动检查更新
    #[serde(default = "default_true")]
    pub auto_check_update: bool,
    /// 历史保留最大条数；0 表示无限
    #[serde(default = "default_history_limit")]
    pub history_limit: usize,
    /// 主题：`auto` / `light` / `dark`
    #[serde(default = "default_theme")]
    pub theme: String,
    /// 选词触发方式：`hotkey`（按下热键时抓取选区）/`auto`（划选即抓取）
    #[serde(default = "default_trigger")]
    pub trigger: String,
}

fn default_schema() -> u32 {
    SCHEMA_VERSION
}
fn default_true() -> bool {
    true
}
fn default_target_lang() -> String {
    "zh-CN".into()
}
fn default_source_lang() -> String {
    "auto".into()
}
fn default_backends() -> Vec<BackendEntry> {
    vec![BackendEntry::system_default()]
}

fn new_backend_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn backend_params_openai(base: &str) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    m.insert("base".into(), base.trim_end_matches('/').to_string());
    if let Some(k) = std::env::var("TRANSLATE_API_KEY")
        .ok()
        .filter(|s| !s.is_empty())
    {
        m.insert("key_ref".into(), secrets::ref_for_inline(&k));
    }
    m.insert(
        "model".into(),
        std::env::var("TRANSLATE_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into()),
    );
    m
}

fn default_ocr_backend() -> String {
    "auto".into()
}
fn default_tts_backend() -> String {
    "auto".into()
}
fn default_history_limit() -> usize {
    5000
}
fn default_theme() -> String {
    "auto".into()
}
fn default_trigger() -> String {
    "hotkey".into()
}
fn default_translate_hotkey() -> HotkeySpec {
    HotkeySpec {
        modifiers: vec!["alt".into()],
        key: "D".into(),
    }
}
fn default_pin_hotkey() -> HotkeySpec {
    HotkeySpec {
        modifiers: vec!["cmd".into(), "shift".into()],
        key: "P".into(),
    }
}
fn default_ocr_hotkey() -> HotkeySpec {
    HotkeySpec {
        modifiers: vec!["alt".into()],
        key: "F".into(),
    }
}
fn default_input_hotkey() -> HotkeySpec {
    HotkeySpec {
        modifiers: vec!["cmd".into(), "shift".into()],
        key: "I".into(),
    }
}
fn default_speak_hotkey() -> HotkeySpec {
    HotkeySpec {
        modifiers: vec!["cmd".into(), "shift".into()],
        key: "R".into(),
    }
}
fn default_history_hotkey() -> HotkeySpec {
    HotkeySpec {
        modifiers: vec!["cmd".into(), "shift".into()],
        key: "H".into(),
    }
}
fn default_settings_hotkey() -> HotkeySpec {
    HotkeySpec {
        modifiers: vec!["cmd".into()],
        key: ",".into(),
    }
}
fn default_ocr_only_hotkey() -> HotkeySpec {
    HotkeySpec {
        modifiers: vec!["shift".into(), "alt".into()],
        key: "S".into(),
    }
}
fn default_ocr_silent_hotkey() -> HotkeySpec {
    HotkeySpec {
        modifiers: vec!["alt".into()],
        key: "C".into(),
    }
}
fn default_ocr_finder_hotkey() -> HotkeySpec {
    HotkeySpec {
        modifiers: vec![],
        key: String::new(),
    }
}
fn default_silent_replace_hotkey() -> HotkeySpec {
    HotkeySpec {
        modifiers: vec!["alt".into()],
        key: "S".into(),
    }
}
fn default_silent_input_hotkey() -> HotkeySpec {
    HotkeySpec {
        modifiers: vec!["alt".into()],
        key: "A".into(),
    }
}
fn default_overlay_hotkey() -> HotkeySpec {
    HotkeySpec {
        modifiers: vec!["alt".into(), "shift".into()],
        key: "O".into(),
    }
}
fn default_favorites_hotkey() -> HotkeySpec {
    HotkeySpec {
        modifiers: vec!["cmd".into(), "shift".into()],
        key: "F".into(),
    }
}

/// 一个热键：`modifiers + key`。运行时由 [`crate::app::hotkey`] 转成
/// [`global_hotkey::hotkey::HotKey`]。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HotkeySpec {
    #[serde(default)]
    pub modifiers: Vec<String>,
    pub key: String,
}

impl std::fmt::Display for HotkeySpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts: Vec<String> = self.modifiers.iter().map(|m| pretty_mod(m)).collect();
        parts.push(self.key.to_uppercase());
        write!(f, "{}", parts.join("+"))
    }
}

fn pretty_mod(m: &str) -> String {
    match m.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => "Ctrl".into(),
        "cmd" | "meta" | "super" => "Cmd".into(),
        "alt" => "Alt".into(),
        "shift" => "Shift".into(),
        other => other.to_string(),
    }
}

/// 翻译后端枚举。新增后端只需扩这个 enum + 在 [`crate::services::translate::backends`] 实现。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    OpenAiCompat,
    DeepL,
    Google,
    Baidu,
    Youdao,
    Tencent,
    Microsoft,
    GoogleFree,
    Caiyun,
    Volcano,
    Aliyun,
    Apple,
    /// Free Dictionary API（英语单词释义，与其它通道并排）
    Dictionary,
    Niutrans,
    Amazon,
    Claude,
    Gemini,
}

impl BackendKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            BackendKind::OpenAiCompat => "openai_compat",
            BackendKind::DeepL => "deepl",
            BackendKind::Google => "google",
            BackendKind::Baidu => "baidu",
            BackendKind::Youdao => "youdao",
            BackendKind::Tencent => "tencent",
            BackendKind::Microsoft => "microsoft",
            BackendKind::GoogleFree => "google_free",
            BackendKind::Caiyun => "caiyun",
            BackendKind::Volcano => "volcano",
            BackendKind::Aliyun => "aliyun",
            BackendKind::Apple => "apple",
            BackendKind::Dictionary => "dictionary",
            BackendKind::Niutrans => "niutrans",
            BackendKind::Amazon => "amazon",
            BackendKind::Claude => "claude",
            BackendKind::Gemini => "gemini",
        }
    }

    pub fn all() -> &'static [BackendKind] {
        &[
            BackendKind::OpenAiCompat,
            BackendKind::DeepL,
            BackendKind::Google,
            BackendKind::Baidu,
            BackendKind::Youdao,
            BackendKind::Microsoft,
            BackendKind::GoogleFree,
            BackendKind::Caiyun,
            BackendKind::Volcano,
            BackendKind::Aliyun,
            BackendKind::Apple,
            BackendKind::Dictionary,
            BackendKind::Niutrans,
            BackendKind::Amazon,
            BackendKind::Claude,
            BackendKind::Gemini,
        ]
    }

    pub fn needs_key(&self) -> bool {
        !matches!(
            self,
            BackendKind::GoogleFree | BackendKind::Apple | BackendKind::Dictionary
        )
    }

    pub fn default_display_name(self) -> &'static str {
        match self {
            BackendKind::OpenAiCompat => "OpenAI 兼容",
            BackendKind::DeepL => "DeepL 翻译",
            BackendKind::Google => "Google 翻译",
            BackendKind::Baidu => "百度翻译",
            BackendKind::Youdao => "有道翻译",
            BackendKind::Tencent => "腾讯翻译君",
            BackendKind::Microsoft => "Microsoft 翻译",
            BackendKind::GoogleFree => "Google 免费",
            BackendKind::Caiyun => "彩云小译",
            BackendKind::Volcano => "火山翻译",
            BackendKind::Aliyun => "阿里翻译",
            BackendKind::Apple => "Apple 翻译",
            BackendKind::Dictionary => "词典",
            BackendKind::Niutrans => "小牛翻译",
            BackendKind::Amazon => "Amazon 翻译",
            BackendKind::Claude => "Claude",
            BackendKind::Gemini => "Gemini",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendEntry {
    /// 稳定唯一 id（同一 `kind` 可有多条，如多个 OpenAI 兼容接口）。
    #[serde(default)]
    pub id: String,
    pub kind: BackendKind,
    pub enabled: bool,
    #[serde(default)]
    pub display_name: String,
    /// 自由参数：
    /// - OpenAI 兼容：`base` / `model` / `key_ref`（指向钥匙串）/ `temperature` / `prompt`
    /// - DeepL：`key_ref`（`:fx` 自动走免费地址）
    /// - Google Cloud：`key_ref` / `project`
    /// - 百度：`app_id` / `key_ref`
    /// - 有道：`app_id` / `key_ref`
    /// - 腾讯：`secret_id` / `secret_key_ref` / `region`
    /// - 微软：`key_ref` / `region` / `endpoint`
    #[serde(default)]
    pub params: BTreeMap<String, String>,
}

impl BackendEntry {
    /// 新安装默认：macOS 用 Apple 翻译，其它平台用 Google 免费。
    pub fn system_default() -> Self {
        #[cfg(target_os = "macos")]
        let kind = BackendKind::Apple;
        #[cfg(not(target_os = "macos"))]
        let kind = BackendKind::GoogleFree;
        Self::from_kind(kind)
    }

    /// 按服务类型新建一条（添加面板选用）。
    pub fn from_kind(kind: BackendKind) -> Self {
        if kind == BackendKind::OpenAiCompat {
            return Self::openai_compat(
                kind.default_display_name(),
                "https://api.openai.com/v1",
                BTreeMap::new(),
                true,
            );
        }
        Self {
            id: new_backend_id(),
            kind,
            enabled: true,
            display_name: kind.default_display_name().into(),
            params: BTreeMap::new(),
        }
    }

    /// 新建一条可自定义的 OpenAI 兼容接口。
    pub fn openai_compat(
        display_name: impl Into<String>,
        base: &str,
        mut params: BTreeMap<String, String>,
        enabled: bool,
    ) -> Self {
        params
            .entry("base".into())
            .or_insert_with(|| base.trim_end_matches('/').to_string());
        params
            .entry("model".into())
            .or_insert_with(|| "gpt-4o-mini".into());
        Self {
            id: new_backend_id(),
            kind: BackendKind::OpenAiCompat,
            enabled,
            display_name: display_name.into(),
            params,
        }
    }

    /// 钥匙串 account：按条目 id 隔离，避免多个 OpenAI 兼容接口共用一把密钥。
    pub fn secret_account(&self) -> String {
        if self.id.is_empty() {
            self.kind.as_str().to_string()
        } else {
            format!("{}_{}", self.kind.as_str(), self.id)
        }
    }

    pub fn ensure_id(&mut self) {
        if self.id.is_empty() {
            self.id = new_backend_id();
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema: SCHEMA_VERSION,
            translate_hotkey: default_translate_hotkey(),
            pin_hotkey: default_pin_hotkey(),
            ocr_hotkey: default_ocr_hotkey(),
            input_hotkey: default_input_hotkey(),
            speak_hotkey: default_speak_hotkey(),
            history_hotkey: default_history_hotkey(),
            settings_hotkey: default_settings_hotkey(),
            ocr_only_hotkey: default_ocr_only_hotkey(),
            ocr_silent_hotkey: default_ocr_silent_hotkey(),
            ocr_finder_hotkey: default_ocr_finder_hotkey(),
            silent_replace_hotkey: default_silent_replace_hotkey(),
            silent_input_hotkey: default_silent_input_hotkey(),
            overlay_hotkey: default_overlay_hotkey(),
            favorites_hotkey: default_favorites_hotkey(),
            ocr_continuous: false,
            ocr_auto_copy: true,
            ocr_smart_paragraph: true,
            target_lang: default_target_lang(),
            source_lang: default_source_lang(),
            auto_swap: true,
            pin_popup_by_default: false,
            popup_follow_mouse: false,
            backends: default_backends(),
            ocr_backend: default_ocr_backend(),
            tts_backend: default_tts_backend(),
            auto_check_update: true,
            history_limit: default_history_limit(),
            theme: default_theme(),
            trigger: default_trigger(),
        }
    }
}

impl Config {
    /// 从默认配置生成（首次启动时使用）。
    pub fn fresh() -> Self {
        Self::default()
    }

    /// 从文件加载，文件不存在或解析失败时返回默认。
    pub fn load_or_default(path: &Path) -> Self {
        match std::fs::read(path) {
            Ok(bytes) => match serde_json::from_slice::<Config>(&bytes) {
                Ok(mut c) => {
                    c.migrate();
                    c
                }
                Err(e) => {
                    log::warn!("配置 {} 解析失败: {e}，使用默认配置", path.display());
                    Self::default()
                }
            },
            Err(_) => {
                // 第一次启动：把环境变量迁过来（向后兼容）
                let mut c = Self::default();
                c.absorb_legacy_env();
                c
            }
        }
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let s = serde_json::to_string_pretty(self)?;
        // 写临时文件再 rename，避免半途崩溃导致配置损坏
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, s)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// 把旧版本字段映射到新结构。
    fn migrate(&mut self) {
        if self.schema < 2 {
            // schema 2：统一默认热键
            // Option+A 静默输入框 / Option+S 静默划词 / Option+D 划词翻译 / Option+F 截图 OCR 翻译
            self.silent_input_hotkey = default_silent_input_hotkey();
            self.silent_replace_hotkey = default_silent_replace_hotkey();
            self.translate_hotkey = default_translate_hotkey();
            self.ocr_hotkey = default_ocr_hotkey();
            self.schema = 2;
        }
        if self.schema < 3 {
            // schema 3：每条后端有稳定 id，支持多个 OpenAI 兼容接口
            for b in self.backends.iter_mut() {
                b.ensure_id();
            }
            self.schema = 3;
        }
        if self.schema < 4 {
            // schema 4：词典作为普通翻译通道（默认启用）
            if !self
                .backends
                .iter()
                .any(|b| b.kind == BackendKind::Dictionary)
            {
                self.backends.insert(
                    0,
                    BackendEntry {
                        id: new_backend_id(),
                        kind: BackendKind::Dictionary,
                        enabled: true,
                        display_name: BackendKind::Dictionary.default_display_name().into(),
                        params: BTreeMap::new(),
                    },
                );
            }
            self.schema = 4;
        }
        if self.schema < 5 {
            for preset in OPENAI_PRESETS {
                let exists = self.backends.iter().any(|b| {
                    b.kind == BackendKind::OpenAiCompat
                        && b.params.get("preset").map(|s| s.as_str()) == Some(preset.id)
                });
                if !exists {
                    self.backends.push(preset.to_entry(false));
                }
            }
            self.schema = 5;
        }
        if self.schema < 6 {
            self.backends.retain(|b| b.kind != BackendKind::Tencent);
            self.schema = 6;
        }
        if self.schema < 7 {
            self.backends.retain(|b| !backend_is_unused_stub(b));
            if !self
                .backends
                .iter()
                .any(|b| matches!(b.kind, BackendKind::Apple | BackendKind::GoogleFree))
            {
                self.backends.insert(0, BackendEntry::system_default());
            }
            self.schema = 7;
        }
        if self.schema == 0 {
            self.schema = SCHEMA_VERSION;
        }
        for b in self.backends.iter_mut() {
            b.ensure_id();
        }
        self.absorb_legacy_env();
    }

    fn absorb_legacy_env(&mut self) {
        if let Ok(base) = std::env::var("TRANSLATE_API_BASE")
            && !base.is_empty()
        {
            let mut found = false;
            for b in self.backends.iter_mut() {
                if b.kind == BackendKind::OpenAiCompat {
                    b.params
                        .insert("base".into(), base.trim_end_matches('/').to_string());
                    if !b.params.contains_key("model") {
                        b.params.insert(
                            "model".into(),
                            std::env::var("TRANSLATE_MODEL")
                                .unwrap_or_else(|_| "gpt-4o-mini".into()),
                        );
                    }
                    if let Ok(k) = std::env::var("TRANSLATE_API_KEY")
                        && !k.is_empty()
                    {
                        b.params
                            .entry("key_ref".into())
                            .or_insert_with(|| secrets::ref_for_inline(&k));
                    }
                    found = true;
                }
            }
            if !found {
                self.backends.insert(
                    0,
                    BackendEntry::openai_compat(
                        "OpenAI 兼容",
                        &base,
                        backend_params_openai(&base),
                        true,
                    ),
                );
            }
        }
        if let Ok(target) = std::env::var("TRANSLATE_TARGET_LANG")
            && !target.is_empty()
        {
            self.target_lang = target;
        }
        if let Ok(fb) = std::env::var("TRANSLATE_FREE_FALLBACK") {
            let want = !(fb == "0" || fb.eq_ignore_ascii_case("false"));
            for b in self.backends.iter_mut() {
                if b.kind == BackendKind::GoogleFree {
                    b.enabled = want;
                }
            }
        }
    }
}

/// 旧设置页会把所有通道预填成「已禁用、未配置」占位；这些不再需要。
fn backend_is_unused_stub(b: &BackendEntry) -> bool {
    if b.enabled {
        return false;
    }
    if b.kind == BackendKind::Tencent {
        return true;
    }
    if b.params.keys().any(|k| {
        matches!(
            k.as_str(),
            "key_ref" | "key" | "app_id" | "secret_id" | "access_key" | "access_key_id"
        )
    }) {
        return false;
    }
    if b.kind == BackendKind::OpenAiCompat && b.params.contains_key("preset") {
        return true;
    }
    if matches!(b.kind, BackendKind::Apple | BackendKind::GoogleFree) {
        return false;
    }
    b.params.is_empty()
        || b.params
            .keys()
            .all(|k| matches!(k.as_str(), "base" | "model" | "auth" | "api_version" | "preset"))
}
