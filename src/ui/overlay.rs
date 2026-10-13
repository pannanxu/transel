//! 原图翻译覆盖层：在截图上按 OCR 框叠加译文。

use std::sync::Arc;

use gpui_kit::*;

use crate::services::ocr::TextBox;
use crate::ui::loading;
use crate::ui::theme::palette;

use super::icons;

#[derive(Clone)]
pub struct OverlayBlock {
    pub box_: TextBox,
    pub translation: String,
}

#[derive(Clone)]
pub struct OverlayActions {
    pub on_close: Arc<dyn Fn() + Send + Sync>,
    pub on_copy_all: Arc<dyn Fn(String) + Send + Sync>,
}

impl Default for OverlayActions {
    fn default() -> Self {
        Self {
            on_close: Arc::new(|| {}),
            on_copy_all: Arc::new(|_| {}),
        }
    }
}

pub struct OverlayView {
    pub blocks: Vec<OverlayBlock>,
    pub image_w: f32,
    pub image_h: f32,
    pub loading: bool,
    pub status: Option<String>,
    pub actions: OverlayActions,
}

impl OverlayView {
    pub fn new(blocks: Vec<OverlayBlock>, image_w: f32, image_h: f32, actions: OverlayActions) -> Self {
        Self {
            blocks,
            image_w: image_w.max(1.),
            image_h: image_h.max(1.),
            loading: false,
            status: None,
            actions,
        }
    }

    pub fn loading(actions: OverlayActions) -> Self {
        Self {
            blocks: Vec::new(),
            image_w: 640.,
            image_h: 420.,
            loading: true,
            status: Some("识别中…".into()),
            actions,
        }
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.loading = true;
        self.status = Some(msg.into());
    }

    pub fn set_blocks(&mut self, blocks: Vec<OverlayBlock>, image_w: f32, image_h: f32) {
        self.blocks = blocks;
        self.image_w = image_w.max(1.);
        self.image_h = image_h.max(1.);
        self.loading = false;
        self.status = None;
    }

    pub fn set_error(&mut self, msg: impl Into<String>) {
        self.loading = false;
        self.status = Some(msg.into());
    }
}

impl Render for OverlayView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(window.appearance());
        let all_text = self
            .blocks
            .iter()
            .map(|b| b.translation.clone())
            .filter(|t| !t.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        let busy = self.loading;

        let close_btn = div()
            .id("ov_close")
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

        let copy_btn = div()
            .id("ov_copy")
            .px(px(10.))
            .py(px(4.))
            .rounded(px(5.))
            .bg(p.accent)
            .text_size(px(11.5))
            .text_color(hsla(0., 0., 1., 1.))
            .cursor_pointer()
            .opacity(if busy || all_text.is_empty() { 0.4 } else { 1. })
            .child("复制全部")
            .on_click({
                let all_text = all_text.clone();
                cx.listener(move |this, _, _, _cx| {
                    if this.loading || all_text.trim().is_empty() {
                        return;
                    }
                    (this.actions.on_copy_all)(all_text.clone());
                })
            });

        let topbar = div()
            .flex()
            .flex_row()
            .items_center()
            .h(px(36.))
            .px(px(12.))
            .gap(px(8.))
            .bg(hsla(0., 0., 0., 0.55))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(hsla(0., 0., 1., 0.95))
                    .child("原图翻译"),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(hsla(0., 0., 1., 0.65))
                    .child(self.status.clone().unwrap_or_default()),
            )
            .child(div().flex_1())
            .child(copy_btn)
            .child(close_btn);

        let canvas: AnyElement = if self.loading {
            div()
                .flex_1()
                .bg(hsla(0., 0., 0.12, 1.))
                .child(loading::loading_block(
                    self.status.clone().unwrap_or_else(|| "处理中…".into()),
                    &p,
                ))
                .into_any_element()
        } else if self.blocks.is_empty() {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .bg(hsla(0., 0., 0.12, 1.))
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(p.muted)
                        .child(self.status.clone().unwrap_or_else(|| "没有结果".into())),
                )
                .into_any_element()
        } else {
            let mut boxes: Vec<AnyElement> = Vec::new();
            for (i, b) in self.blocks.iter().enumerate() {
                let text = if b.translation.trim().is_empty() {
                    b.box_.text.clone()
                } else {
                    b.translation.clone()
                };
                let x = b.box_.x * 100.;
                let y = (1.0 - b.box_.y - b.box_.h) * 100.;
                let w = b.box_.w * 100.;
                let h = b.box_.h * 100.;
                boxes.push(
                    div()
                        .id(("ov_box", i))
                        .absolute()
                        .left(relative(x / 100.))
                        .top(relative(y / 100.))
                        .w(relative(w / 100.))
                        .min_h(relative((h / 100.).max(0.02)))
                        .px(px(4.))
                        .py(px(2.))
                        .rounded(px(3.))
                        .bg(hsla(0.55, 0.55, 0.22, 0.82))
                        .text_size(px(12.))
                        .line_height(px(16.))
                        .text_color(hsla(0., 0., 1., 0.96))
                        .overflow_hidden()
                        .child(text)
                        .into_any_element(),
                );
            }
            div()
                .id("ov_canvas")
                .flex_1()
                .relative()
                .bg(hsla(0., 0., 0.12, 1.))
                .overflow_hidden()
                .children(boxes)
                .into_any_element()
        };

        let card = div()
            .id("ov_card")
            .size_full()
            .flex()
            .flex_col()
            .bg(p.card)
            .border_1()
            .border_color(p.border)
            .rounded(px(crate::platform::CORNER_RADIUS as f32))
            .font_family(".SystemUIFont")
            .child(topbar)
            .child(canvas);

        div()
            .id("ov_root")
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
