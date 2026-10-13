//! 独立 OCR 结果窗口（不自动翻译）。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::*;

use crate::ui::loading;
use crate::ui::theme::palette;

use super::icons;

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
    pub status: Option<String>,
    pub copied: bool,
    pub loading: bool,
    pub actions: OcrActions,
}

impl OcrView {
    pub fn new(text: String, actions: OcrActions) -> Self {
        Self {
            text,
            status: None,
            copied: false,
            loading: false,
            actions,
        }
    }

    pub fn loading(actions: OcrActions) -> Self {
        Self {
            text: String::new(),
            status: Some("识别中…".into()),
            copied: false,
            loading: true,
            actions,
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

    /// 连续识别：追加文本；否则替换。结束后退出 loading。
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

    pub fn set_error(&mut self, msg: impl Into<String>) {
        self.loading = false;
        self.status = Some(msg.into());
    }
}

impl Render for OcrView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(window.appearance());
        let continuous = self.actions.continuous.load(Ordering::Relaxed);
        let busy = self.loading;

        let close_btn = div()
            .id("ocr_close")
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

        let copy_svg = if self.copied { icons::CHECK } else { icons::COPY };
        let copy_color = if self.copied { p.accent } else { p.muted };
        let text_for_copy = self.text.clone();
        let copy_btn = div()
            .id("ocr_copy")
            .size(px(22.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .cursor_pointer()
            .hover(|s| s.bg(p.hover))
            .opacity(if busy || self.text.is_empty() { 0.4 } else { 1. })
            .child(svg().path(copy_svg).size(px(14.)).text_color(copy_color))
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.loading || this.text.trim().is_empty() {
                    return;
                }
                let t = text_for_copy.clone();
                cx.write_to_clipboard(ClipboardItem::new_string(t.clone()));
                (this.actions.on_copy)(t);
                this.copied = true;
                cx.notify();
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(Duration::from_millis(1200)).await;
                    let _ = this.update(cx, |this, cx| {
                        this.copied = false;
                        cx.notify();
                    });
                })
                .detach();
            }));

        let text_for_tr = self.text.clone();
        let translate_btn = div()
            .id("ocr_translate")
            .px(px(10.))
            .py(px(4.))
            .rounded(px(5.))
            .bg(p.accent)
            .hover(|s| s.bg(p.accent_hover))
            .text_size(px(11.5))
            .text_color(hsla(0., 0., 1., 1.))
            .cursor_pointer()
            .opacity(if busy || self.text.is_empty() { 0.4 } else { 1. })
            .child("翻译")
            .on_click(cx.listener(move |this, _, _, _cx| {
                if this.loading || this.text.trim().is_empty() {
                    return;
                }
                (this.actions.on_translate)(text_for_tr.clone());
            }));

        let cont_btn = div()
            .id("ocr_continuous")
            .px(px(10.))
            .py(px(4.))
            .rounded(px(5.))
            .bg(if continuous { p.accent } else { p.tab_inactive })
            .text_size(px(11.5))
            .text_color(if continuous {
                hsla(0., 0., 1., 1.)
            } else {
                p.text
            })
            .cursor_pointer()
            .child(if continuous { "连续识别 · 开" } else { "连续识别 · 关" })
            .on_click(cx.listener(|this, _, _, cx| {
                let new = !this.actions.continuous.load(Ordering::Relaxed);
                this.actions.continuous.store(new, Ordering::Relaxed);
                (this.actions.on_toggle_continuous)(new);
                cx.notify();
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
                    .child("OCR"),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .child(self.status.clone().unwrap_or_default()),
            )
            .child(div().flex_1())
            .child(cont_btn)
            .child(translate_btn)
            .child(copy_btn)
            .child(close_btn);

        let body = if self.loading {
            div()
                .flex_1()
                .p(px(14.))
                .child(loading::loading_block("正在识别…", &p))
        } else {
            let content = if self.text.is_empty() {
                "识别结果会显示在这里".to_string()
            } else {
                self.text.clone()
            };
            let color = if self.text.is_empty() { p.muted } else { p.text };
            div()
                .flex_1()
                .min_h(px(0.))
                .p(px(14.))
                .child(
                    div()
                        .id("ocr_text")
                        .size_full()
                        .p(px(10.))
                        .rounded(px(6.))
                        .bg(p.input_bg)
                        .border_1()
                        .border_color(p.border)
                        .overflow_y_scrollbar()
                        .text_size(px(13.5))
                        .line_height(px(20.))
                        .text_color(color)
                        .child(content),
                )
        };

        let card = div()
            .id("ocr_card")
            .size_full()
            .flex()
            .flex_col()
            .bg(p.card)
            .border_1()
            .border_color(p.border)
            .rounded(px(crate::platform::CORNER_RADIUS as f32))
            .font_family(".SystemUIFont")
            .child(topbar)
            .child(body);

        div()
            .id("ocr_root")
            .size_full()
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, _cx| {
                if ev.keystroke.key == "escape" {
                    (this.actions.on_close)();
                    window.remove_window();
                }
            }))
            .child(card)
    }
}
