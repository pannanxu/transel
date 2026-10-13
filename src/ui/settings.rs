//! 设置面板窗口：左侧导航 + 右侧表单。
//!
//! 设计：
//! - 视图本身只持有**只读**的配置副本 + 表单输入缓冲。
//! - 所有变更通过 [`SettingsActions`] 回调上抛到主循环；主循环改全局
//!   [`crate::app::config::Config`]，再 `cx.notify()` 触发重渲染。
//! - 保存按钮：把 draft 写到磁盘 + 通知主循环重载。

use std::collections::BTreeMap;
use std::sync::Arc;

use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::*;

use crate::app::config::{BackendKind, Config, HotkeySpec};
use crate::app::paths;
use crate::app::state::AppState;
use crate::services::ocr::{self, OcrBackendKind};
use crate::ui::theme::palette;

use super::icons;

#[derive(Clone)]
pub struct SettingsActions {
    pub on_close: Arc<dyn Fn() + Send + Sync>,
    pub on_save: Arc<dyn Fn(Config) + Send + Sync>,
    pub on_toggle_backend: Arc<dyn Fn(BackendKind) + Send + Sync>,
    pub on_set_target: Arc<dyn Fn(String) + Send + Sync>,
    pub on_toggle_auto_swap: Arc<dyn Fn() + Send + Sync>,
    pub on_toggle_pin_default: Arc<dyn Fn() + Send + Sync>,
    pub on_set_history_limit: Arc<dyn Fn(usize) + Send + Sync>,
    pub on_set_hotkey: Arc<dyn Fn(String, HotkeySpec) + Send + Sync>,
    pub on_set_secret: Arc<dyn Fn(BackendKind, String) + Send + Sync>,
    pub on_clear_history: Arc<dyn Fn() + Send + Sync>,
    pub on_secret_input: Arc<dyn Fn(BackendKind) + Send + Sync>,
}

impl Default for SettingsActions {
    fn default() -> Self {
        Self {
            on_close: Arc::new(|| {}),
            on_save: Arc::new(|_| {}),
            on_toggle_backend: Arc::new(|_| {}),
            on_set_target: Arc::new(|_| {}),
            on_toggle_auto_swap: Arc::new(|| {}),
            on_toggle_pin_default: Arc::new(|| {}),
            on_set_history_limit: Arc::new(|_| {}),
            on_set_hotkey: Arc::new(|_, _| {}),
            on_set_secret: Arc::new(|_, _| {}),
            on_clear_history: Arc::new(|| {}),
            on_secret_input: Arc::new(|_| {}),
        }
    }
}

pub struct SettingsView {
    pub cfg: Config,
    pub selected: SettingsTab,
    pub secret_inputs: BTreeMap<BackendKind, String>,
    pub status: Option<String>,
    pub actions: SettingsActions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Hotkeys,
    Backends,
    About,
}

impl SettingsView {
    pub fn new(cfg: Config, actions: SettingsActions) -> Self {
        Self {
            cfg,
            selected: SettingsTab::General,
            secret_inputs: BTreeMap::new(),
            status: None,
            actions,
        }
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(window.appearance());
        let focus = cx.focus_handle();

        let nav_item = |label: &'static str,
                        icon_svg: &'static str,
                        tab: SettingsTab,
                        selected: bool,
                        p: &crate::ui::theme::Palette,
                        cx: &mut Context<Self>| {
            div()
                .id(label)
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.))
                .px(px(10.))
                .py(px(8.))
                .rounded(px(6.))
                .bg(if selected { p.tab_active } else { p.tab_inactive })
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.selected = tab;
                    cx.notify();
                }))
                .child(svg().path(icon_svg).size(px(14.)).text_color(p.muted))
                .child(div().text_size(px(13.)).text_color(p.text).child(label))
        };

        let left = div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .w(px(160.))
            .p(px(12.))
            .border_r_1()
            .border_color(p.border)
            .child(nav_item(
                "常规",
                icons::SETTINGS,
                SettingsTab::General,
                self.selected == SettingsTab::General,
                &p,
                cx,
            ))
            .child(nav_item(
                "快捷键",
                icons::INPUT,
                SettingsTab::Hotkeys,
                self.selected == SettingsTab::Hotkeys,
                &p,
                cx,
            ))
            .child(nav_item(
                "翻译接口",
                icons::BOOK,
                SettingsTab::Backends,
                self.selected == SettingsTab::Backends,
                &p,
                cx,
            ))
            .child(div().flex_1())
            .child(nav_item(
                "关于",
                icons::HISTORY,
                SettingsTab::About,
                self.selected == SettingsTab::About,
                &p,
                cx,
            ));

        let right: AnyElement = match self.selected {
            SettingsTab::General => self.render_general(&p, cx).into_any_element(),
            SettingsTab::Hotkeys => self.render_hotkeys(&p).into_any_element(),
            SettingsTab::Backends => self.render_backends(&p, cx).into_any_element(),
            SettingsTab::About => self.render_about(&p).into_any_element(),
        };

        let save = div()
            .id("save")
            .px(px(14.))
            .py(px(6.))
            .rounded(px(6.))
            .bg(p.accent)
            .hover(|s| s.bg(p.accent_hover))
            .text_size(px(12.5))
            .text_color(hsla(0., 0., 1., 1.))
            .cursor_pointer()
            .child("保存")
            .on_click(cx.listener(|this, _, _, cx| {
                let draft = this.cfg.clone();
                if let Err(e) = draft.save(&paths::config_file()) {
                    this.status = Some(format!("保存失败: {e}"));
                    cx.notify();
                    return;
                }
                let backend = draft.ocr_backend.clone();
                std::thread::Builder::new()
                    .name("ocr-switch".into())
                    .spawn(move || ocr::set_engine_from_config(&backend))
                    .ok();
                cx.global::<AppState>().reload_config(draft.clone());
                (this.actions.on_save)(draft);
                this.status = Some("已保存并热更新".into());
                cx.notify();
            }));

        let close_btn = div()
            .id("close_btn")
            .size(px(22.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .cursor_pointer()
            .hover(|s| s.bg(p.hover))
            .child(svg().path(icons::CLOSE).size(px(14.)).text_color(p.muted))
            .on_click(cx.listener(|this, _, window, _cx| {
                (this.actions.on_close)();
                window.remove_window();
            }));

        let topbar = div()
            .flex()
            .flex_row()
            .items_center()
            .h(px(40.))
            .px(px(14.))
            .gap(px(8.))
            .border_b_1()
            .border_color(p.border)
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.title)
                    .child("划词翻译 · 设置"),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .child(self.status.clone().unwrap_or_default()),
            )
            .child(save)
            .child(close_btn);

        let body = div()
            .flex()
            .flex_row()
            .flex_1()
            .min_h(px(0.))
            .child(left)
            .child(
                div()
                    .id("settings_body")
                    .flex_1()
                    .min_h(px(0.))
                    .p(px(14.))
                    .overflow_y_scrollbar()
                    .child(right),
            );

        let mut card = div()
            .id("settings_card")
            .size_full()
            .flex()
            .flex_col()
            .bg(p.card)
            .border_1()
            .border_color(p.border)
            .rounded(px(crate::platform::CORNER_RADIUS as f32))
            .font_family(".SystemUIFont");
        #[cfg(not(target_os = "macos"))]
        {
            card = card.shadow(vec![BoxShadow {
                color: p.shadow,
                offset: point(px(0.), px(4.)),
                blur_radius: px(10.),
                spread_radius: px(0.),
                inset: false,
            }]);
        }
        let card = card.child(topbar).child(body);

        div()
            .id("settings_root")
            .track_focus(&focus)
            .on_key_down(cx.listener(|_, ev: &KeyDownEvent, window, _cx| {
                if ev.keystroke.key == "escape" {
                    window.remove_window();
                }
            }))
            .size_full()
            .p(px(super::popup::SHADOW_MARGIN))
            .child(card)
    }
}

impl SettingsView {
    fn render_ort_download(
        &mut self,
        p: &crate::ui::theme::Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = ocr::ort_status_label();
        let st = ocr::ort_status();
        let can_download = matches!(st, ocr::OrtStatus::Missing | ocr::OrtStatus::Downloaded);
        let mut btn = div()
            .id("ocr_download_ort")
            .px(px(10.))
            .py(px(4.))
            .rounded(px(5.))
            .bg(if can_download { p.accent } else { p.tab_inactive })
            .text_size(px(11.5))
            .text_color(if can_download {
                hsla(0., 0., 1., 1.)
            } else {
                p.muted
            });
        if can_download {
            btn = btn.cursor_pointer().on_click(cx.listener(|this, _, _, cx| {
                this.status = Some("正在下载 ONNX Runtime…".into());
                cx.notify();
                cx.spawn(async move |this, cx| {
                    let result = cx
                        .background_executor()
                        .spawn(async {
                            ocr::download_ort_runtime().map(|p| p.display().to_string())
                        })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        match result {
                            Ok(path) => {
                                let backend = this.cfg.ocr_backend.clone();
                                std::thread::Builder::new()
                                    .name("ocr-warmup".into())
                                    .spawn(move || ocr::set_engine_from_config(&backend))
                                    .ok();
                                this.status = Some(format!("ONNX Runtime 已就绪\n{path}"));
                            }
                            Err(e) => {
                                this.status = Some(format!("下载失败: {e:#}"));
                            }
                        }
                        cx.notify();
                    });
                })
                .detach();
            }));
        }
        let btn = btn.child(match st {
            ocr::OrtStatus::Ready => "已就绪",
            ocr::OrtStatus::Downloaded => "加载引擎",
            ocr::OrtStatus::Missing => "下载本地 OCR 引擎",
            ocr::OrtStatus::Unsupported => "当前平台不可用",
        });

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.))
                    .child(btn)
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(p.muted)
                            .child(format!("状态：{label}")),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .child(format!(
                        "下载 Microsoft ONNX Runtime {}（约数十 MB，仅本地 OCR 需要）。",
                        ocr::ORT_VERSION
                    )),
            )
            .into_any_element()
    }

    fn render_general(&mut self, p: &crate::ui::theme::Palette, cx: &mut Context<Self>) -> AnyElement {
        let current_ocr = OcrBackendKind::parse(&self.cfg.ocr_backend);
        let mut ocr_chips: Vec<AnyElement> = Vec::new();
        for kind in OcrBackendKind::available() {
            let is_sel = current_ocr == *kind
                || (*kind == OcrBackendKind::Auto && current_ocr == OcrBackendKind::RapidOcr);
            // 云端占位：可选中展示，但不写入配置（避免下次 OCR 直接失败）
            let cloud = kind.is_cloud_placeholder();
            let id = kind.as_str().to_string();
            let label = kind.label().to_string();
            ocr_chips.push(
                div()
                    .id(format!("ocr_{id}"))
                    .px(px(8.))
                    .py(px(3.))
                    .rounded(px(5.))
                    .bg(if is_sel && !cloud {
                        p.tab_active
                    } else {
                        p.tab_inactive
                    })
                    .text_size(px(11.5))
                    .text_color(if is_sel && !cloud { p.text } else { p.muted })
                    .opacity(if cloud { 0.65 } else { 1. })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if cloud {
                            this.status = Some(format!("{label}：口子已预留，尚未接入"));
                        } else {
                            this.cfg.ocr_backend = id.clone();
                            this.status = Some(format!("OCR：{label}"));
                        }
                        cx.notify();
                    }))
                    .child(kind.label())
                    .into_any_element(),
            );
        }

        let hist_str = self.cfg.history_limit.to_string();
        let mut items: Vec<AnyElement> = Vec::new();
        for (code, _short, label) in crate::ui::theme::LANGUAGES.iter().filter(|(c, _, _)| *c != "auto") {
            let is_sel = self.cfg.target_lang == *code;
            let code_owned = code.to_string();
            let label_owned = (*label).to_string();
            items.push(
                div()
                    .id((*code).to_string())
                    .px(px(8.))
                    .py(px(3.))
                    .rounded(px(5.))
                    .bg(if is_sel { p.tab_active } else { p.tab_inactive })
                    .text_size(px(11.5))
                    .text_color(if is_sel { p.text } else { p.muted })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.cfg.target_lang = code_owned.clone();
                        this.status = Some(format!("目标语言：{label_owned}"));
                        cx.notify();
                    }))
                    .child((*label).to_string())
                    .into_any_element(),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(p.title).child("目标语言"))
            .child(div().flex().flex_row().flex_wrap().gap(px(4.)).children(items))
            .child(div().text_size(px(11.)).text_color(p.muted).child("检测到源 = 目标时自动反向翻译（可在\"自动交换\"中开关）"))
            .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(p.title).child("OCR 引擎"))
            .child(div().flex().flex_row().flex_wrap().gap(px(4.)).children(ocr_chips))
            .child(self.render_ort_download(p, cx))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .child("本地 RapidOCR 需先下载 ONNX Runtime；模型会在首次识别时自动缓存。云端 OCR 口子已预留。保存后生效。"),
            )
            .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(p.title).child("OCR 选项"))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.))
                    .child(div().text_size(px(12.5)).text_color(p.muted).child("连续识别:"))
                    .child(
                        div()
                            .id("ocr_cont")
                            .px(px(8.))
                            .py(px(3.))
                            .rounded(px(5.))
                            .bg(if self.cfg.ocr_continuous { p.tab_active } else { p.tab_inactive })
                            .text_size(px(11.5))
                            .text_color(p.text)
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cfg.ocr_continuous = !this.cfg.ocr_continuous;
                                cx.notify();
                            }))
                            .child(if self.cfg.ocr_continuous { "开" } else { "关" }),
                    )
                    .child(div().text_size(px(12.5)).text_color(p.muted).child("自动复制:"))
                    .child(
                        div()
                            .id("ocr_copy")
                            .px(px(8.))
                            .py(px(3.))
                            .rounded(px(5.))
                            .bg(if self.cfg.ocr_auto_copy { p.tab_active } else { p.tab_inactive })
                            .text_size(px(11.5))
                            .text_color(p.text)
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cfg.ocr_auto_copy = !this.cfg.ocr_auto_copy;
                                cx.notify();
                            }))
                            .child(if self.cfg.ocr_auto_copy { "开" } else { "关" }),
                    )
                    .child(div().text_size(px(12.5)).text_color(p.muted).child("智能分段:"))
                    .child(
                        div()
                            .id("ocr_para")
                            .px(px(8.))
                            .py(px(3.))
                            .rounded(px(5.))
                            .bg(if self.cfg.ocr_smart_paragraph {
                                p.tab_active
                            } else {
                                p.tab_inactive
                            })
                            .text_size(px(11.5))
                            .text_color(p.text)
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cfg.ocr_smart_paragraph = !this.cfg.ocr_smart_paragraph;
                                cx.notify();
                            }))
                            .child(if self.cfg.ocr_smart_paragraph { "开" } else { "关" }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.))
                    .child(div().text_size(px(12.5)).text_color(p.muted).child("自动交换:"))
                    .child(
                        div()
                            .id("swap")
                            .px(px(8.))
                            .py(px(3.))
                            .rounded(px(5.))
                            .bg(if self.cfg.auto_swap { p.tab_active } else { p.tab_inactive })
                            .text_size(px(11.5))
                            .text_color(p.text)
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cfg.auto_swap = !this.cfg.auto_swap;
                                cx.notify();
                            }))
                            .child(if self.cfg.auto_swap { "开" } else { "关" }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.))
                    .child(div().text_size(px(12.5)).text_color(p.muted).child("默认固定弹窗:"))
                    .child(
                        div()
                            .id("pin_default")
                            .px(px(8.))
                            .py(px(3.))
                            .rounded(px(5.))
                            .bg(if self.cfg.pin_popup_by_default {
                                p.tab_active
                            } else {
                                p.tab_inactive
                            })
                            .text_size(px(11.5))
                            .text_color(p.text)
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cfg.pin_popup_by_default = !this.cfg.pin_popup_by_default;
                                cx.notify();
                            }))
                            .child(if self.cfg.pin_popup_by_default { "开" } else { "关" }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.))
                    .child(div().text_size(px(12.5)).text_color(p.muted).child("历史最大条数:"))
                    .child(
                        div()
                            .id("histlimit")
                            .h(px(24.))
                            .rounded(px(5.))
                            .bg(p.input_bg)
                            .border_1()
                            .border_color(p.border)
                            .px(px(8.))
                            .flex()
                            .items_center()
                            .text_size(px(12.))
                            .text_color(p.text)
                            .child(hist_str),
                    ),
            )
            .into_any_element()
    }

    fn render_hotkeys(&mut self, p: &crate::ui::theme::Palette) -> AnyElement {
        let items: [(String, &str, HotkeySpec); 14] = [
            ("划词翻译".into(), "translate", self.cfg.translate_hotkey.clone()),
            ("截图翻译".into(), "ocr", self.cfg.ocr_hotkey.clone()),
            ("截图 OCR".into(), "ocr_only", self.cfg.ocr_only_hotkey.clone()),
            ("静默截图 OCR".into(), "ocr_silent", self.cfg.ocr_silent_hotkey.clone()),
            ("访达选图 OCR".into(), "ocr_finder", self.cfg.ocr_finder_hotkey.clone()),
            ("静默划词替换".into(), "silent_replace", self.cfg.silent_replace_hotkey.clone()),
            ("静默输入框替换".into(), "silent_input", self.cfg.silent_input_hotkey.clone()),
            ("原图翻译".into(), "overlay", self.cfg.overlay_hotkey.clone()),
            ("收藏夹".into(), "favorites", self.cfg.favorites_hotkey.clone()),
            ("输入翻译窗口".into(), "input", self.cfg.input_hotkey.clone()),
            ("朗读选中文本".into(), "speak", self.cfg.speak_hotkey.clone()),
            ("切换弹窗固定".into(), "pin", self.cfg.pin_hotkey.clone()),
            ("显示历史".into(), "history", self.cfg.history_hotkey.clone()),
            ("显示设置".into(), "settings", self.cfg.settings_hotkey.clone()),
        ];
        let mut rows: Vec<AnyElement> = Vec::new();
        for (label, name, spec) in items.iter() {
            let spec_str = spec.to_string();
            rows.push(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.))
                    .child(div().text_size(px(12.5)).text_color(p.text).child(label.clone()).w(px(140.)))
                    .child(div().id((*name).to_string()).h(px(26.)).px(px(8.)).rounded(px(5.)).bg(p.input_bg).border_1().border_color(p.border).flex().items_center().text_size(px(12.)).text_color(p.muted).child(spec_str))
                    .into_any_element()
            );
        }
        let on_clear_hist = self.actions.on_clear_history.clone();
        let clear_btn = div()
            .id("clear_hist")
            .px(px(10.))
            .py(px(4.))
            .rounded(px(5.))
            .bg(p.danger)
            .text_size(px(11.5))
            .text_color(hsla(0., 0., 1., 1.))
            .cursor_pointer()
            .on_click(move |_, _, _| (on_clear_hist)())
            .child("清空历史");
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(p.title).child("快捷键"))
            .children(rows)
            .child(div().h(px(8.)))
            .child(div().text_size(px(11.)).text_color(p.muted).child("(快捷键重绑定将在后续版本中提供)"))
            .child(div().h(px(8.)))
            .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(p.title).child("数据"))
            .child(clear_btn)
            .into_any_element()
    }

    fn render_backends(&mut self, p: &crate::ui::theme::Palette, cx: &mut Context<Self>) -> AnyElement {
        let mut rows: Vec<AnyElement> = Vec::new();
        for b in self.cfg.backends.iter() {
            let kind = b.kind;
            let has_key = kind.needs_key();
            let enabled = b.enabled;
            let val = self.secret_inputs.get(&kind).cloned().unwrap_or_default();
            let mut row = div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .p(px(8.))
                .rounded(px(6.))
                .bg(p.tab_inactive)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(8.))
                        .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(p.text).child(kind_label(&kind)))
                        .child(div().text_size(px(11.)).text_color(p.muted).child(kind.as_str()))
                        .child(div().flex_1())
                        .child(
                            div()
                                .id(format!("be_toggle_{}", kind.as_str()))
                                .px(px(8.))
                                .py(px(3.))
                                .rounded(px(5.))
                                .bg(if enabled { p.accent } else { p.tab_inactive })
                                .text_size(px(11.))
                                .text_color(hsla(0., 0., 1., 1.))
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(b) = this.cfg.backends.iter_mut().find(|b| b.kind == kind) {
                                        b.enabled = !b.enabled;
                                    }
                                    (this.actions.on_toggle_backend)(kind);
                                    cx.notify();
                                }))
                                .child(if enabled { "已启用" } else { "已禁用" }),
                        ),
                );
            if has_key {
                row = row.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(6.))
                        .child(div().text_size(px(11.5)).text_color(p.muted).w(px(80.)).child("API Key:"))
                        .child(
                            div()
                                .id(format!("be_key_{}", kind.as_str()))
                                .h(px(24.))
                                .rounded(px(5.))
                                .bg(p.input_bg)
                                .border_1()
                                .border_color(p.border)
                                .px(px(8.))
                                .flex_1()
                                .flex()
                                .items_center()
                                .text_size(px(12.))
                                .text_color(if val.is_empty() { p.muted } else { p.text })
                                .child(if val.is_empty() {
                                    "点击：从剪贴板写入钥匙串".to_string()
                                } else {
                                    "*".repeat(val.len().min(20))
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let Ok(mut cb) = arboard::Clipboard::new() else {
                                        this.status = Some("无法读取剪贴板".into());
                                        cx.notify();
                                        return;
                                    };
                                    let Ok(raw) = cb.get_text() else {
                                        this.status = Some("剪贴板无文本".into());
                                        cx.notify();
                                        return;
                                    };
                                    let key = raw.trim().to_string();
                                    if key.is_empty() {
                                        this.status = Some("剪贴板为空".into());
                                        cx.notify();
                                        return;
                                    }
                                    let account = crate::app::secrets::account_for(kind.as_str());
                                    let store = crate::app::secrets::KeyringStore;
                                    if let Err(e) = crate::app::secrets::SecretStore::set(&store, &account, &key) {
                                        this.status = Some(format!("钥匙串写入失败: {e}"));
                                        cx.notify();
                                        return;
                                    }
                                    let key_ref = crate::app::secrets::make_ref(&account);
                                    if let Some(b) = this.cfg.backends.iter_mut().find(|b| b.kind == kind) {
                                        b.params.insert("key_ref".into(), key_ref);
                                    }
                                    this.secret_inputs.insert(kind, key);
                                    this.status = Some(format!("{} 密钥已写入，请点保存", kind_label(&kind)));
                                    (this.actions.on_secret_input)(kind);
                                    cx.notify();
                                })),
                        ),
                );
            }
            rows.push(row.into_any_element());
        }
        let verify_btn = div()
            .id("verify_backends")
            .px(px(10.))
            .py(px(4.))
            .rounded(px(5.))
            .bg(p.accent)
            .text_size(px(11.5))
            .text_color(hsla(0., 0., 1., 1.))
            .cursor_pointer()
            .child("验证已启用接口")
            .on_click(cx.listener(|this, _, _, cx| {
                this.status = Some("正在验证…".into());
                cx.notify();
                let cfg = this.cfg.clone();
                cx.spawn(async move |this, cx| {
                    let msg = cx
                        .background_executor()
                        .spawn(async move { probe_backends(&cfg) })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        this.status = Some(msg);
                        cx.notify();
                    });
                })
                .detach();
            }));

        let tts_row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .child(div().text_size(px(12.5)).text_color(p.text).child("TTS 后端:"))
            .children(["auto", "system", "microsoft", "google"].iter().map(|id| {
                let selected = self.cfg.tts_backend == *id;
                let label = match *id {
                    "auto" | "system" => "系统",
                    "microsoft" => "Microsoft",
                    "google" => "Google",
                    _ => id,
                };
                div()
                    .id(format!("tts_{id}"))
                    .px(px(8.))
                    .py(px(3.))
                    .rounded(px(5.))
                    .bg(if selected { p.accent } else { p.tab_inactive })
                    .text_size(px(11.))
                    .text_color(if selected {
                        hsla(0., 0., 1., 1.)
                    } else {
                        p.text
                    })
                    .cursor_pointer()
                    .on_click({
                        let id = (*id).to_string();
                        cx.listener(move |this, _, _, cx| {
                            this.cfg.tts_backend = id.clone();
                            cx.notify();
                        })
                    })
                    .child(label)
            }));

        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(p.title).child("翻译接口"))
            .child(div().text_size(px(11.)).text_color(p.muted).child("勾选\"已启用\"才会调用；启用多个时弹窗里可以 tab 切换对比。"))
            .children(rows)
            .child(verify_btn)
            .child(div().h(px(8.)))
            .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(p.title).child("语音合成"))
            .child(tts_row)
            .child(div().text_size(px(11.)).text_color(p.muted).child("云 TTS 需配置 MICROSOFT_TTS_KEY / GOOGLE_TTS_API_KEY 或写入钥匙串。"))
            .into_any_element()
    }

    fn render_about(&self, p: &crate::ui::theme::Palette) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).text_color(p.title).child("划词翻译"))
            .child(div().text_size(px(12.)).text_color(p.muted).child(format!("版本 {}", env!("CARGO_PKG_VERSION"))))
            .child(div().text_size(px(12.)).text_color(p.muted).child("基于 GPUI + gpui-kit，跨平台 (macOS / Windows / Linux)"))
            .child(div().text_size(px(11.)).text_color(p.muted).child("配置文件: "))
            .child(div().text_size(px(11.)).text_color(p.text).child(crate::app::paths::config_file().display().to_string()))
            .child(div().text_size(px(11.)).text_color(p.muted).child("历史文件: "))
            .child(div().text_size(px(11.)).text_color(p.text).child(crate::app::paths::history_file().display().to_string()))
            .into_any_element()
    }
}

fn kind_label(k: &BackendKind) -> &'static str {
    k.default_display_name()
}

/// 对已启用后端发一次探测翻译。
fn probe_backends(cfg: &Config) -> String {
    use crate::services::translate::{RequestCtx, TranslateService};
    let svc = TranslateService::start(cfg);
    let enabled: Vec<_> = cfg.backends.iter().filter(|b| b.enabled).collect();
    if enabled.is_empty() {
        return "没有已启用的翻译接口".into();
    }
    let rx = svc.request(RequestCtx {
        source: "hello".into(),
        prepared: "hello".into(),
        source_lang_hint: Some("en".into()),
        target_lang: cfg.target_lang.clone(),
        only: None,
    });
    match futures::executor::block_on(crate::services::translate::collect_batch(rx)) {
        Ok(batch) => {
            let ok: Vec<_> = batch
                .results
                .iter()
                .filter(|r| r.ok && !r.pending)
                .map(|r| r.backend.clone())
                .collect();
            let fail: Vec<_> = batch
                .results
                .iter()
                .filter(|r| !r.ok && !r.pending)
                .map(|r| {
                    format!(
                        "{}({})",
                        r.backend,
                        r.error.clone().unwrap_or_else(|| "失败".into())
                    )
                })
                .collect();
            if fail.is_empty() {
                format!("验证成功: {}", ok.join(", "))
            } else if ok.is_empty() {
                format!("全部失败: {}", fail.join("; "))
            } else {
                format!("部分成功: {} | 失败: {}", ok.join(", "), fail.join("; "))
            }
        }
        Err(e) => format!("验证失败: {e}"),
    }
}
