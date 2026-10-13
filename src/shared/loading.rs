//! 统一加载态：圆弧 Spinner + ShimmerText（RenderOnce 风格的函数组件）。

use gpui_kit::component::Icon;
use gpui_kit::component::Sizable;
use gpui_kit::component::shimmer::ShimmerText;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::*;

use crate::shared::icons;
use crate::shared::theme::Palette;

fn accent_spinner(size: gpui_kit::component::Size, color: Hsla) -> Spinner {
    Spinner::new()
        .icon(Icon::default().path(icons::LOADER_CIRCLE))
        .with_size(size)
        .color(color)
        .ease(|t| t)
}

/// 行内加载指示。
pub fn loading_row(label: impl Into<SharedString>, p: &Palette) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(accent_spinner(gpui_kit::component::Size::Small, p.accent))
        .child(
            ShimmerText::new(label)
                .id("loading_shimmer")
                .text_sm()
                .text_color(p.muted),
        )
        .into_any_element()
}

/// 居中大块加载区。
pub fn loading_block(label: impl Into<SharedString>, p: &Palette) -> AnyElement {
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_3()
        .child(accent_spinner(gpui_kit::component::Size::Large, p.accent))
        .child(
            ShimmerText::new(label)
                .id("loading_block_shimmer")
                .text_sm()
                .text_color(p.muted),
        )
        .into_any_element()
}
