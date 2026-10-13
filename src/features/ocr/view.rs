//! 独立 OCR 结果窗口。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, Disableable as _, Sizable};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::platform;
use crate::shared::actions::{CloseWindow, CopyResult, UTILITY_KEY_CONTEXT};
use crate::shared::chrome::EmptyState;
use crate::shared::loading;
use crate::shared::theme::palette;
use crate::shared::tokens::SHADOW_MARGIN;

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
        }
    }

    pub fn loading(actions: OcrActions, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle().tab_stop(true);
        focus.focus(window, cx);
        Self {
            text: String::new(),
            status: Some("识别中…".into()),
            copied: false,
            loading: true,
            actions,
            focus,
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

    pub fn apply_result(&mut self, new_text: &str, append: bool) {
        let t = new_text.trim();
        self.loading = false;
        if t.is_empty() {
            self.status = Some("没有识别出文字".into());
            return;
        }
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

impl Render for OcrView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(window.appearance());
        let theme = cx.theme();
        let continuous = self.actions.continuous.load(Ordering::Relaxed);
        let busy = self.loading;
        let status = self.status.clone();
        let status_bar = status.clone().filter(|_| busy || self.text.is_empty());

        let body: AnyElement = if self.loading {
            div()
                .flex_1()
                .p_3()
                .child(loading::loading_block("正在识别…", &p))
                .into_any_element()
        } else if self.text.is_empty() {
            EmptyState::new("识别结果会显示在这里")
                .detail(status.unwrap_or_else(|| "截图选区后开始识别".into()))
                .into_any_element()
        } else {
            div()
                .flex_1()
                .min_h_0()
                .p_3()
                .child(
                    div()
                        .id("ocr_text")
                        .size_full()
                        .p_2()
                        .rounded(px(6.))
                        .bg(theme.secondary)
                        .border_1()
                        .border_color(theme.border)
                        .overflow_y_scrollbar()
                        .text_sm()
                        .text_color(theme.foreground)
                        .child(self.text.clone()),
                )
                .into_any_element()
        };

        let card = div()
            .id("ocr_card")
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .border_1()
            .border_color(theme.border)
            .rounded(px(platform::CORNER_RADIUS as f32))
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_10()
                    .px_3()
                    .gap_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("OCR"),
                    )
                    .when_some(status_bar, |el, s| {
                        el.child(div().text_xs().text_color(theme.muted_foreground).child(s))
                    })
                    .child(div().flex_1())
                    .child(
                        Switch::new("ocr_continuous")
                            .label("连续识别")
                            .checked(continuous)
                            .on_change(cx.listener(|this, checked: &bool, _, cx| {
                                this.actions.continuous.store(*checked, Ordering::Relaxed);
                                (this.actions.on_toggle_continuous)(*checked);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("ocr_translate")
                            .primary()
                            .label("翻译")
                            .disabled(busy || self.text.is_empty())
                            .with_size(gpui_kit::component::Size::Small)
                            .on_click(cx.listener(|this, _, _, _| {
                                if this.loading || this.text.trim().is_empty() {
                                    return;
                                }
                                (this.actions.on_translate)(this.text.clone());
                            })),
                    )
                    .child(
                        Button::new("ocr_copy")
                            .ghost()
                            .label(if self.copied { "已复制" } else { "复制" })
                            .disabled(busy || self.text.is_empty())
                            .with_size(gpui_kit::component::Size::Small)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.copy_result(&CopyResult, window, cx);
                            })),
                    )
                    .child(
                        Button::new("ocr_close")
                            .ghost()
                            .label("关闭")
                            .with_size(gpui_kit::component::Size::Small)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close(&CloseWindow, window, cx);
                            })),
                    ),
            )
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
