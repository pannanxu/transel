//! 统一加载态：旋转 Spinner + 闪烁文案。

use gpui_kit::component::Sizable;
use gpui_kit::component::shimmer::ShimmerText;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::*;

use crate::ui::theme::Palette;

/// 行内加载指示：Spinner + ShimmerText。
pub fn loading_row(label: impl Into<SharedString>, p: &Palette) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(10.))
        .child(Spinner::new().with_size(gpui_kit::component::Size::Small).color(p.accent))
        .child(
            ShimmerText::new(label)
                .id("loading_shimmer")
                .text_size(px(14.))
                .text_color(p.muted),
        )
        .into_any_element()
}

/// 居中大块加载区（OCR 面板等）。
pub fn loading_block(label: impl Into<SharedString>, p: &Palette) -> AnyElement {
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(12.))
        .child(Spinner::new().with_size(gpui_kit::component::Size::Large).color(p.accent))
        .child(
            ShimmerText::new(label)
                .id("loading_block_shimmer")
                .text_size(px(14.))
                .text_color(p.muted),
        )
        .into_any_element()
}
