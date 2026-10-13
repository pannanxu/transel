//! 主题：颜色与字体。

use gpui_kit::*;

pub struct Palette {
    pub card: Hsla,
    pub border: Hsla,
    pub title: Hsla,
    pub muted: Hsla,
    pub text: Hsla,
    pub pill_bg: Hsla,
    pub hover: Hsla,
    pub divider: Hsla,
    pub accent: Hsla,
    pub accent_hover: Hsla,
    pub danger: Hsla,
    pub shadow: Hsla,
    pub input_bg: Hsla,
    pub tab_active: Hsla,
    pub tab_inactive: Hsla,
}

pub fn palette(appearance: WindowAppearance) -> Palette {
    let (light_a, dark_a) = if cfg!(target_os = "macos") { (0.28, 0.22) } else { (0.92, 0.88) };
    match appearance {
        WindowAppearance::Dark | WindowAppearance::VibrantDark => Palette {
            card: hsla(0., 0., 0.12, dark_a),
            border: hsla(0., 0., 1., 0.12),
            title: hsla(0., 0., 1., 0.85),
            muted: hsla(0., 0., 1., 0.5),
            text: hsla(0., 0., 1., 0.93),
            pill_bg: hsla(0., 0., 1., 0.1),
            hover: hsla(0., 0., 1., 0.12),
            divider: hsla(0., 0., 1., 0.08),
            accent: hsla(211. / 360., 1., 0.62, 1.),
            accent_hover: hsla(211. / 360., 1., 0.7, 1.),
            danger: hsla(0., 0.7, 0.6, 1.),
            shadow: hsla(0., 0., 0., 0.45),
            input_bg: hsla(0., 0., 1., 0.06),
            tab_active: hsla(211. / 360., 1., 0.62, 0.2),
            tab_inactive: hsla(0., 0., 1., 0.04),
        },
        _ => Palette {
            card: hsla(0., 0., 0.99, light_a),
            border: hsla(0., 0., 0., 0.1),
            title: hsla(0., 0., 0., 0.8),
            muted: hsla(0., 0., 0., 0.48),
            text: hsla(0., 0., 0., 0.88),
            pill_bg: hsla(0., 0., 0., 0.06),
            hover: hsla(0., 0., 0., 0.07),
            divider: hsla(0., 0., 0., 0.07),
            accent: hsla(211. / 360., 1., 0.5, 1.),
            accent_hover: hsla(211. / 360., 1., 0.55, 1.),
            danger: hsla(0., 0.7, 0.5, 1.),
            shadow: hsla(0., 0., 0., 0.22),
            input_bg: hsla(0., 0., 0., 0.04),
            tab_active: hsla(211. / 360., 1., 0.5, 0.18),
            tab_inactive: hsla(0., 0., 0., 0.04),
        },
    }
}

/// 可选语言列表：`(代码, 短标签, 全称)`。源语言含 `auto`；目标语言请跳过 auto。
pub const LANGUAGES: &[(&str, &str, &str)] = &[
    ("auto", "自动", "自动检测"),
    ("zh-CN", "中文", "中文 (简体)"),
    ("zh-TW", "繁中", "中文 (繁体)"),
    ("en", "EN", "英语"),
    ("ja", "日", "日语"),
    ("ko", "韩", "韩语"),
    ("fr", "FR", "法语"),
    ("de", "DE", "德语"),
    ("es", "ES", "西班牙语"),
    ("ru", "RU", "俄语"),
];

pub fn lang_label(code: &str) -> String {
    let norm = normalize_lang_code(code);
    LANGUAGES
        .iter()
        .find(|(c, _, _)| *c == norm)
        .map(|(_, short, _)| (*short).to_string())
        .unwrap_or_else(|| code.to_uppercase())
}

/// 归一化为配置里使用的语言代码。
pub fn normalize_lang_code(code: &str) -> String {
    let c = code.to_ascii_lowercase().replace('_', "-");
    match c.as_str() {
        "zh" | "zh-cn" | "zh-hans" | "zh-chs" => "zh-CN".into(),
        "zh-tw" | "zh-hant" | "zh-cht" => "zh-TW".into(),
        "en" | "en-us" | "en-gb" => "en".into(),
        "ja" | "ja-jp" => "ja".into(),
        "ko" | "ko-kr" => "ko".into(),
        "fr" | "fr-fr" => "fr".into(),
        "de" | "de-de" => "de".into(),
        "es" | "es-es" => "es".into(),
        "ru" | "ru-ru" => "ru".into(),
        "auto" => "auto".into(),
        _ => code.to_string(),
    }
}
