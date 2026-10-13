//! 原图翻译覆盖层：在截图上按 OCR 框叠加译文。

use std::sync::Arc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme, Disableable as _, Sizable};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::platform;
use crate::services::ocr::TextBox;
use crate::shared::actions::{CloseWindow, CopyResult, UTILITY_KEY_CONTEXT};
use crate::shared::chrome::EmptyState;
use crate::shared::loading;
use crate::shared::theme::palette;
use crate::shared::tokens::SHADOW_MARGIN;

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
    pub status: Option<SharedString>,
    pub actions: OverlayActions,
    focus: FocusHandle,
}

impl OverlayView {
    pub fn new(
        blocks: Vec<OverlayBlock>,
        image_w: f32,
        image_h: f32,
        actions: OverlayActions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle().tab_stop(true);
        focus.focus(window, cx);
        Self {
            blocks,
            image_w: image_w.max(1.),
            image_h: image_h.max(1.),
            loading: false,
            status: None,
            actions,
            focus,
        }
    }

    pub fn loading(actions: OverlayActions, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle().tab_stop(true);
        focus.focus(window, cx);
        Self {
            blocks: Vec::new(),
            image_w: 640.,
            image_h: 420.,
            loading: true,
            status: Some("识别中…".into()),
            actions,
            focus,
        }
    }

    pub fn set_status(&mut self, msg: impl Into<SharedString>) {
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

    pub fn set_error(&mut self, msg: impl Into<SharedString>) {
        self.loading = false;
        self.status = Some(msg.into());
    }

    fn close(&mut self, _: &CloseWindow, window: &mut Window, _cx: &mut Context<Self>) {
        (self.actions.on_close)();
        window.remove_window();
    }

    fn copy_result(&mut self, _: &CopyResult, _window: &mut Window, _cx: &mut Context<Self>) {
        let all_text = self
            .blocks
            .iter()
            .map(|b| b.translation.clone())
            .filter(|t| !t.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if self.loading || all_text.trim().is_empty() {
            return;
        }
        (self.actions.on_copy_all)(all_text);
    }
}

impl Render for OverlayView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(window.appearance());
        let theme = cx.theme();
        let busy = self.loading;
        let status = self.status.clone();
        let empty_copy = self.blocks.iter().all(|b| b.translation.trim().is_empty());

        // Overlay box fill: semantic primary with alpha — raster/overlay exception.
        let box_bg = theme.primary.opacity(0.82);
        let box_fg = theme.primary_foreground;
        let canvas_bg = theme.background.opacity(0.92);

        let canvas: AnyElement = if self.loading {
            div()
                .flex_1()
                .bg(canvas_bg)
                .child(loading::loading_block(
                    status.clone().unwrap_or_else(|| "处理中…".into()),
                    &p,
                ))
                .into_any_element()
        } else if self.blocks.is_empty() {
            EmptyState::new(status.unwrap_or_else(|| "没有结果".into()))
                .detail("未检测到可用文本框。")
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
                        .px_1()
                        .py_0p5()
                        .rounded(px(3.))
                        .bg(box_bg)
                        .text_xs()
                        .text_color(box_fg)
                        .overflow_hidden()
                        .child(text)
                        .into_any_element(),
                );
            }
            div()
                .id("ov_canvas")
                .flex_1()
                .relative()
                .bg(canvas_bg)
                .overflow_hidden()
                .children(boxes)
                .into_any_element()
        };

        let card = div()
            .id("ov_card")
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
                    .h_9()
                    .px_3()
                    .gap_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("原图翻译"),
                    )
                    .when_some(self.status.clone(), |el, s| {
                        el.child(div().text_xs().text_color(theme.muted_foreground).child(s))
                    })
                    .child(div().flex_1())
                    .child(
                        Button::new("ov_copy")
                            .primary()
                            .label("复制全部")
                            .disabled(busy || empty_copy)
                            .with_size(gpui_kit::component::Size::Small)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.copy_result(&CopyResult, window, cx);
                            })),
                    )
                    .child(
                        Button::new("ov_close")
                            .ghost()
                            .label("关闭")
                            .with_size(gpui_kit::component::Size::Small)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close(&CloseWindow, window, cx);
                            })),
                    ),
            )
            .child(canvas);

        div()
            .id("ov_root")
            .key_context(UTILITY_KEY_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::close))
            .on_action(cx.listener(Self::copy_result))
            .size_full()
            .p(px(SHADOW_MARGIN))
            .child(card)
    }
}
