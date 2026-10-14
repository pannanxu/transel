//! 独立 OCR 结果窗口。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::app::state::AppState;
use crate::platform;
use crate::shared::actions::{CloseWindow, CopyResult, UTILITY_KEY_CONTEXT};
use crate::shared::chrome::EmptyState;
use crate::shared::icons;
use crate::shared::loading;
use crate::shared::text_select::selectable_plain;
use crate::shared::theme::palette;
use crate::shared::tokens::SHADOW_MARGIN;
use crate::shared::tooltip;

const PAD: f32 = 14.;
const HEADER_H: f32 = 24.;
const GAP: f32 = 10.;

#[derive(Clone)]
pub struct OcrActions {
    pub on_close: Arc<dyn Fn() + Send + Sync>,
    pub on_copy: Arc<dyn Fn(String) + Send + Sync>,
    pub on_translate: Arc<dyn Fn(String) + Send + Sync>,
    pub on_toggle_continuous: Arc<dyn Fn(bool) + Send + Sync>,
    pub continuous: Arc<AtomicBool>,
}

impl Default for OcrActions {
    fn default() -> Self {
        Self {
            on_close: Arc::new(|| {}),
            on_copy: Arc::new(|_| {}),
            on_translate: Arc::new(|_| {}),
            on_toggle_continuous: Arc::new(|_| {}),
            continuous: Arc::new(AtomicBool::new(false)),
        }
    }
}

pub struct OcrView {
    pub text: String,
    pub status: Option<SharedString>,
    pub copied: bool,
    pub loading: bool,
    pub actions: OcrActions,
    focus: FocusHandle,
    drag: Option<platform::ManualDrag>,
}

impl OcrView {
    pub fn new(
        text: String,
        actions: OcrActions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle().tab_stop(true);
        focus.focus(window, cx);
        Self {
            text,
            status: None,
            copied: false,
            loading: false,
            actions,
            focus,
            drag: None,
        }
    }

    pub fn loading(
        actions: OcrActions,
        window: &mut Window,
        cx: &mut Context<Self>,
        existing: String,
    ) -> Self {
        let focus = cx.focus_handle().tab_stop(true);
        focus.focus(window, cx);
        Self {
            text: existing,
            status: Some("识别中…".into()),
            copied: false,
            loading: true,
            actions,
            focus,
            drag: None,
        }
    }

    pub fn set_loading(&mut self, on: bool) {
        self.loading = on;
        self.status = if on {
            Some("识别中…".into())
        } else {
            None
        };
    }

    pub fn apply_result(&mut self, new_text: &str) {
        let t = new_text.trim();
        self.loading = false;
        if t.is_empty() {
            self.status = Some("没有识别出文字".into());
            return;
        }
        let append = self.actions.continuous.load(Ordering::Relaxed);
        if append && !self.text.trim().is_empty() {
            self.text.push_str("\n\n");
            self.text.push_str(t);
        } else {
            self.text = t.to_string();
        }
        self.status = None;
    }

    pub fn set_error(&mut self, msg: impl Into<SharedString>) {
        self.loading = false;
        self.status = Some(msg.into());
    }

    fn close(&mut self, _: &CloseWindow, window: &mut Window, _cx: &mut Context<Self>) {
        (self.actions.on_close)();
        window.remove_window();
    }

    fn copy_result(&mut self, _: &CopyResult, _window: &mut Window, cx: &mut Context<Self>) {
        if self.loading || self.text.trim().is_empty() {
            return;
        }
        let t = self.text.clone();
        cx.write_to_clipboard(ClipboardItem::new_string(t.clone()));
        (self.actions.on_copy)(t);
        self.copied = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1200))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.copied = false;
                cx.notify();
            });
        })
        .detach();
    }
}

fn icon_button(
    id: &'static str,
    icon: &'static str,
    color: Hsla,
    hover: Hsla,
    tip: impl Into<SharedString>,
) -> Stateful<Div> {
    let tip = tip.into();
    div()
        .id(id)
        .size(px(22.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .active(move |s| s.opacity(0.7))
        .tooltip(tooltip::build(tip))
        .child(svg().path(icon).size(px(14.)).text_color(color))
}

impl Render for OcrView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(window.appearance());
        let continuous = self.actions.continuous.load(Ordering::Relaxed);
        let busy = self.loading;
        let can_use = !busy && !self.text.trim().is_empty();
        let status = self.status.clone();

        let body: AnyElement = if self.loading && self.text.is_empty() {
            loading::loading_row("正在识别…", &p)
        } else if self.text.is_empty() {
            EmptyState::new("识别结果会显示在这里")
                .detail(status.clone().unwrap_or_else(|| "截图选区后开始识别".into()))
                .into_any_element()
        } else {
            div()
                .id("ocr_text")
                .flex_1()
                .min_h_0()
                .w_full()
                .overflow_y_scrollbar()
                .when(self.loading, |el| {
                    el.child(loading::loading_row("正在识别…", &p))
                })
                .child(selectable_plain(
                    "ocr_body",
                    &self.text,
                    p.text,
                    14.,
                    21.,
                ))
                .into_any_element()
        };

        let copy_svg = if self.copied {
            icons::CHECK
        } else {
            icons::COPY
        };
        let copy_color = if self.copied { p.accent } else { p.muted };
        let cont_color = if continuous { p.accent } else { p.muted };
        let action_color = if can_use { p.muted } else { p.muted.opacity(0.35) };

        let header = div()
            .id("ocr_header")
            .flex()
            .flex_row()
            .items_center()
            .h(px(HEADER_H))
            .gap(px(8.))
            .cursor_default()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, _cx| this.drag = platform::start_drag(window)),
            )
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, _cx| {
                if let Some(d) = &this.drag {
                    if ev.pressed_button == Some(MouseButton::Left) {
                        platform::drag_move(d);
                    } else {
                        this.drag = None;
                    }
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _cx| this.drag = None),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.))
                    .child(img(icons::LOGO).size(px(16.)).rounded(px(3.)))
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.title)
                            .child("OCR"),
                    ),
            )
            .when_some(status.filter(|_| busy), |el, s| {
                el.child(div().text_size(px(11.)).text_color(p.muted).child(s))
            })
            .child(div().flex_1())
            .child(
                icon_button(
                    "ocr_continuous",
                    icons::REPEAT,
                    cont_color,
                    p.hover,
                    if continuous {
                        "关闭连续识别"
                    } else {
                        "开启连续识别"
                    },
                )
                    .when(continuous, |el| el.bg(p.pill_bg))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            let on = !this.actions.continuous.load(Ordering::Relaxed);
                            this.actions.continuous.store(on, Ordering::Relaxed);
                            cx.global::<AppState>()
                                .ocr_continuous
                                .store(on, Ordering::Relaxed);
                            (this.actions.on_toggle_continuous)(on);
                            cx.notify();
                        }),
                    ),
            )
            .child(
                icon_button(
                    "ocr_translate",
                    icons::LANGUAGES,
                    action_color,
                    p.hover,
                    "翻译识别结果",
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if this.loading || this.text.trim().is_empty() {
                            return;
                        }
                        (this.actions.on_translate)(this.text.clone());
                    }),
                ),
            )
            .child(
                icon_button(
                    "ocr_copy",
                    copy_svg,
                    copy_color,
                    p.hover,
                    if self.copied { "已复制" } else { "复制" },
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.copy_result(&CopyResult, window, cx);
                    }),
                ),
            )
            .child(
                icon_button("ocr_close", icons::CLOSE, p.muted, p.hover, "关闭").on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.close(&CloseWindow, window, cx);
                    }),
                ),
            );

        let card = div()
            .id("ocr_card")
            .size_full()
            .flex()
            .flex_col()
            .gap(px(GAP))
            .p(px(PAD))
            .rounded(px(platform::CORNER_RADIUS as f32))
            .border_1()
            .border_color(p.border)
            .bg(p.card)
            .when(SHADOW_MARGIN > 0., |el| {
                el.shadow(vec![BoxShadow {
                    color: p.shadow,
                    offset: point(px(0.), px(4.)),
                    blur_radius: px(10.),
                    spread_radius: px(0.),
                    inset: false,
                }])
            })
            .font_family(".SystemUIFont")
            .cursor_default()
            .child(header)
            .child(div().h(px(1.)).w_full().bg(p.divider))
            .child(body);

        div()
            .id("ocr_root")
            .key_context(UTILITY_KEY_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::close))
            .on_action(cx.listener(Self::copy_result))
            .size_full()
            .p(px(SHADOW_MARGIN))
            .child(card)
    }
}
