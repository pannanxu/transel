//! GPUI 图标路径。
//!
//! `svg().path(...)` 从 [`AssetSource`] 按路径加载 SVG，不能传入内联 markup。
//! 默认 [`gpui_kit::assets::Assets`] 只包含组件库常用图标，其余用
//! [`gpui_kit::assets::icon_assets`] 补进 [`AppAssets`]。

use std::borrow::Cow;

use gpui_kit::*;

pub const COPY: &str = "icons/copy.svg";
pub const CHECK: &str = "icons/check.svg";
pub const CLOSE: &str = "icons/close.svg";
pub const PIN: &str = "icons/pin.svg";
pub const PIN_OFF: &str = "icons/pin-off.svg";
pub const SPEAKER: &str = "icons/volume-2.svg";
pub const BOOK: &str = "icons/book-open.svg";
pub const SETTINGS: &str = "icons/settings.svg";
pub const HISTORY: &str = "icons/refresh-cw.svg";
pub const INPUT: &str = "icons/square-terminal.svg";
pub const OCR: &str = "icons/camera.svg";
pub const PLUS: &str = "icons/plus.svg";
pub const TRASH: &str = "icons/delete.svg";
pub const CHEVRON_DOWN: &str = "icons/chevron-down.svg";
pub const CHEVRON_RIGHT: &str = "icons/chevron-right.svg";
pub const BOT: &str = "icons/bot.svg";
pub const GLOBE: &str = "icons/globe.svg";
pub const SPARKLES: &str = "icons/sparkles.svg";
pub const LANGUAGES: &str = "icons/languages.svg";
pub const MESSAGE: &str = "icons/message-square.svg";
pub const CPU: &str = "icons/cpu.svg";
pub const LOADER_CIRCLE: &str = "icons/loader-circle.svg";
pub const REPEAT: &str = "icons/repeat.svg";
pub const LOGO: &str = "images/icon.png";

const LOGO_PNG: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/packaging/icon.png"));

gpui_kit::assets::icon_assets!(
    ExtraIcons,
    [
        Pin,
        PinOff,
        Volume2,
        Camera,
        ChevronDown,
        ChevronRight,
        Bot,
        Globe,
        Sparkles,
        Languages,
        MessageSquare,
        Cpu,
        LoaderCircle,
        Repeat,
    ]
);

/// 按通道显示名选一个代表图标。
pub fn channel_icon(backend_name: &str) -> &'static str {
    let n = backend_name.to_ascii_lowercase();
    if n.contains("openai")
        || n.contains("gpt")
        || n.contains("deepseek")
        || n.contains("兼容")
        || n.contains("claude")
        || n.contains("ollama")
        || n.contains("llm")
    {
        BOT
    } else if n.contains("google") {
        GLOBE
    } else if n.contains("词典") || n.contains("dictionary") || n.contains("dict") {
        BOOK
    } else if n.contains("deepl") || n.contains("彩云") || n.contains("有道") {
        LANGUAGES
    } else if n.contains("apple") {
        SPARKLES
    } else if n.contains("微软") || n.contains("microsoft") || n.contains("azure") {
        CPU
    } else if n.contains("百度") || n.contains("腾讯") || n.contains("阿里") || n.contains("火山")
    {
        MESSAGE
    } else {
        BOOK
    }
}

/// 默认组件图标 + 本应用额外 Lucide 图标。
pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == LOGO {
            return Ok(Some(Cow::Borrowed(LOGO_PNG)));
        }
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        if path.is_empty() || path == "images" || path == "images/" {
            paths.push(LOGO.into());
        }
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}
