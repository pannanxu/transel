//! 设置面板：左侧导航 + 服务分栏（参考系统设置风格）。
//!
//! 草稿配置放在 [`SettingsDraft`] Global；点「保存」才落盘并 `reload_config`。

use std::collections::BTreeMap;
use std::sync::Arc;

type ToggleFn = Arc<dyn Fn(bool, &mut App)>;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::group_box::GroupBoxVariant;
use gpui_kit::component::h_flex;
use gpui_kit::component::input::{Input, InputContentType, InputEvent, InputState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::setting::{
    NumberFieldOptions, RenderOptions, SettingField, SettingGroup, SettingItem, SettingPage,
    Settings,
};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::v_flex;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, IconName, Sizable as _, Size};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::app::config::{BackendEntry, BackendKind, Config, HotkeySpec};
use crate::app::openai_presets::{OPENAI_PRESETS, OpenAiPreset};
use crate::app::paths;
use crate::app::state::AppState;
use crate::services::ocr::{self, OcrBackendKind};
use crate::shared::actions::{CloseWindow, UTILITY_KEY_CONTEXT};
use crate::shared::icons;
use crate::shared::theme::{
    LANGUAGES, opaque_chip, opaque_elevated, opaque_panel, palette, sync_kit_theme_opaque,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NavPage {
    Translate,
    Services,
    Ocr,
    Hotkeys,
    About,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ServiceTab {
    Translate,
    Ocr,
    Tts,
}

/// 状态消息的级别。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusKind {
    /// 保存成功等正面反馈
    Success,
    /// 操作提示、进行中
    Info,
    /// 格式错误、保存失败等
    Error,
}

/// 带级别的状态消息。
#[derive(Clone, Debug)]
pub(crate) struct StatusMsg {
    pub(crate) id: u64,
    pub(crate) kind: StatusKind,
    pub(crate) text: String,
}

/// 设置窗口打开期间的草稿（未保存）。
pub struct SettingsDraft {
    pub cfg: Config,
    pub secret_inputs: BTreeMap<String, String>,
    pub(crate) status: Option<StatusMsg>,
    /// 快捷键的原始编辑字符串；键为快捷键名（如 "划词翻译"），值为用户输入的原始文本。
    /// 在输入过程中暂存，保存时才解析回 `HotkeySpec`。
    pub hotkey_edits: BTreeMap<String, String>,
}

impl Global for SettingsDraft {}

fn draft(cx: &App) -> &SettingsDraft {
    cx.global::<SettingsDraft>()
}

fn draft_mut(cx: &mut App) -> &mut SettingsDraft {
    cx.global_mut::<SettingsDraft>()
}

fn set_status(kind: StatusKind, msg: impl Into<String>, cx: &mut App) {
    static NEXT_STATUS_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = NEXT_STATUS_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    draft_mut(cx).status = Some(StatusMsg {
        id,
        kind,
        text: msg.into(),
    });

    let auto_dismiss_secs = match kind {
        StatusKind::Success => Some(3),
        StatusKind::Info => Some(4),
        StatusKind::Error => None,
    };

    if let Some(secs) = auto_dismiss_secs {
        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(secs))
                .await;
            let _ = cx.update(|cx| {
                if cx.has_global::<SettingsDraft>() {
                    let draft = draft_mut(cx);
                    if draft.status.as_ref().map(|s| s.id) == Some(id) {
                        draft.status = None;
                        cx.refresh_windows();
                    }
                }
            });
        })
        .detach();
    }
}

#[derive(Clone)]
pub struct SettingsActions {
    pub on_close: Arc<dyn Fn() + Send + Sync>,
    pub on_save: Arc<dyn Fn(Config) + Send + Sync>,
    pub on_toggle_backend: Arc<dyn Fn(String) + Send + Sync>,
    pub on_clear_history: Arc<dyn Fn() + Send + Sync>,
    pub on_secret_input: Arc<dyn Fn(String) + Send + Sync>,
}

impl Default for SettingsActions {
    fn default() -> Self {
        Self {
            on_close: Arc::new(|| {}),
            on_save: Arc::new(|_| {}),
            on_toggle_backend: Arc::new(|_| {}),
            on_clear_history: Arc::new(|| {}),
            on_secret_input: Arc::new(|_| {}),
        }
    }
}

pub struct SettingsView {
    pub actions: SettingsActions,
    nav: NavPage,
    service_tab: ServiceTab,
    selected_id: Option<String>,
    picker_open: bool,
    focus: FocusHandle,
    _appearance: Subscription,
}

impl SettingsView {
    pub fn new(
        cfg: Config,
        actions: SettingsActions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // Settings 组件读 Kit Theme；与窗口外观对齐，避免顶栏深色、内容浅色。
        sync_kit_theme_opaque(Some(window), cx);
        if !cx.has_global::<SettingsDraft>() {
            cx.set_global(SettingsDraft {
                cfg,
                secret_inputs: BTreeMap::new(),
                status: None,
                hotkey_edits: BTreeMap::new(),
            });
        }
        let appearance = cx.observe_window_appearance(window, |_, window, cx| {
            sync_kit_theme_opaque(Some(window), cx);
            cx.notify();
        });
        let focus = cx.focus_handle().tab_stop(true);
        focus.focus(window, cx);
        let selected_id = cx
            .global::<SettingsDraft>()
            .cfg
            .backends
            .iter()
            .find(|b| b.kind != BackendKind::Tencent)
            .map(|b| b.id.clone());
        Self {
            actions,
            nav: NavPage::Services,
            service_tab: ServiceTab::Translate,
            selected_id,
            picker_open: false,
            focus,
            _appearance: appearance,
        }
    }

    fn close(&mut self, _: &CloseWindow, window: &mut Window, cx: &mut Context<Self>) {
        (self.actions.on_close)();
        if cx.has_global::<AppState>() {
            *cx.global_mut::<AppState>().settings_window.lock() = None;
        }
        window.remove_window();
    }

    fn form_page(&self, view: Entity<Self>, _cx: &App) -> Option<SettingPage> {
        match self.nav {
            NavPage::Translate => Some(self.page_general(view)),
            NavPage::Ocr => Some(self.page_ocr(view)),
            NavPage::Hotkeys => Some(self.page_hotkeys(view)),
            NavPage::About => Some(self.page_about()),
            NavPage::Services => None,
        }
    }

    fn page_general(&self, _view: Entity<Self>) -> SettingPage {
        let lang_opts: Vec<(SharedString, SharedString)> = LANGUAGES
            .iter()
            .filter(|(c, _, _)| *c != "auto")
            .map(|(code, _, label)| ((*code).into(), (*label).into()))
            .collect();

        SettingPage::new("翻译设置")
            .icon(IconName::Settings2)
            .resettable(false)
            .groups(vec![
                SettingGroup::new().title("翻译").items(vec![
                    SettingItem::new(
                        "目标语言",
                        SettingField::scrollable_dropdown(
                            lang_opts,
                            |cx| draft(cx).cfg.target_lang.clone().into(),
                            |val, cx| {
                                draft_mut(cx).cfg.target_lang = val.to_string();
                                set_status(
                                    StatusKind::Info,
                                    format!("目标语言：{val}（保存后生效）"),
                                    cx,
                                );
                            },
                        ),
                    )
                    .description("检测到源 = 目标时，可配合「自动交换」反向翻译。"),
                    SettingItem::new(
                        "自动交换",
                        SettingField::switch(
                            |cx| draft(cx).cfg.auto_swap,
                            |val, cx| draft_mut(cx).cfg.auto_swap = val,
                        )
                        .default_value(true),
                    )
                    .description("源语言与目标语言相同时自动对调。"),
                ]),
                SettingGroup::new().title("弹窗").items(vec![
                    SettingItem::new(
                        "默认固定弹窗",
                        SettingField::switch(
                            |cx| draft(cx).cfg.pin_popup_by_default,
                            |val, cx| draft_mut(cx).cfg.pin_popup_by_default = val,
                        )
                        .default_value(false),
                    )
                    .description("打开翻译窗时默认钉住，失焦不关闭。"),
                    SettingItem::new(
                        "弹窗跟随鼠标",
                        SettingField::switch(
                            |cx| draft(cx).cfg.popup_follow_mouse,
                            |val, cx| draft_mut(cx).cfg.popup_follow_mouse = val,
                        )
                        .default_value(false),
                    )
                    .description("关闭时出现在屏幕中上方。"),
                ]),
                SettingGroup::new().title("系统").items(vec![
                    SettingItem::new(
                        "开机自启动",
                        SettingField::switch(
                            |cx| draft(cx).cfg.auto_start,
                            |val, cx| draft_mut(cx).cfg.auto_start = val,
                        )
                        .default_value(false),
                    )
                    .description("开机或登录系统时自动启动 Transel。")
                    .keywords(["自启", "开机", "启动", "系统", "autostart", "startup"]),
                ]),
                SettingGroup::new().title("历史").items(vec![
                    SettingItem::new(
                        "历史最大条数",
                        SettingField::number_input(
                            NumberFieldOptions {
                                min: 0.0,
                                max: 10_000.0,
                                ..Default::default()
                            },
                            |cx| draft(cx).cfg.history_limit as f64,
                            |val, cx| {
                                draft_mut(cx).cfg.history_limit = val.max(0.0) as usize;
                            },
                        )
                        .default_value(200.0),
                    )
                    .description("0 表示不限制。"),
                    SettingItem::render({
                        let view = _view.clone();
                        let clear = self.actions.on_clear_history.clone();
                        move |options, _, _cx| {
                            Button::new("clear_hist")
                                .danger()
                                .label("清空历史")
                                .with_size(options.size())
                                .on_click({
                                    let clear = clear.clone();
                                    let view = view.clone();
                                    move |_, _, cx| {
                                        clear();
                                        set_status(StatusKind::Success, "历史已清空", cx);
                                        view.update(cx, |_, cx| cx.notify());
                                    }
                                })
                        }
                    })
                    .keywords(["清空", "历史", "数据"]),
                ]),
            ])
    }

    fn page_ocr(&self, view: Entity<Self>) -> SettingPage {
        SettingPage::new("OCR 设置")
            .icon(IconName::Eye)
            .resettable(false)
            .groups(vec![SettingGroup::new()
                .title("识别")
                .items(vec![
                    SettingItem::new(
                        "连续识别",
                        SettingField::switch(
                            |cx| draft(cx).cfg.ocr_continuous,
                            |val, cx| draft_mut(cx).cfg.ocr_continuous = val,
                        )
                        .default_value(false),
                    )
                    .description("新结果追加到 OCR 窗口。"),
                    SettingItem::new(
                        "自动复制",
                        SettingField::switch(
                            |cx| draft(cx).cfg.ocr_auto_copy,
                            |val, cx| draft_mut(cx).cfg.ocr_auto_copy = val,
                        )
                        .default_value(true),
                    ),
                    SettingItem::new(
                        "智能分段",
                        SettingField::switch(
                            |cx| draft(cx).cfg.ocr_smart_paragraph,
                            |val, cx| draft_mut(cx).cfg.ocr_smart_paragraph = val,
                        )
                        .default_value(true),
                    )
                    .description("按段落合并断行。"),
                    SettingItem::render({
                        let view = view.clone();
                        move |options, _, cx| ort_download_row(options, view.clone(), cx)
                    })
                    .keywords(["ONNX", "OCR", "下载", "引擎"]),
                ])
                .footer(|_, _| {
                    "引擎在「服务 → 文本识别」中选择。本地 RapidOCR 需先下载 ONNX Runtime。"
                })])
    }

    fn page_hotkeys(&self, view: Entity<Self>) -> SettingPage {
        type HotkeyGetter = fn(&Config) -> &HotkeySpec;
        let items: Vec<(&'static str, HotkeyGetter)> = vec![
            ("划词翻译", |c| &c.translate_hotkey),
            ("截图翻译", |c| &c.ocr_hotkey),
            ("截图 OCR", |c| &c.ocr_only_hotkey),
            ("静默截图 OCR", |c| &c.ocr_silent_hotkey),
            (
                if cfg!(target_os = "macos") {
                    "访达选图 OCR"
                } else if cfg!(target_os = "windows") {
                    "资源管理器选图 OCR"
                } else {
                    "选中文件 OCR"
                },
                |c| &c.ocr_finder_hotkey,
            ),
            ("静默划词替换", |c| &c.silent_replace_hotkey),
            ("静默输入框替换", |c| &c.silent_input_hotkey),
            ("原图翻译", |c| &c.overlay_hotkey),
            ("收藏夹", |c| &c.favorites_hotkey),
            ("输入翻译窗口", |c| &c.input_hotkey),
            ("朗读选中文本", |c| &c.speak_hotkey),
            ("切换弹窗固定", |c| &c.pin_hotkey),
            ("显示历史", |c| &c.history_hotkey),
            ("显示设置", |c| &c.settings_hotkey),
        ];

        let hotkey_items: Vec<SettingItem> = items
            .into_iter()
            .map(|(label, getter)| {
                let label_key: String = label.into();
                let label_key2 = label_key.clone();
                SettingItem::new(
                    label,
                    SettingField::input(
                        move |cx| {
                            // 优先返回正在编辑的原始字符串，否则从配置读取
                            let d = draft(cx);
                            if let Some(raw) = d.hotkey_edits.get(&label_key) {
                                SharedString::from(raw.clone())
                            } else {
                                SharedString::from(getter(&d.cfg).to_string())
                            }
                        },
                        move |val, cx| {
                            // 将用户输入的原始字符串暂存，不立即解析
                            draft_mut(cx)
                                .hotkey_edits
                                .insert(label_key2.clone(), val.to_string());
                        },
                    ),
                )
                .description("格式：修饰键+按键，如 Alt+D、Cmd+Shift+P。留空表示不绑定。")
            })
            .collect();

        let _ = view;
        SettingPage::new("快捷键")
            .icon(IconName::BookOpen)
            .resettable(false)
            .group(
                SettingGroup::new()
                    .title("全局热键")
                    .items(hotkey_items)
                    .footer(|_, _| "修改热键后需点击保存生效。"),
            )
    }

    fn page_about(&self) -> SettingPage {
        let version = env!("CARGO_PKG_VERSION").to_string();
        let config_path = paths::config_file().display().to_string();
        let history_path = paths::history_file().display().to_string();

        SettingPage::new("关于")
            .icon(IconName::Info)
            .resettable(false)
            .group(SettingGroup::new().items(vec![
                SettingItem::render(move |_, _, cx| {
                    let muted = cx.theme().muted_foreground;
                    v_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(cx.theme().foreground)
                                .child(format!("Transel {version}")),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child("基于 GPUI + gpui-kit（macOS / Windows / Linux）"),
                        )
                }),
                SettingItem::new(
                    "配置文件",
                    SettingField::input(
                        {
                            let p = config_path.clone();
                            move |_| SharedString::from(p.clone())
                        },
                        |_, _| {},
                    ),
                )
                .layout(Axis::Vertical),
                SettingItem::new(
                    "历史文件",
                    SettingField::input(
                        {
                            let p = history_path.clone();
                            move |_| SharedString::from(p.clone())
                        },
                        |_, _| {},
                    ),
                )
                .layout(Axis::Vertical),
            ]))
    }
}

fn status_colors(
    kind: StatusKind,
    appearance: gpui::WindowAppearance,
    p: &crate::shared::theme::Palette,
) -> (Hsla, Hsla, Hsla, &'static str) {
    let dark = matches!(appearance, gpui::WindowAppearance::Dark);
    match kind {
        StatusKind::Success => {
            let text = if dark {
                hsla(142. / 360., 0.75, 0.55, 1.)
            } else {
                hsla(142. / 360., 0.85, 0.30, 1.)
            };
            let bg = if dark {
                hsla(142. / 360., 0.70, 0.35, 0.18)
            } else {
                hsla(142. / 360., 0.70, 0.40, 0.12)
            };
            let border = if dark {
                hsla(142. / 360., 0.70, 0.50, 0.35)
            } else {
                hsla(142. / 360., 0.75, 0.35, 0.30)
            };
            (text, bg, border, icons::CHECK)
        }
        StatusKind::Error => {
            let text = p.danger;
            let bg = Hsla {
                a: if dark { 0.20 } else { 0.12 },
                ..p.danger
            };
            let border = Hsla {
                a: if dark { 0.45 } else { 0.35 },
                ..p.danger
            };
            (text, bg, border, icons::CLOSE)
        }
        StatusKind::Info => {
            let text = p.accent;
            let bg = Hsla {
                a: if dark { 0.18 } else { 0.10 },
                ..p.accent
            };
            let border = Hsla {
                a: if dark { 0.40 } else { 0.28 },
                ..p.accent
            };
            (text, bg, border, icons::SPARKLES)
        }
    }
}

/// 底部操作栏（如“保存”按钮旁边）的状态指示徽标。
fn status_badge(
    st: &StatusMsg,
    appearance: gpui::WindowAppearance,
    p: &crate::shared::theme::Palette,
) -> AnyElement {
    let (text_color, bg_color, border_color, icon_path) = status_colors(st.kind, appearance, p);
    h_flex()
        .items_center()
        .gap_2()
        .px_3()
        .py(px(5.))
        .rounded(px(8.))
        .bg(bg_color)
        .border_1()
        .border_color(border_color)
        .child(svg().path(icon_path).size(px(14.)).text_color(text_color))
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(text_color)
                .child(st.text.clone()),
        )
        .into_any_element()
}

/// 窗口顶部居中浮动的全局 Toast 提示卡片。
fn floating_status_toast(
    st: &StatusMsg,
    appearance: gpui::WindowAppearance,
    p: &crate::shared::theme::Palette,
    view: Entity<SettingsView>,
) -> AnyElement {
    let (text_color, bg_color, border_color, icon_path) = status_colors(st.kind, appearance, p);
    let card_bg = opaque_elevated(appearance);

    div()
        .id("settings_floating_toast")
        .absolute()
        .top(px(12.))
        .left_0()
        .right_0()
        .flex()
        .justify_center()
        .child(
            h_flex()
                .items_center()
                .gap_2()
                .px_4()
                .py(px(8.))
                .rounded(px(12.))
                .bg(card_bg)
                .border_1()
                .border_color(border_color)
                .shadow_lg()
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    if cx.has_global::<SettingsDraft>() {
                        draft_mut(cx).status = None;
                    }
                    view.update(cx, |_, cx| cx.notify());
                })
                .child(
                    div()
                        .p(px(3.))
                        .rounded(px(6.))
                        .bg(bg_color)
                        .child(svg().path(icon_path).size(px(14.)).text_color(text_color)),
                )
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.text)
                        .child(st.text.clone()),
                )
                .child(
                    div()
                        .pl_2()
                        .child(
                            svg()
                                .path(icons::CLOSE)
                                .size(px(11.))
                                .text_color(p.muted),
                        ),
                ),
        )
        .into_any_element()
}

fn ort_download_row(options: &RenderOptions, view: Entity<SettingsView>, cx: &App) -> AnyElement {
    let label = ocr::ort_status_label();
    let st = ocr::ort_status();
    let can_download = matches!(st, ocr::OrtStatus::Missing | ocr::OrtStatus::Downloaded);
    let btn_label = match st {
        ocr::OrtStatus::Ready => "已就绪",
        ocr::OrtStatus::Downloaded => "加载引擎",
        ocr::OrtStatus::Missing => "下载本地 OCR 引擎",
        ocr::OrtStatus::Unsupported => "当前平台不可用",
    };
    let size = options.size();

    h_flex()
        .w_full()
        .items_center()
        .gap_2()
        .child(
            Button::new("ocr_download_ort")
                .when(can_download, |b| b.primary())
                .disabled(!can_download)
                .label(btn_label)
                .with_size(size)
                .on_click({
                    let view = view.clone();
                    move |_, _, cx| {
                        if !can_download {
                            return;
                        }
                        set_status(StatusKind::Info, "正在下载 ONNX Runtime…", cx);
                        view.update(cx, |_, cx| cx.notify());
                        let view2 = view.clone();
                        cx.spawn(async move |cx| {
                            let result = cx
                                .background_executor()
                                .spawn(async {
                                    ocr::download_ort_runtime().map(|p| p.display().to_string())
                                })
                                .await;
                            view2.update(cx, |_, cx| {
                                match result {
                                    Ok(path) => {
                                        let backend = draft(cx).cfg.ocr_backend.clone();
                                        std::thread::Builder::new()
                                            .name("ocr-warmup".into())
                                            .spawn(move || ocr::set_engine_from_config(&backend))
                                            .ok();
                                        set_status(
                                            StatusKind::Success,
                                            format!("ONNX Runtime 已就绪\n{path}"),
                                            cx,
                                        );
                                    }
                                    Err(e) => {
                                        set_status(
                                            StatusKind::Error,
                                            format!("下载失败: {e:#}"),
                                            cx,
                                        );
                                    }
                                }
                                cx.notify();
                            });
                        })
                        .detach();
                    }
                }),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(format!(
                    "状态：{label} · Microsoft ONNX Runtime {}（约数十 MB）",
                    ocr::ORT_VERSION
                )),
        )
        .into_any_element()
}

fn nav_sidebar(
    view: Entity<SettingsView>,
    nav: NavPage,
    p: &crate::shared::theme::Palette,
) -> AnyElement {
    v_flex()
        .id("settings_nav")
        .w(px(188.))
        .min_w(px(188.))
        .h_full()
        .px_2()
        .py_3()
        .gap_3()
        .border_r_1()
        .border_color(p.divider)
        .child(nav_section(
            "翻译",
            vec![
                ("翻译设置", NavPage::Translate, icons::SETTINGS),
                ("服务", NavPage::Services, icons::GLOBE),
            ],
            nav,
            view.clone(),
            p,
        ))
        .child(nav_section(
            "OCR",
            vec![("OCR 设置", NavPage::Ocr, icons::OCR)],
            nav,
            view.clone(),
            p,
        ))
        .child(nav_section(
            "通用",
            vec![
                ("快捷键", NavPage::Hotkeys, icons::BOOK),
                ("关于", NavPage::About, icons::SETTINGS),
            ],
            nav,
            view,
            p,
        ))
        .into_any_element()
}

fn nav_section(
    title: &'static str,
    items: Vec<(&'static str, NavPage, &'static str)>,
    current: NavPage,
    view: Entity<SettingsView>,
    p: &crate::shared::theme::Palette,
) -> AnyElement {
    v_flex()
        .w_full()
        .gap_1()
        .child(div().px_2().text_xs().text_color(p.muted).child(title))
        .children(items.into_iter().map(|(label, page, icon)| {
            let active = current == page;
            let view = view.clone();
            h_flex()
                .id(SharedString::from(format!("nav_{label}")))
                .w_full()
                .items_center()
                .gap_2()
                .px_2()
                .py(px(6.))
                .rounded(px(8.))
                .cursor_pointer()
                .when(active, |el| el.bg(p.tab_active))
                .when(!active, |el| el.hover(|s| s.bg(p.hover)))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    view.update(cx, |this, cx| {
                        this.nav = page;
                        this.picker_open = false;
                        cx.notify();
                    });
                })
                .child(svg().path(icon).size(px(14.)).text_color(if active {
                    p.text
                } else {
                    p.muted
                }))
                .child(div().text_sm().text_color(p.text).child(label))
                .into_any_element()
        }))
        .into_any_element()
}

fn services_workspace(
    view: Entity<SettingsView>,
    actions: SettingsActions,
    tab: ServiceTab,
    selected: Option<String>,
    options: &RenderOptions,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let p = palette(window.appearance());
    let fill = opaque_chip(window.appearance());
    v_flex()
        .w_full()
        .h_full()
        .gap_3()
        .child(
            h_flex().w_full().items_start().gap_3().child(
                v_flex()
                    .flex_1()
                    .min_w(px(0.))
                    .gap_1()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.title)
                            .child("服务"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(p.muted)
                            .child("翻译功能的核心服务支持配置，下方开启的服务将被使用。"),
                    ),
            ),
        )
        .child(service_tabs(view.clone(), tab, &p))
        .child(
            h_flex()
                .w_full()
                .flex_1()
                .min_h(px(0.))
                .items_stretch()
                .gap_3()
                .child(service_list_panel(
                    view.clone(),
                    actions.clone(),
                    tab,
                    selected.as_deref(),
                    &p,
                    fill,
                    cx,
                ))
                .child(div().flex_1().min_w(px(0.)).h_full().child(match tab {
                    ServiceTab::Translate => {
                        if let Some(id) = selected {
                            backend_edit(id, view, actions, options, window, cx)
                        } else {
                            empty_detail(&p, fill, "选择左侧服务查看配置，或点 + 添加。")
                        }
                    }
                    ServiceTab::Ocr => ocr_detail(view, actions, options, window, cx),
                    ServiceTab::Tts => tts_detail(view, actions, &p, fill, window.appearance(), cx),
                })),
        )
        .into_any_element()
}

fn service_tabs(
    view: Entity<SettingsView>,
    tab: ServiceTab,
    p: &crate::shared::theme::Palette,
) -> AnyElement {
    h_flex()
        .items_center()
        .rounded(px(10.))
        .bg(p.hover)
        .p(px(3.))
        .gap_0()
        .child(segment_tab(
            "tab_tr",
            "文本翻译",
            tab == ServiceTab::Translate,
            p,
            {
                let view = view.clone();
                move |cx| {
                    view.update(cx, |this, cx| {
                        this.service_tab = ServiceTab::Translate;
                        this.picker_open = false;
                        cx.notify();
                    });
                }
            },
        ))
        .child(div().w(px(1.)).h(px(12.)).bg(p.divider))
        .child(segment_tab(
            "tab_ocr",
            "文本识别",
            tab == ServiceTab::Ocr,
            p,
            {
                let view = view.clone();
                move |cx| {
                    view.update(cx, |this, cx| {
                        this.service_tab = ServiceTab::Ocr;
                        this.picker_open = false;
                        cx.notify();
                    });
                }
            },
        ))
        .child(div().w(px(1.)).h(px(12.)).bg(p.divider))
        .child(segment_tab(
            "tab_tts",
            "语音合成",
            tab == ServiceTab::Tts,
            p,
            {
                let view = view.clone();
                move |cx| {
                    view.update(cx, |this, cx| {
                        this.service_tab = ServiceTab::Tts;
                        this.picker_open = false;
                        cx.notify();
                    });
                }
            },
        ))
        .into_any_element()
}

fn segment_tab(
    id: &'static str,
    label: &'static str,
    active: bool,
    p: &crate::shared::theme::Palette,
    on_click: impl Fn(&mut App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .px_3()
        .py_1()
        .rounded(px(8.))
        .cursor_pointer()
        .when(active, |el| {
            el.bg(p.accent).text_color(hsla(0., 0., 1., 1.))
        })
        .when(!active, |el| {
            el.bg(transparent_black())
                .text_color(p.muted)
                .hover(|s| s.bg(p.hover))
        })
        .text_sm()
        .font_weight(if active {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::NORMAL
        })
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            cx.stop_propagation();
            on_click(cx);
        })
        .child(label)
        .into_any_element()
}

fn empty_detail(p: &crate::shared::theme::Palette, fill: Hsla, msg: &str) -> AnyElement {
    div()
        .w_full()
        .h_full()
        .rounded(px(12.))
        .bg(fill)
        .p_4()
        .min_h(px(280.))
        .child(div().text_sm().text_color(p.muted).child(msg.to_string()))
        .into_any_element()
}

fn service_list_panel(
    view: Entity<SettingsView>,
    actions: SettingsActions,
    tab: ServiceTab,
    selected: Option<&str>,
    p: &crate::shared::theme::Palette,
    fill: Hsla,
    cx: &App,
) -> AnyElement {
    let rows = match tab {
        ServiceTab::Translate => {
            translate_list_rows(view.clone(), actions.clone(), selected, p, cx)
        }
        ServiceTab::Ocr => ocr_list_rows(view.clone(), actions.clone(), p, cx),
        ServiceTab::Tts => tts_list_rows(view.clone(), actions.clone(), p, cx),
    };
    let can_add = tab == ServiceTab::Translate;
    let can_remove = tab == ServiceTab::Translate && selected.is_some();
    v_flex()
        .w(px(248.))
        .min_w(px(248.))
        .h_full()
        .rounded(px(12.))
        .bg(fill)
        .overflow_hidden()
        .child(
            div()
                .id("svc_list_scroll")
                .w_full()
                .flex_1()
                .min_h(px(0.))
                .overflow_y_scrollbar()
                .p_1()
                .child(v_flex().w_full().gap(px(1.)).children(rows)),
        )
        .child(
            h_flex()
                .w_full()
                .items_center()
                .border_t_1()
                .border_color(p.divider)
                .child(list_tool_label("svc_add", "+", can_add, p, {
                    let view = view.clone();
                    move |cx| {
                        view.update(cx, |this, cx| {
                            this.picker_open = true;
                            cx.notify();
                        });
                    }
                }))
                .child(div().w(px(1.)).h(px(16.)).bg(p.divider))
                .child(list_tool_label("svc_del", "−", can_remove, p, {
                    let view = view.clone();
                    let on_save = actions.on_save.clone();
                    let id = selected.map(str::to_string);
                    move |cx| {
                        let Some(id) = id.clone() else {
                            return;
                        };
                        draft_mut(cx).cfg.backends.retain(|b| b.id != id);
                        draft_mut(cx).secret_inputs.remove(&id);
                        persist_deleted_backend(&id, &on_save, cx);
                        view.update(cx, |this, cx| {
                            this.selected_id = draft(cx)
                                .cfg
                                .backends
                                .iter()
                                .find(|b| b.kind != BackendKind::Tencent)
                                .map(|b| b.id.clone());
                            cx.notify();
                        });
                    }
                })),
        )
        .into_any_element()
}

fn list_tool_label(
    id: &'static str,
    label: &'static str,
    enabled: bool,
    p: &crate::shared::theme::Palette,
    on_click: impl Fn(&mut App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .flex_1()
        .h(px(32.))
        .flex()
        .items_center()
        .justify_center()
        .when(enabled, |el| {
            el.cursor_pointer().hover(|s| s.bg(p.hover)).on_mouse_down(
                MouseButton::Left,
                move |_, _, cx| {
                    cx.stop_propagation();
                    on_click(cx);
                },
            )
        })
        .child(
            div()
                .text_sm()
                .text_color(if enabled { p.text } else { p.muted })
                .child(label),
        )
        .into_any_element()
}

fn translate_list_rows(
    view: Entity<SettingsView>,
    actions: SettingsActions,
    selected: Option<&str>,
    p: &crate::shared::theme::Palette,
    cx: &App,
) -> Vec<AnyElement> {
    let mut rows = Vec::new();
    for b in draft(cx).cfg.backends.iter() {
        if b.kind == BackendKind::Tencent {
            continue;
        }
        let id = b.id.clone();
        let name = if b.display_name.is_empty() {
            kind_label(&b.kind).to_string()
        } else {
            b.display_name.clone()
        };
        let builtin = !b.kind.needs_key();
        let enabled = b.enabled;
        let active = selected == Some(id.as_str());
        let icon = icons::channel_icon(&name);
        let on_toggle = actions.on_toggle_backend.clone();
        rows.push(service_row(
            format!("be_row_{id}"),
            icon,
            name,
            if builtin { "内置" } else { "密钥" },
            enabled,
            active,
            p,
            {
                let view = view.clone();
                let id = id.clone();
                move |cx| {
                    view.update(cx, |this, cx| {
                        this.selected_id = Some(id.clone());
                        cx.notify();
                    });
                }
            },
            Some({
                let id = id.clone();
                let view = view.clone();
                let on_save = actions.on_save.clone();
                Arc::new(move |checked: bool, cx: &mut App| {
                    view.update(cx, |_, cx| {
                        if let Some(b) = draft_mut(cx).cfg.backends.iter_mut().find(|b| b.id == id)
                        {
                            b.enabled = checked;
                        }
                        save_backend_enabled(&id, checked, &on_save, cx);
                        on_toggle(id.clone());
                        cx.notify();
                    });
                }) as ToggleFn
            }),
        ));
    }
    if rows.is_empty() {
        rows.push(
            div()
                .px_2()
                .py_3()
                .text_xs()
                .text_color(p.muted)
                .child("还没有翻译服务。点 + 添加。")
                .into_any_element(),
        );
    }
    rows
}

fn ocr_list_rows(
    view: Entity<SettingsView>,
    actions: SettingsActions,
    p: &crate::shared::theme::Palette,
    cx: &App,
) -> Vec<AnyElement> {
    let current = draft(cx).cfg.ocr_backend.clone();
    OcrBackendKind::available()
        .iter()
        .map(|kind| {
            let id = kind.as_str();
            let selected = current == id;
            let builtin = matches!(kind, OcrBackendKind::Auto | OcrBackendKind::RapidOcr);
            service_row(
                format!("ocr_row_{id}"),
                icons::OCR,
                kind.label().to_string(),
                if builtin { "内置" } else { "密钥" },
                selected,
                selected,
                p,
                {
                    let view = view.clone();
                    let on_save = actions.on_save.clone();
                    let id = id.to_string();
                    move |cx| {
                        view.update(cx, |_, cx| {
                            draft_mut(cx).cfg.ocr_backend = id.clone();
                            save_ocr_engine(&on_save, cx);
                            cx.notify();
                        });
                    }
                },
                Some({
                    let view = view.clone();
                    let on_save = actions.on_save.clone();
                    let id = id.to_string();
                    Arc::new(move |checked: bool, cx: &mut App| {
                        if !checked {
                            return;
                        }
                        view.update(cx, |_, cx| {
                            draft_mut(cx).cfg.ocr_backend = id.clone();
                            save_ocr_engine(&on_save, cx);
                            cx.notify();
                        });
                    }) as ToggleFn
                }),
            )
        })
        .collect()
}

fn tts_list_rows(
    view: Entity<SettingsView>,
    actions: SettingsActions,
    p: &crate::shared::theme::Palette,
    cx: &App,
) -> Vec<AnyElement> {
    let current = draft(cx).cfg.tts_backend.clone();
    tts_catalog()
        .into_iter()
        .map(|(id, label, builtin)| {
            let selected =
                current == id || (id == "system" && (current == "auto" || current.is_empty()));
            service_row(
                format!("tts_row_{id}"),
                icons::SPEAKER,
                label.to_string(),
                if builtin { "内置" } else { "密钥" },
                selected,
                selected,
                p,
                {
                    let view = view.clone();
                    let on_save = actions.on_save.clone();
                    let id = id.to_string();
                    move |cx| {
                        view.update(cx, |_, cx| {
                            draft_mut(cx).cfg.tts_backend = id.clone();
                            save_tts_engine(&on_save, cx);
                            cx.notify();
                        });
                    }
                },
                Some({
                    let view = view.clone();
                    let on_save = actions.on_save.clone();
                    let id = id.to_string();
                    Arc::new(move |checked: bool, cx: &mut App| {
                        if !checked {
                            return;
                        }
                        view.update(cx, |_, cx| {
                            draft_mut(cx).cfg.tts_backend = id.clone();
                            save_tts_engine(&on_save, cx);
                            cx.notify();
                        });
                    }) as ToggleFn
                }),
            )
        })
        .collect()
}

fn tts_catalog() -> Vec<(&'static str, &'static str, bool)> {
    vec![
        ("system", "系统语音", true),
        ("volcano", "火山语音合成", false),
        ("tencent", "腾讯语音合成", false),
        ("google", "Google 语音合成", false),
        ("microsoft", "Microsoft 语音合成", false),
    ]
}

#[allow(clippy::too_many_arguments)]
fn service_row(
    row_id: String,
    icon: &'static str,
    name: String,
    badge: &'static str,
    enabled: bool,
    selected: bool,
    p: &crate::shared::theme::Palette,
    on_select: impl Fn(&mut App) + 'static,
    on_toggle: Option<ToggleFn>,
) -> AnyElement {
    h_flex()
        .id(SharedString::from(row_id.clone()))
        .w_full()
        .items_center()
        .gap_2()
        .px_2()
        .py(px(6.))
        .rounded(px(8.))
        .cursor_pointer()
        .when(selected, |el| el.bg(p.tab_active))
        .when(!selected, |el| el.hover(|s| s.bg(p.hover)))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            cx.stop_propagation();
            on_select(cx);
        })
        .child(svg().path(icon).size(px(14.)).text_color(p.muted))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .text_sm()
                .text_color(p.text)
                .child(name),
        )
        .child(div().text_xs().text_color(p.muted).child(badge))
        .when_some(on_toggle, |el, on_toggle| {
            el.child(
                div()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        Switch::new(SharedString::from(format!("{row_id}_sw")))
                            .checked(enabled)
                            .color(p.accent)
                            .small()
                            .on_click(move |checked, _, cx| on_toggle(*checked, cx)),
                    ),
            )
        })
        .into_any_element()
}

fn ocr_detail(
    view: Entity<SettingsView>,
    actions: SettingsActions,
    options: &RenderOptions,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let p = palette(window.appearance());
    let kind = OcrBackendKind::parse(&draft(cx).cfg.ocr_backend);
    v_flex()
        .w_full()
        .h_full()
        .rounded(px(12.))
        .bg(opaque_chip(window.appearance()))
        .p_4()
        .gap_3()
        .min_h(px(280.))
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.title)
                .child(kind.label()),
        )
        .child(div().text_xs().text_color(p.muted).child(
            if matches!(kind, OcrBackendKind::Auto | OcrBackendKind::RapidOcr) {
                "本地识别。需先下载 ONNX Runtime。"
            } else {
                "云端识别，使用对应厂商密钥（环境变量或钥匙串）。"
            },
        ))
        .child(ort_download_row(options, view.clone(), cx))
        .child(div().flex_1())
        .child(page_save_button(
            "ocr_engine_save",
            view,
            actions,
            options,
            save_ocr_engine,
            &p,
            window.appearance(),
            cx,
        ))
        .into_any_element()
}

fn tts_detail(
    view: Entity<SettingsView>,
    actions: SettingsActions,
    p: &crate::shared::theme::Palette,
    fill: Hsla,
    appearance: gpui::WindowAppearance,
    cx: &App,
) -> AnyElement {
    v_flex()
        .w_full()
        .h_full()
        .rounded(px(12.))
        .bg(fill)
        .p_4()
        .gap_2()
        .min_h(px(280.))
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.title)
                .child("语音合成"),
        )
        .child(
            div()
                .text_xs()
                .text_color(p.muted)
                .child("左侧开关选择当前朗读引擎。云 TTS 使用对应厂商密钥。"),
        )
        .child(div().flex_1())
        .child(page_save_button(
            "tts_engine_save",
            view,
            actions,
            &RenderOptions::new().with_size(Size::Small),
            save_tts_engine,
            p,
            appearance,
            cx,
        ))
        .into_any_element()
}

fn page_save_button(
    id: &'static str,
    view: Entity<SettingsView>,
    actions: SettingsActions,
    options: &RenderOptions,
    save: impl Fn(&Arc<dyn Fn(Config) + Send + Sync>, &mut App) + 'static,
    p: &crate::shared::theme::Palette,
    appearance: gpui::WindowAppearance,
    cx: &App,
) -> AnyElement {
    let status = if cx.has_global::<SettingsDraft>() {
        draft(cx).status.clone()
    } else {
        None
    };

    h_flex()
        .w_full()
        .items_center()
        .justify_between()
        .child(
            if let Some(ref st) = status {
                status_badge(st, appearance, p)
            } else {
                div().into_any_element()
            },
        )
        .child(
            Button::new(id)
                .primary()
                .label("保存")
                .with_size(options.size())
                .on_click({
                    let view = view.clone();
                    let on_save = actions.on_save.clone();
                    move |_, _, cx| {
                        save(&on_save, cx);
                        view.update(cx, |_, cx| cx.notify());
                    }
                }),
        )
        .into_any_element()
}

#[derive(Clone, Copy)]
enum CatalogPick {
    Kind(BackendKind),
    Preset(&'static OpenAiPreset),
}

impl CatalogPick {
    fn key(self) -> SharedString {
        match self {
            Self::Kind(k) => format!("kind:{}", k.as_str()).into(),
            Self::Preset(p) => format!("preset:{}", p.id).into(),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Kind(BackendKind::Apple | BackendKind::GoogleFree) => "系统翻译",
            Self::Kind(BackendKind::Dictionary) => "简明英汉词典",
            Self::Kind(k) => k.default_display_name(),
            Self::Preset(p) => match p.id {
                "glm" => "智谱",
                "aihubmix" => "AiHubMix",
                "ai302" => "302AI",
                "qwen" => "千问",
                "ernie" => "文心",
                "lmstudio" => "LM Studio",
                _ => p.display_name,
            },
        }
    }
}

fn catalog_groups() -> Vec<(&'static str, Vec<CatalogPick>)> {
    let system = if cfg!(target_os = "macos") {
        BackendKind::Apple
    } else {
        BackendKind::GoogleFree
    };
    let preset = |id: &str| {
        OPENAI_PRESETS
            .iter()
            .find(|p| p.id == id)
            .map(CatalogPick::Preset)
            .expect("openai preset")
    };
    let builtin = vec![
        preset("glm"),
        preset("siliconflow"),
        CatalogPick::Kind(system),
        CatalogPick::Kind(BackendKind::Dictionary),
    ];
    let keyed = vec![
        preset("aihubmix"),
        preset("sophnet"),
        preset("ai302"),
        CatalogPick::Kind(BackendKind::Volcano),
        CatalogPick::Kind(BackendKind::Aliyun),
        CatalogPick::Kind(BackendKind::Baidu),
        CatalogPick::Kind(BackendKind::Youdao),
        CatalogPick::Kind(BackendKind::Caiyun),
        CatalogPick::Kind(BackendKind::Niutrans),
        CatalogPick::Kind(BackendKind::NiutransPro),
        CatalogPick::Kind(BackendKind::DeepL),
        CatalogPick::Kind(BackendKind::Google),
        CatalogPick::Kind(BackendKind::Microsoft),
        CatalogPick::Kind(BackendKind::Amazon),
        preset("openai"),
        preset("azure_openai"),
        CatalogPick::Kind(BackendKind::Gemini),
        CatalogPick::Kind(BackendKind::Claude),
        preset("grok"),
        preset("groq"),
        preset("openrouter"),
        preset("ollama"),
        preset("lmstudio"),
        preset("deepseek"),
        preset("qwen"),
        preset("ernie"),
        preset("doubao"),
        preset("hunyuan"),
        preset("ark"),
        preset("kimi"),
    ];
    vec![("内置", builtin), ("自带密钥", keyed)]
}

fn add_service_dialog(
    view: Entity<SettingsView>,
    window: &mut Window,
    _cx: &mut App,
) -> AnyElement {
    let p = palette(window.appearance());
    let mut sections: Vec<AnyElement> = Vec::new();
    for (title, picks) in catalog_groups() {
        let chips: Vec<AnyElement> = picks
            .into_iter()
            .map(|pick| catalog_chip(pick, view.clone(), &p, opaque_chip(window.appearance())))
            .collect();
        sections.push(
            v_flex()
                .w_full()
                .gap_2()
                .child(div().text_sm().text_color(p.muted).child(title))
                .child(h_flex().w_full().flex_wrap().gap_2().children(chips))
                .into_any_element(),
        );
    }

    div()
        .id("add_svc_overlay")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(hsla(0., 0., 0., 0.45))
        .on_mouse_down(MouseButton::Left, {
            let view = view.clone();
            move |_, _, cx| {
                view.update(cx, |this, cx| {
                    this.picker_open = false;
                    cx.notify();
                });
            }
        })
        .child(
            v_flex()
                .id("add_svc_sheet")
                .w(px(760.))
                .max_w(px(760.))
                .max_h(px(560.))
                .rounded(px(16.))
                .bg(opaque_elevated(window.appearance()))
                .border_1()
                .border_color(p.border)
                .overflow_hidden()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .id("be_picker_scroll")
                        .w_full()
                        .flex_1()
                        .min_h(px(0.))
                        .overflow_y_scrollbar()
                        .px_5()
                        .pt_4()
                        .pb_2()
                        .child(v_flex().w_full().gap_4().children(sections)),
                )
                .child(
                    h_flex()
                        .w_full()
                        .justify_end()
                        .px_5()
                        .py_3()
                        .border_t_1()
                        .border_color(p.divider)
                        .child(Button::new("add_svc_cancel").label("取消").on_click({
                            let view = view.clone();
                            move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.picker_open = false;
                                    cx.notify();
                                });
                            }
                        })),
                ),
        )
        .into_any_element()
}

fn catalog_chip(
    pick: CatalogPick,
    view: Entity<SettingsView>,
    p: &crate::shared::theme::Palette,
    chip_bg: Hsla,
) -> AnyElement {
    let label = pick.label();
    let icon = icons::channel_icon(label);
    h_flex()
        .id(pick.key())
        .w(px(168.))
        .h(px(36.))
        .items_center()
        .gap_2()
        .px_3()
        .rounded(px(10.))
        .cursor_pointer()
        .bg(chip_bg)
        .hover(|s| s.bg(p.accent).text_color(hsla(0., 0., 1., 1.)))
        .on_mouse_down(MouseButton::Left, {
            let view = view.clone();
            move |_, _, cx| {
                cx.stop_propagation();
                add_pick(pick, &view, cx);
            }
        })
        .child(svg().path(icon).size(px(14.)).text_color(p.text))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .text_sm()
                .text_color(p.text)
                .child(label),
        )
        .into_any_element()
}

fn next_display_name(base: &str, cx: &App) -> String {
    let used: Vec<String> = draft(cx)
        .cfg
        .backends
        .iter()
        .map(|b| {
            if b.display_name.is_empty() {
                kind_label(&b.kind).to_string()
            } else {
                b.display_name.clone()
            }
        })
        .collect();
    if !used.iter().any(|n| n == base) {
        return base.to_string();
    }
    for i in 2..100 {
        let candidate = format!("{base} {i}");
        if !used.iter().any(|n| n == &candidate) {
            return candidate;
        }
    }
    format!("{base} {}", used.len() + 1)
}

fn finish_add(mut entry: BackendEntry, view: &Entity<SettingsView>, cx: &mut App) {
    let name = next_display_name(&entry.display_name, cx);
    entry.display_name = name.clone();
    let id = entry.id.clone();
    draft_mut(cx).cfg.backends.push(entry);
    set_status(
        StatusKind::Info,
        format!("已添加 {name}，请填写配置后保存"),
        cx,
    );
    view.update(cx, |this, cx| {
        this.selected_id = Some(id);
        this.picker_open = false;
        this.service_tab = ServiceTab::Translate;
        cx.notify();
    });
}

fn add_pick(pick: CatalogPick, view: &Entity<SettingsView>, cx: &mut App) {
    match pick {
        CatalogPick::Kind(kind) => finish_add(BackendEntry::from_kind(kind), view, cx),
        CatalogPick::Preset(preset) => finish_add(preset.to_entry(true), view, cx),
    }
}

fn backend_edit(
    id: String,
    view: Entity<SettingsView>,
    actions: SettingsActions,
    options: &RenderOptions,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let p = palette(window.appearance());
    let Some(entry) = draft(cx).cfg.backends.iter().find(|b| b.id == id).cloned() else {
        return empty_detail(
            &p,
            opaque_chip(window.appearance()),
            "选择左侧服务查看配置，或点 + 添加。",
        );
    };
    let openai = entry.kind == BackendKind::OpenAiCompat;
    let has_key = entry.kind.needs_key();
    let key_hint = draft(cx)
        .secret_inputs
        .get(&id)
        .cloned()
        .unwrap_or_default();
    let has_key_ref = entry
        .params
        .get("key_ref")
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    let extra = extra_backend_fields(entry.kind);
    let display_name = if entry.display_name.is_empty() {
        kind_label(&entry.kind).to_string()
    } else {
        entry.display_name.clone()
    };

    v_flex()
        .w_full()
        .h_full()
        .rounded(px(12.))
        .bg(opaque_chip(window.appearance()))
        .p_4()
        .gap_3()
        .min_h(px(280.))
        .child(text_field_row(
            format!("be_name_{id}"),
            "服务名称",
            display_name,
            "显示名称",
            &p,
            id.clone(),
            "display_name",
            window,
            cx,
        ))
        .child(div().text_xs().text_color(p.muted).child(if has_key {
            "使用本服务需要自行申请密钥并填写到下方设置项。"
        } else {
            "此服务无需 API Key，启用后即可使用。"
        }))
        .when(openai, |el| {
            el.child(text_field_row(
                format!("be_base_{id}"),
                "接口地址",
                entry.params.get("base").cloned().unwrap_or_default(),
                "https://api.openai.com/v1",
                &p,
                id.clone(),
                "base",
                window,
                cx,
            ))
            .child(text_field_row(
                format!("be_model_{id}"),
                "模型",
                entry
                    .params
                    .get("model")
                    .cloned()
                    .unwrap_or_else(|| "gpt-4o-mini".into()),
                "gpt-4o-mini",
                &p,
                id.clone(),
                "model",
                window,
                cx,
            ))
        })
        .children(extra.into_iter().map(|(key, label, placeholder)| {
            text_field_row(
                format!("be_{key}_{id}"),
                label,
                entry.params.get(key).cloned().unwrap_or_default(),
                placeholder,
                &p,
                id.clone(),
                key,
                window,
                cx,
            )
        }))
        .when(has_key, |el| {
            el.child(secret_key_row(
                id.clone(),
                key_hint,
                has_key_ref,
                &p,
                actions.on_secret_input.clone(),
                openai,
                window,
                cx,
            ))
        })
        .child(div().flex_1())
        .child({
            let status = if cx.has_global::<SettingsDraft>() {
                draft(cx).status.clone()
            } else {
                None
            };
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .child(
                    if let Some(ref st) = status {
                        status_badge(st, window.appearance(), &p)
                    } else {
                        div().into_any_element()
                    },
                )
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(
                            Button::new("be_verify")
                                .ghost()
                                .label("验证")
                                .with_size(options.size())
                                .on_click({
                                    let view = view.clone();
                                    let id = id.clone();
                                    move |_, _, cx| {
                                        persist_secret_for(&id, cx);
                                        set_status(StatusKind::Info, "正在验证…", cx);
                                        view.update(cx, |_, cx| cx.notify());
                                        let cfg = draft(cx).cfg.clone();
                                        let id = id.clone();
                                        let view2 = view.clone();
                                        cx.spawn(async move |cx| {
                                            let msg = cx
                                                .background_executor()
                                                .spawn(async move { probe_backend(&cfg, &id) })
                                                .await;
                                            view2.update(cx, |_, cx| {
                                                set_status(
                                                    if msg.contains("成功") {
                                                        StatusKind::Success
                                                    } else {
                                                        StatusKind::Error
                                                    },
                                                    msg,
                                                    cx,
                                                );
                                                cx.notify();
                                            });
                                        })
                                        .detach();
                                    }
                                }),
                        )
                        .child(
                            Button::new("be_revert")
                                .ghost()
                                .label("撤销")
                                .with_size(options.size())
                                .on_click({
                                    let view = view.clone();
                                    let id = id.clone();
                                    move |_, _, cx| {
                                        let saved = cx.global::<AppState>().config().clone();
                                        if let Some(orig) =
                                            saved.backends.iter().find(|b| b.id == id).cloned()
                                            && let Some(b) =
                                                draft_mut(cx).cfg.backends.iter_mut().find(|b| b.id == id)
                                        {
                                            *b = orig;
                                        }
                                        draft_mut(cx).secret_inputs.remove(&id);
                                        set_status(StatusKind::Info, "已撤销该服务的未保存更改", cx);
                                        view.update(cx, |_, cx| cx.notify());
                                    }
                                }),
                        )
                        .child(
                            Button::new("be_save")
                                .primary()
                                .label("保存")
                                .with_size(options.size())
                                .on_click({
                                    let view = view.clone();
                                    let on_save = actions.on_save.clone();
                                    let id = id.clone();
                                    move |_, _, cx| {
                                        save_one_backend(&id, &on_save, cx);
                                        view.update(cx, |_, cx| cx.notify());
                                    }
                                }),
                        ),
                )
        })
        .into_any_element()
}

fn extra_backend_fields(kind: BackendKind) -> Vec<(&'static str, &'static str, &'static str)> {
    match kind {
        BackendKind::Baidu => vec![("app_id", "APP ID", "开发者信息中的 APP ID")],
        BackendKind::Youdao | BackendKind::Niutrans => {
            vec![("app_id", "App ID", "App ID")]
        }
        BackendKind::Volcano => vec![
            ("access_key", "Access Key ID", "AKLT…"),
            ("region", "地域", "cn-north-1"),
        ],
        BackendKind::Amazon => vec![
            ("access_key", "AccessKey", "Access Key"),
            ("region", "地域", "地域"),
        ],
        BackendKind::Aliyun => vec![("access_key_id", "AccessKey ID", "LTAI…")],
        BackendKind::Microsoft => vec![
            ("region", "地域", "eastasia"),
            ("endpoint", "终结点", "可留空使用全球地址"),
            ("deployment", "部署名", "可留空使用神经机器翻译"),
        ],
        BackendKind::DeepL => Vec::new(),
        BackendKind::Google => vec![("project", "项目", "GCP 项目 ID（可选）")],
        BackendKind::Claude | BackendKind::Gemini => vec![("model", "模型", "模型名")],
        _ => Vec::new(),
    }
}

struct FieldInputState {
    input: Entity<InputState>,
    _subscription: Subscription,
}

#[allow(clippy::too_many_arguments)]
fn text_field_row(
    field_id: String,
    label: &'static str,
    value: String,
    placeholder: &'static str,
    p: &crate::shared::theme::Palette,
    entry_id: String,
    param_key: &'static str,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let value_s: SharedString = value.clone().into();
    let placeholder_s: SharedString = placeholder.into();
    let state = window.use_keyed_state(SharedString::from(field_id), cx, {
        let value_s = value_s.clone();
        let placeholder_s = placeholder_s.clone();
        let entry_id = entry_id.clone();
        move |window, cx| {
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder_s.clone())
                    .default_value(value_s.clone())
            });
            let _subscription = cx.subscribe(&input, {
                let entry_id = entry_id.clone();
                move |_, input, event: &InputEvent, cx| {
                    if !matches!(event, InputEvent::Change) {
                        return;
                    }
                    let v = input.read(cx).value().to_string();
                    if let Some(b) = draft_mut(cx)
                        .cfg
                        .backends
                        .iter_mut()
                        .find(|b| b.id == entry_id)
                    {
                        match param_key {
                            "display_name" => b.display_name = v,
                            key => {
                                b.params.insert(key.into(), v);
                            }
                        }
                    }
                }
            });
            FieldInputState {
                input,
                _subscription,
            }
        }
    });
    state.update(cx, |state, cx| {
        if state.input.read(cx).value().as_ref() != value.as_str() {
            state.input.update(cx, |input, cx| {
                input.set_value(value.clone(), window, cx);
            });
        }
    });
    let input = state.read(cx).input.clone();
    v_flex()
        .w_full()
        .gap_1()
        .child(div().text_xs().text_color(p.muted).child(label))
        .child(Input::new(&input).small().w_full())
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn secret_key_row(
    id: String,
    key_hint: String,
    has_key_ref: bool,
    p: &crate::shared::theme::Palette,
    on_secret: Arc<dyn Fn(String) + Send + Sync>,
    openai: bool,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let placeholder: SharedString = if has_key_ref && key_hint.is_empty() {
        "已配置，输入新密钥可覆盖".into()
    } else {
        "API Key".into()
    };
    let value: SharedString = key_hint.clone().into();
    let field_id = SharedString::from(format!("be_key_{id}"));
    let state = window.use_keyed_state(field_id, cx, {
        let value = value.clone();
        let placeholder = placeholder.clone();
        let id = id.clone();
        let on_secret = on_secret.clone();
        move |window, cx| {
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(true)
                    .placeholder(placeholder.clone())
                    .default_value(value.clone())
            });
            let _subscription = cx.subscribe(&input, {
                let id = id.clone();
                let on_secret = on_secret.clone();
                move |_, input, event: &InputEvent, cx| match event {
                    InputEvent::Change => {
                        let v = input.read(cx).value().to_string();
                        draft_mut(cx).secret_inputs.insert(id.clone(), v);
                    }
                    InputEvent::Blur => {
                        commit_secret(&id, openai, &on_secret, cx);
                    }
                    _ => {}
                }
            });
            FieldInputState {
                input,
                _subscription,
            }
        }
    });
    state.update(cx, |state, cx| {
        if state.input.read(cx).value().as_ref() != key_hint.as_str() {
            state.input.update(cx, |input, cx| {
                input.set_value(key_hint.clone(), window, cx);
            });
        }
    });
    let input = state.read(cx).input.clone();
    v_flex()
        .w_full()
        .gap_1()
        .child(div().text_xs().text_color(p.muted).child("API Key"))
        .child(
            Input::new(&input)
                .small()
                .w_full()
                .content_type(InputContentType::Password)
                .mask_toggle(),
        )
        .into_any_element()
}

fn commit_secret(
    id: &str,
    openai: bool,
    on_secret: &Arc<dyn Fn(String) + Send + Sync>,
    cx: &mut App,
) {
    let Some(key) = draft(cx).secret_inputs.get(id).cloned() else {
        return;
    };
    let key = key.trim().to_string();
    if key.is_empty() {
        return;
    }
    let Some(account) = draft(cx)
        .cfg
        .backends
        .iter()
        .find(|b| b.id == id)
        .map(|b| b.secret_account())
    else {
        return;
    };
    let store = crate::app::secrets::KeyringStore;
    if let Err(e) = crate::app::secrets::SecretStore::set(&store, &account, &key) {
        set_status(StatusKind::Error, format!("钥匙串写入失败: {e}"), cx);
        return;
    }
    let label = if let Some(b) = draft_mut(cx).cfg.backends.iter_mut().find(|b| b.id == id) {
        b.params
            .insert("key_ref".into(), crate::app::secrets::make_ref(&account));
        if openai && !b.display_name.is_empty() {
            b.display_name.clone()
        } else {
            kind_label(&b.kind).to_string()
        }
    } else {
        return;
    };
    set_status(
        StatusKind::Info,
        format!("{label} 密钥已写入，请点保存"),
        cx,
    );
    on_secret(id.to_string());
}

fn persist_secret_for(id: &str, cx: &mut App) {
    let noop: Arc<dyn Fn(String) + Send + Sync> = Arc::new(|_| {});
    commit_secret(id, true, &noop, cx);
}

fn live_config(cx: &App) -> Config {
    cx.global::<AppState>().config().clone()
}

fn write_live(
    next: Config,
    apply_ocr: bool,
    msg: impl Into<String>,
    on_save: &Arc<dyn Fn(Config) + Send + Sync>,
    cx: &mut App,
) {
    if let Err(e) = next.save(&paths::config_file()) {
        set_status(StatusKind::Error, format!("保存失败: {e}"), cx);
        return;
    }
    if let Err(e) = crate::platform::autostart::set_auto_start(next.auto_start) {
        log::warn!("设置开机自启动失败: {e:#}");
    }
    if apply_ocr {
        let backend = next.ocr_backend.clone();
        std::thread::Builder::new()
            .name("ocr-switch".into())
            .spawn(move || ocr::set_engine_from_config(&backend))
            .ok();
    }
    cx.global::<AppState>().reload_config(next.clone());
    on_save(next);
    set_status(StatusKind::Success, msg, cx);
}

fn save_translate_form(on_save: &Arc<dyn Fn(Config) + Send + Sync>, cx: &mut App) {
    let d = draft(cx).cfg.clone();
    let mut c = live_config(cx);
    c.target_lang = d.target_lang;
    c.source_lang = d.source_lang;
    c.auto_swap = d.auto_swap;
    c.pin_popup_by_default = d.pin_popup_by_default;
    c.popup_follow_mouse = d.popup_follow_mouse;
    c.auto_start = d.auto_start;
    c.history_limit = d.history_limit;
    write_live(c, false, "翻译设置已保存", on_save, cx);
}

fn save_ocr_form(on_save: &Arc<dyn Fn(Config) + Send + Sync>, cx: &mut App) {
    let d = draft(cx).cfg.clone();
    let mut c = live_config(cx);
    c.ocr_continuous = d.ocr_continuous;
    c.ocr_auto_copy = d.ocr_auto_copy;
    c.ocr_smart_paragraph = d.ocr_smart_paragraph;
    write_live(c, false, "OCR 设置已保存", on_save, cx);
}

fn save_hotkeys_form(on_save: &Arc<dyn Fn(Config) + Send + Sync>, cx: &mut App) {
    // 先把 hotkey_edits 中的原始字符串解析到 draft.cfg
    let edits = draft(cx).hotkey_edits.clone();
    type HotkeySetter = fn(&mut Config, HotkeySpec);
    let setters: Vec<(&str, HotkeySetter)> = vec![
        ("划词翻译", |c, v| c.translate_hotkey = v),
        ("截图翻译", |c, v| c.ocr_hotkey = v),
        ("截图 OCR", |c, v| c.ocr_only_hotkey = v),
        ("静默截图 OCR", |c, v| c.ocr_silent_hotkey = v),
        (if cfg!(target_os = "macos") {
            "访达选图 OCR"
        } else if cfg!(target_os = "windows") {
            "资源管理器选图 OCR"
        } else {
            "选中文件 OCR"
        }, |c, v| c.ocr_finder_hotkey = v),
        ("静默划词替换", |c, v| c.silent_replace_hotkey = v),
        ("静默输入框替换", |c, v| c.silent_input_hotkey = v),
        ("原图翻译", |c, v| c.overlay_hotkey = v),
        ("收藏夹", |c, v| c.favorites_hotkey = v),
        ("输入翻译窗口", |c, v| c.input_hotkey = v),
        ("朗读选中文本", |c, v| c.speak_hotkey = v),
        ("切换弹窗固定", |c, v| c.pin_hotkey = v),
        ("显示历史", |c, v| c.history_hotkey = v),
        ("显示设置", |c, v| c.settings_hotkey = v),
    ];
    let mut errors = Vec::new();
    for (label, setter) in &setters {
        if let Some(raw) = edits.get(*label) {
            if let Some(spec) = HotkeySpec::from_display_string(raw) {
                setter(&mut draft_mut(cx).cfg, spec);
            } else if !raw.trim().is_empty() {
                errors.push(format!("{label}: \"{raw}\""));
            }
        }
    }
    if !errors.is_empty() {
        set_status(
            StatusKind::Error,
            format!("格式错误，未保存：{}", errors.join("、")),
            cx,
        );
        return;
    }
    // 保存成功后清空编辑缓存
    draft_mut(cx).hotkey_edits.clear();

    let d = draft(cx).cfg.clone();
    let mut c = live_config(cx);
    c.translate_hotkey = d.translate_hotkey;
    c.pin_hotkey = d.pin_hotkey;
    c.ocr_hotkey = d.ocr_hotkey;
    c.input_hotkey = d.input_hotkey;
    c.speak_hotkey = d.speak_hotkey;
    c.history_hotkey = d.history_hotkey;
    c.settings_hotkey = d.settings_hotkey;
    c.ocr_only_hotkey = d.ocr_only_hotkey;
    c.ocr_silent_hotkey = d.ocr_silent_hotkey;
    c.ocr_finder_hotkey = d.ocr_finder_hotkey;
    c.silent_replace_hotkey = d.silent_replace_hotkey;
    c.silent_input_hotkey = d.silent_input_hotkey;
    c.overlay_hotkey = d.overlay_hotkey;
    c.favorites_hotkey = d.favorites_hotkey;
    write_live(c, false, "快捷键已保存", on_save, cx);
}

fn save_backend_enabled(
    id: &str,
    enabled: bool,
    on_save: &Arc<dyn Fn(Config) + Send + Sync>,
    cx: &mut App,
) {
    let mut c = live_config(cx);
    let Some(slot) = c.backends.iter_mut().find(|b| b.id == id) else {
        return;
    };
    slot.enabled = enabled;
    let label = if slot.display_name.is_empty() {
        kind_label(&slot.kind).to_string()
    } else {
        slot.display_name.clone()
    };
    write_live(
        c,
        false,
        if enabled {
            format!("{label} 已启用")
        } else {
            format!("{label} 已关闭")
        },
        on_save,
        cx,
    );
}

fn save_one_backend(id: &str, on_save: &Arc<dyn Fn(Config) + Send + Sync>, cx: &mut App) {
    persist_secret_for(id, cx);
    let Some(entry) = draft(cx).cfg.backends.iter().find(|b| b.id == id).cloned() else {
        set_status(StatusKind::Error, "未找到该服务", cx);
        return;
    };
    let label = if entry.display_name.is_empty() {
        kind_label(&entry.kind).to_string()
    } else {
        entry.display_name.clone()
    };
    let mut c = live_config(cx);
    if let Some(slot) = c.backends.iter_mut().find(|b| b.id == id) {
        *slot = entry;
    } else {
        c.backends.push(entry);
    }
    write_live(c, false, format!("{label} 已保存"), on_save, cx);
}

fn save_ocr_engine(on_save: &Arc<dyn Fn(Config) + Send + Sync>, cx: &mut App) {
    let mut c = live_config(cx);
    c.ocr_backend = draft(cx).cfg.ocr_backend.clone();
    write_live(c, true, "OCR 服务已保存", on_save, cx);
}

fn save_tts_engine(on_save: &Arc<dyn Fn(Config) + Send + Sync>, cx: &mut App) {
    let mut c = live_config(cx);
    c.tts_backend = draft(cx).cfg.tts_backend.clone();
    write_live(c, false, "语音服务已保存", on_save, cx);
}

fn persist_deleted_backend(id: &str, on_save: &Arc<dyn Fn(Config) + Send + Sync>, cx: &mut App) {
    let mut c = live_config(cx);
    c.backends.retain(|b| b.id != id);
    write_live(c, false, "已删除该接口", on_save, cx);
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(window.appearance());
        let view = cx.entity();
        let nav = self.nav;
        let actions = self.actions.clone();
        let status: Option<StatusMsg> = if cx.has_global::<SettingsDraft>() {
            draft(cx).status.clone()
        } else {
            None
        };

        let options = RenderOptions::new().with_size(Size::Small);
        let content = match nav {
            NavPage::Services => services_workspace(
                view.clone(),
                actions.clone(),
                self.service_tab,
                self.selected_id.clone(),
                &options,
                window,
                cx,
            ),
            _ => {
                let page = self.form_page(view.clone(), cx).unwrap();
                let hide_sidebar = StyleRefinement {
                    display: Some(Display::None),
                    ..Default::default()
                };
                let form = Settings::new(format!("transel-settings-{nav:?}"))
                    .with_size(Size::Small)
                    .with_group_variant(GroupBoxVariant::Fill)
                    .sidebar_width(px(0.))
                    .sidebar_size_range(px(0.)..px(0.))
                    .sidebar_style(&hide_sidebar)
                    .pages(vec![page]);
                v_flex()
                    .size_full()
                    .gap_2()
                    .child(div().flex_1().min_h(px(0.)).min_w(px(0.)).child(form))
                    .when(
                        matches!(nav, NavPage::Translate | NavPage::Ocr | NavPage::Hotkeys),
                        |el| {
                            el.child(page_save_button(
                                match nav {
                                    NavPage::Translate => "save_translate_page",
                                    NavPage::Ocr => "save_ocr_page",
                                    NavPage::Hotkeys => "save_hotkeys_page",
                                    _ => "save_page",
                                },
                                view.clone(),
                                actions.clone(),
                                &options,
                                move |on_save, cx| match nav {
                                    NavPage::Translate => save_translate_form(on_save, cx),
                                    NavPage::Ocr => save_ocr_form(on_save, cx),
                                    NavPage::Hotkeys => save_hotkeys_form(on_save, cx),
                                    _ => {}
                                },
                                &p,
                                window.appearance(),
                                cx,
                            ))
                        },
                    )
                    .into_any_element()
            }
        };

        let picker_open = self.picker_open;
        div()
            .id("settings_root")
            .key_context(UTILITY_KEY_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::close))
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(opaque_panel(window.appearance()))
            .font_family(".SystemUIFont")
            .child(
                v_flex()
                    .id("settings_card")
                    .size_full()
                    .overflow_hidden()
                    .child(
                        h_flex()
                            .id("settings_body")
                            .flex_1()
                            .min_h(px(0.))
                            .w_full()
                            .child(nav_sidebar(view.clone(), nav, &p))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .h_full()
                                    .px_4()
                                    .py_3()
                                    .child(content),
                            ),
                    ),
            )
            .when_some(status, |el, st| {
                el.child(floating_status_toast(
                    &st,
                    window.appearance(),
                    &p,
                    view.clone(),
                ))
            })
            .when(picker_open, |el| {
                el.child(add_service_dialog(view, window, cx))
            })
    }
}

fn kind_label(k: &BackendKind) -> &'static str {
    k.default_display_name()
}

/// 只探测当前这条服务（未启用也会临时打开来测）。
fn probe_backend(cfg: &Config, id: &str) -> String {
    use crate::services::translate::{RequestCtx, TranslateService};
    let Some(entry) = cfg.backends.iter().find(|b| b.id == id) else {
        return "未找到该服务".into();
    };
    let label = if entry.display_name.is_empty() {
        kind_label(&entry.kind).to_string()
    } else {
        entry.display_name.clone()
    };
    let mut cfg = cfg.clone();
    for b in &mut cfg.backends {
        b.enabled = b.id == id;
    }
    let svc = TranslateService::start(&cfg);
    let rx = svc.request(RequestCtx {
        source: "hello".into(),
        prepared: "hello".into(),
        source_lang_hint: Some("en".into()),
        target_lang: cfg.target_lang.clone(),
        only: None,
    });
    match futures::executor::block_on(crate::services::translate::collect_batch(rx)) {
        Ok(batch) => {
            if let Some(r) = batch.results.iter().find(|r| !r.pending) {
                if r.ok {
                    format!("{label} 验证成功")
                } else {
                    format!(
                        "{label} 验证失败: {}",
                        r.error.clone().unwrap_or_else(|| "失败".into())
                    )
                }
            } else {
                format!("{label} 没有返回结果")
            }
        }
        Err(e) => format!("{label} 验证失败: {e}"),
    }
}
