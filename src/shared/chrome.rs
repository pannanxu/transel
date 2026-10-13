//! 可复用表面：卡片壳、空状态、标题栏区域（RenderOnce 值类型）。

use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::platform;
use crate::shared::theme::Palette;

/// 透明 PopUp 窗口背景（毛玻璃 / 自绘卡片透出）。
pub fn window_background() -> WindowBackgroundAppearance {
    WindowBackgroundAppearance::Transparent
}

/// 弹窗外层卡片：主题色 + 圆角 + 可选阴影边距。
#[derive(IntoElement)]
pub struct CardShell {
    width: Option<Pixels>,
    pad: bool,
    children: Vec<AnyElement>,
}

impl CardShell {
    pub fn new() -> Self {
        Self {
            width: None,
            pad: true,
            children: Vec::new(),
        }
    }

    pub fn w(mut self, w: Pixels) -> Self {
        self.width = Some(w);
        self
    }

    pub fn no_pad(mut self) -> Self {
        self.pad = false;
        self
    }
}

impl Default for CardShell {
    fn default() -> Self {
        Self::new()
    }
}

impl ParentElement for CardShell {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for CardShell {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = crate::shared::theme::palette(window.appearance());
        let radius = px(platform::CORNER_RADIUS as f32);
        div()
            .when_some(self.width, |el, w| el.w(w))
            .flex()
            .flex_col()
            .bg(cx.theme().popover)
            .text_color(cx.theme().popover_foreground)
            .border_1()
            .border_color(cx.theme().border)
            .rounded(radius)
            .overflow_hidden()
            .when(self.pad, |el| el.p_3().gap_2())
            .shadow(if cfg!(target_os = "macos") {
                Vec::new()
            } else {
                vec![BoxShadow {
                    color: p.shadow,
                    offset: point(px(0.), px(8.)),
                    blur_radius: px(24.),
                    spread_radius: px(0.),
                    inset: false,
                }]
            })
            .children(self.children)
    }
}

/// 空 / 错误 / 无权限等提示块。
#[derive(IntoElement)]
pub struct EmptyState {
    title: SharedString,
    detail: Option<SharedString>,
    muted: bool,
}

impl EmptyState {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            detail: None,
            muted: true,
        }
    }

    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn emphasis(mut self) -> Self {
        self.muted = false;
        self
    }
}

impl RenderOnce for EmptyState {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .items_start()
            .p_3()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if self.muted {
                        cx.theme().muted_foreground
                    } else {
                        cx.theme().foreground
                    })
                    .child(self.title),
            )
            .when_some(self.detail, |el, d| {
                el.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(d),
                )
            })
    }
}

/// 紧凑工具栏行：左侧标题，右侧操作。
#[derive(IntoElement)]
pub struct Toolbar {
    title: SharedString,
    trailing: Vec<AnyElement>,
}

impl Toolbar {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            trailing: Vec::new(),
        }
    }

    pub fn trailing(mut self, el: impl IntoElement) -> Self {
        self.trailing.push(el.into_any_element());
        self
    }
}

impl RenderOnce for Toolbar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .w_full()
            .items_center()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(cx.theme().foreground)
                    .child(self.title),
            )
            .children(self.trailing)
    }
}

/// 用 Palette 给尚未迁到 theme() 的遗留绘制提供桥接（逐步淘汰）。
pub fn palette_for(window: &Window) -> Palette {
    crate::shared::theme::palette(window.appearance())
}
