//! 主题：颜色与字体。
//!
//! - 自绘视图用 [`palette`]（随 `WindowAppearance`）。
//! - GPUI Kit 组件（Settings / Button / Switch…）读 `Theme` Global。
//!   启动与窗口外观变化时需调用 [`sync_kit_theme`]，否则 Kit 默认浅色，
//!   会和弹窗等自绘深色界面不一致。

use gpui_kit::component::Theme;
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
    /// 开关关闭时的滑轨（开启用 accent / primary）
    pub switch_off: Hsla,
}

/// 把 GPUI Kit [`Theme`] 同步到窗口外观，并用本应用 [`Palette`] 覆盖语义色，
/// 使 Settings / Button 等与翻译弹窗视觉一致（半透明卡片、同色强调色）。
pub fn sync_kit_theme(window: Option<&mut Window>, cx: &mut App) {
    let appearance = window
        .as_ref()
        .map(|w| w.appearance())
        .unwrap_or_else(|| cx.window_appearance());
    // 先切 Light/Dark（会重载默认主题），再叠 Palette（需第二次 update）。
    Theme::sync_system_appearance(window, cx);
    apply_palette_to_theme(&palette(appearance), cx);
}

/// 设置窗口用实色 Kit 主题，避免毛玻璃透底。
pub fn sync_kit_theme_opaque(window: Option<&mut Window>, cx: &mut App) {
    let appearance = window
        .as_ref()
        .map(|w| w.appearance())
        .unwrap_or_else(|| cx.window_appearance());
    Theme::sync_system_appearance(window, cx);
    apply_palette_to_theme(&palette(appearance), cx);
    let panel = opaque_panel(appearance);
    let chip = opaque_chip(appearance);
    let elev = opaque_elevated(appearance);
    Theme::update(cx, |theme| {
        let c = &mut theme.colors;
        c.background = panel;
        c.popover = elev;
        c.input = chip;
        c.group_box = chip;
        c.button = chip;
        c.secondary = chip;
        c.sidebar = panel;
        c.title_bar = panel;
        c.list = panel;
        c.list_even = panel;
        c.muted = chip;
    });
}

/// 将弹窗配色写入 Kit Theme，供 Settings 等组件共用。
pub fn apply_palette_to_theme(p: &Palette, cx: &mut App) {
    let on_accent = hsla(0., 0., 1., 1.);
    Theme::update(cx, |theme| {
        // 经 DerefMut → ThemeColor；勿写 theme.list（会撞到 ListSettings 字段）。
        let c = &mut theme.colors;
        c.background = p.card;
        c.foreground = p.text;
        c.border = p.border;
        c.muted = p.tab_inactive;
        c.muted_foreground = p.muted;
        c.accent = p.tab_active;
        c.accent_foreground = p.text;
        c.primary = p.accent;
        c.primary_foreground = on_accent;
        c.secondary = p.input_bg;
        c.secondary_foreground = p.text;
        c.danger = p.danger;
        c.danger_foreground = on_accent;
        c.popover = p.card;
        c.popover_foreground = p.text;
        c.input = p.input_bg;
        c.ring = p.accent;
        c.group_box = p.input_bg;
        c.group_box_foreground = p.text;
        c.button = p.input_bg;
        c.button_foreground = p.text;
        c.button_hover = p.hover;
        c.button_active = p.tab_active;
        c.button_primary = p.accent;
        c.button_primary_hover = p.accent_hover;
        c.button_primary_active = p.accent_hover;
        c.button_primary_foreground = on_accent;
        c.button_danger = p.danger;
        c.button_danger_hover = p.danger;
        c.button_danger_active = p.danger;
        c.button_danger_foreground = on_accent;
        c.button_secondary = p.tab_inactive;
        c.button_secondary_hover = p.hover;
        c.button_secondary_active = p.tab_active;
        c.button_secondary_foreground = p.text;
        c.list = transparent_black();
        c.list_even = transparent_black();
        c.list_hover = p.hover;
        c.list_active = p.tab_active;
        c.list_active_border = p.accent;
        c.list_head = p.tab_inactive;
        c.sidebar = transparent_black();
        c.sidebar_foreground = p.text;
        c.sidebar_border = p.border;
        c.sidebar_accent = p.tab_active;
        c.sidebar_accent_foreground = p.text;
        c.sidebar_primary = p.accent;
        c.sidebar_primary_foreground = on_accent;
        c.tab_bar = transparent_black();
        c.tab_active = p.tab_active;
        c.tab_active_foreground = p.text;
        c.tab_foreground = p.muted;
        c.title_bar = p.card;
        c.title_bar_border = p.border;
        c.accordion = transparent_black();
        c.caret = p.accent;
        // 关闭态滑轨；开启态走 primary（accent），必须和这里区分开
        c.switch = p.switch_off;
        c.switch_thumb = hsla(0., 0., 1., 1.);
        c.tab = transparent_black();
        // 与弹窗圆角接近
        theme.font_family = ".SystemUIFont".into();
        theme.radius = px(6.);
        theme.radius_lg = px(crate::platform::CORNER_RADIUS as f32);
        theme.shadow = false;
    });
}

pub fn palette(appearance: WindowAppearance) -> Palette {
    let (light_a, dark_a) = if cfg!(target_os = "macos") {
        (0.28, 0.22)
    } else {
        (0.92, 0.88)
    };
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
            switch_off: hsla(0., 0., 0.42, 0.95),
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
            switch_off: hsla(0., 0., 0.78, 1.),
        },
    }
}

/// 不透明面板底（设置窗口；不要毛玻璃）。
pub fn opaque_panel(appearance: WindowAppearance) -> Hsla {
    match appearance {
        WindowAppearance::Dark | WindowAppearance::VibrantDark => hsla(0., 0., 0.14, 1.),
        _ => hsla(0., 0., 0.97, 1.),
    }
}

/// 浮层卡片（添加服务弹窗）。
pub fn opaque_elevated(appearance: WindowAppearance) -> Hsla {
    match appearance {
        WindowAppearance::Dark | WindowAppearance::VibrantDark => hsla(0., 0., 0.18, 1.),
        _ => hsla(0., 0., 1., 1.),
    }
}

/// 不透明胶囊底。
pub fn opaque_chip(appearance: WindowAppearance) -> Hsla {
    match appearance {
        WindowAppearance::Dark | WindowAppearance::VibrantDark => hsla(0., 0., 0.24, 1.),
        _ => hsla(0., 0., 0.94, 1.),
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
