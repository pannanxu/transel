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
    centered: bool,
}

impl EmptyState {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            detail: None,
            muted: true,
            centered: false,
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

    pub fn centered(mut self) -> Self {
        self.centered = true;
        self
    }
}

impl RenderOnce for EmptyState {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .when(self.centered, |el| {
                el.flex_1()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .text_center()
            })
            .when(!self.centered, |el| el.items_start())
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

/// 历史 / 收藏列表行：来源胶囊 + 语言 + 时间，下面原文 / 译文。
#[derive(IntoElement)]
pub struct RecordRow {
    id: ElementId,
    origin: SharedString,
    langs: SharedString,
    time: SharedString,
    source: SharedString,
    translation: SharedString,
    translation_empty: bool,
    trailing: Vec<AnyElement>,
}

impl RecordRow {
    pub fn new(
        id: impl Into<ElementId>,
        origin: impl Into<SharedString>,
        langs: impl Into<SharedString>,
        time: impl Into<SharedString>,
        source: impl Into<SharedString>,
        translation: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            origin: origin.into(),
            langs: langs.into(),
            time: time.into(),
            source: source.into(),
            translation: translation.into(),
            translation_empty: false,
            trailing: Vec::new(),
        }
    }

    pub fn empty_translation(mut self) -> Self {
        self.translation_empty = true;
        self
    }

    pub fn trailing(mut self, el: impl IntoElement) -> Self {
        self.trailing.push(el.into_any_element());
        self
    }
}

impl RenderOnce for RecordRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let muted = theme.muted_foreground;
        let text = theme.foreground;
        let chip = theme.muted;
        div()
            .id(self.id)
            .w_full()
            .min_w_0()
            .px_3()
            .py_2()
            .rounded(px(10.))
            .hover(|s| s.bg(theme.muted.opacity(0.45)))
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .px(px(6.))
                            .py(px(1.))
                            .rounded(px(4.))
                            .bg(chip)
                            .text_xs()
                            .text_color(muted)
                            .child(self.origin),
                    )
                    .child(div().text_xs().text_color(muted).child(self.langs))
                    .child(div().flex_1())
                    .child(div().text_xs().text_color(muted).child(self.time))
                    .children(self.trailing),
            )
            .child(crate::shared::text_select::clamped_plain(
                self.source,
                text,
                13.,
                19.,
                2,
            ))
            .child(crate::shared::text_select::clamped_plain(
                self.translation,
                if self.translation_empty {
                    muted
                } else {
                    text.opacity(0.72)
                },
                13.,
                19.,
                2,
            ))
    }
}

#[derive(IntoElement)]
pub struct DaySection {
    label: SharedString,
}

impl DaySection {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
        }
    }
}

impl RenderOnce for DaySection {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .pt_3()
            .pb_1()
            .px_3()
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .text_color(cx.theme().muted_foreground)
            .child(self.label)
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
