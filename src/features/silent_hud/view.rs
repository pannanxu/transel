//! 静默 HUD：小圆角条，左侧加载图标 + 右侧关闭按钮。

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use gpui_kit::component::ActiveTheme;
use gpui_kit::component::Colorize as _;
use gpui_kit::component::Icon;
use gpui_kit::*;

use crate::features::silent_hud;
use crate::shared::icons;
use crate::shared::theme::palette;

pub struct SilentHudView {
    _label: SharedString,
    _cancel: Arc<AtomicBool>,
}

impl SilentHudView {
    pub fn new(label: SharedString, cancel: Arc<AtomicBool>) -> Self {
        Self {
            _label: label,
            _cancel: cancel,
        }
    }
}

impl Render for SilentHudView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(window.appearance());
        let accent = p.accent;

        div()
            .id("silent_hud")
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(4.))
                    .px(px(5.))
                    .py(px(4.))
                    .rounded(px(6.))
                    .bg(cx.theme().popover)
                    .border_1()
                    .border_color(cx.theme().border)
                    .shadow(vec![BoxShadow {
                        color: p.shadow.opacity(0.4),
                        offset: point(px(0.), px(2.)),
                        blur_radius: px(8.),
                        spread_radius: px(0.),
                        inset: false,
                    }])
                    .child(
                        Icon::default()
                            .path(icons::LOADER_CIRCLE)
                            .size(px(12.))
                            .text_color(accent)
                            .with_animation(
                                "silent_hud_spin",
                                Animation::new(Duration::from_millis(720))
                                    .repeat()
                                    .with_easing(|t| t),
                                |this, delta| {
                                    this.transform(Transformation::rotate(percentage(delta)))
                                },
                            ),
                    )
                    .child(
                        div()
                            .id("silent_hud_stop")
                            .size(px(16.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .cursor_pointer()
                            .hover(|s| s.bg(p.hover))
                            .active(|s| s.opacity(0.7))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|_, _, _, cx| {
                                    cx.stop_propagation();
                                    silent_hud::request_stop(cx);
                                }),
                            )
                            .child(
                                Icon::default()
                                    .path(icons::CLOSE)
                                    .size(px(11.))
                                    .text_color(p.muted),
                            ),
                    ),
            )
    }
}
