//! 原图翻译覆盖层：在截图上按 OCR 框叠加译文。

use std::sync::Arc;

use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::platform;
use crate::services::ocr::TextBox;
use crate::shared::actions::{CloseWindow, CopyResult, OVERLAY_KEY_CONTEXT};
use crate::shared::chrome::EmptyState;
use crate::shared::icons;
use crate::shared::loading;
use crate::shared::theme::palette;
use crate::shared::tokens::SHADOW_MARGIN;
use crate::shared::tooltip;

const PAD: f32 = 14.;
const HEADER_H: f32 = 24.;
const GAP: f32 = 10.;

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
    pub copied: bool,
    pub actions: OverlayActions,
    focus: FocusHandle,
    drag: Option<platform::ManualDrag>,
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
            copied: false,
            actions,
            focus,
            drag: None,
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
            copied: false,
            actions,
            focus,
            drag: None,
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

    fn copy_result(&mut self, _: &CopyResult, _window: &mut Window, cx: &mut Context<Self>) {
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
        self.copied = true;
        cx.notify();
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

impl Render for OverlayView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(window.appearance());
        let theme = cx.theme();
        let busy = self.loading;
        let status = self.status.clone();
        let empty_copy = self.blocks.iter().all(|b| b.translation.trim().is_empty());
        let copy_svg = if self.copied {
            icons::CHECK
        } else {
            icons::COPY
        };
        let copy_color = if self.copied {
            p.accent
        } else if busy || empty_copy {
            p.muted.opacity(0.35)
        } else {
            p.muted
        };

        let box_bg = theme.primary.opacity(0.82);
        let box_fg = theme.primary_foreground;
        let canvas_bg = theme.background.opacity(0.92);

        let canvas: AnyElement = if self.loading {
            loading::loading_block(status.clone().unwrap_or_else(|| "处理中…".into()), &p)
        } else if self.blocks.is_empty() {
            EmptyState::new(status.clone().unwrap_or_else(|| "没有结果".into()))
                .detail("未检测到可用文本框。")
                .centered()
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
                        .rounded(px(4.))
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
                .min_h_0()
                .relative()
                .rounded(px(8.))
                .bg(canvas_bg)
                .overflow_hidden()
                .children(boxes)
                .into_any_element()
        };

        let header = div()
            .id("ov_header")
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
                            .child("原图翻译"),
                    ),
            )
            .when_some(status.filter(|_| busy), |el, s| {
                el.child(div().text_size(px(11.)).text_color(p.muted).child(s))
            })
            .child(div().flex_1())
            .child(
                icon_button(
                    "ov_copy",
                    copy_svg,
                    copy_color,
                    p.hover,
                    if self.copied {
                        "已复制全部"
                    } else {
                        "复制全部"
                    },
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
                icon_button("ov_close", icons::CLOSE, p.muted, p.hover, "关闭").on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.close(&CloseWindow, window, cx);
                    }),
                ),
            );

        let card = div()
            .id("ov_card")
            .size_full()
            .flex()
            .flex_col()
            .gap(px(GAP))
            .p(px(PAD))
            .rounded(px(platform::CORNER_RADIUS as f32))
            .border_1()
            .border_color(p.border)
            .bg(p.card)
            .overflow_hidden()
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
            .child(header)
            .child(div().h(px(1.)).w_full().bg(p.divider))
            .child(canvas);

        div()
            .id("ov_root")
            .key_context(OVERLAY_KEY_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::close))
            .on_action(cx.listener(Self::copy_result))
            .size_full()
            .p(px(SHADOW_MARGIN))
            .child(card)
    }
}
