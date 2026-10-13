//! 收藏夹：Kit Button + EmptyState + Action 关闭。

use std::sync::Arc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme, Sizable};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::app::favorites::FavoriteEntry;
use crate::platform;
use crate::shared::actions::{ClearList, CloseWindow, UTILITY_KEY_CONTEXT};
use crate::shared::chrome::EmptyState;
use crate::shared::text_select::clamped_plain;
use crate::shared::tokens::SHADOW_MARGIN;

#[derive(Clone)]
pub struct FavoritesActions {
    pub on_close: Arc<dyn Fn() + Send + Sync>,
    pub on_clear: Arc<dyn Fn() + Send + Sync>,
    pub on_export: Arc<dyn Fn() + Send + Sync>,
    pub on_remove: Arc<dyn Fn(String) + Send + Sync>,
    pub on_copy: Arc<dyn Fn(String) + Send + Sync>,
}

impl Default for FavoritesActions {
    fn default() -> Self {
        Self {
            on_close: Arc::new(|| {}),
            on_clear: Arc::new(|| {}),
            on_export: Arc::new(|| {}),
            on_remove: Arc::new(|_| {}),
            on_copy: Arc::new(|_| {}),
        }
    }
}

pub struct FavoritesView {
    pub entries: Vec<FavoriteEntry>,
    pub status: Option<SharedString>,
    pub actions: FavoritesActions,
    focus: FocusHandle,
}

impl FavoritesView {
    pub fn new(
        entries: Vec<FavoriteEntry>,
        actions: FavoritesActions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle().tab_stop(true);
        focus.focus(window, cx);
        Self {
            entries,
            status: None,
            actions,
            focus,
        }
    }

    fn close(&mut self, _: &CloseWindow, window: &mut Window, _cx: &mut Context<Self>) {
        (self.actions.on_close)();
        window.remove_window();
    }

    fn clear_list(&mut self, _: &ClearList, _window: &mut Window, cx: &mut Context<Self>) {
        (self.actions.on_clear)();
        self.entries.clear();
        self.status = Some("已清空".into());
        cx.notify();
    }
}

impl Render for FavoritesView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let text = theme.foreground;
        let count = self.entries.len();
        let status = self.status.clone();

        let mut rows: Vec<AnyElement> = Vec::new();
        for (i, e) in self.entries.iter().rev().enumerate() {
            let id = e.id.clone();
            let copy_payload = if e.translation.is_empty() {
                e.source.clone()
            } else {
                format!("{}\n{}", e.source, e.translation)
            };
            let src = e.source.clone();
            let tr = e.translation.clone();
            let tr_empty = tr.is_empty();
            let tr_label: SharedString = if tr_empty {
                "（无译文）".into()
            } else {
                tr.clone().into()
            };

            rows.push(
                div()
                    .id(("fav_row", i))
                    .w_full()
                    .min_w_0()
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .rounded(px(6.))
                    .bg(theme.secondary)
                    .border_1()
                    .border_color(theme.border)
                    .child(clamped_plain(src, muted, 12., 18., 2))
                    .child(clamped_plain(
                        tr_label,
                        if tr_empty { muted } else { text },
                        13.5,
                        20.,
                        2,
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(
                                Button::new(("fav_copy", i))
                                    .ghost()
                                    .label("复制")
                                    .with_size(gpui_kit::component::Size::XSmall)
                                    .on_click({
                                        let copy_payload = copy_payload.clone();
                                        cx.listener(move |this, _, _, _| {
                                            (this.actions.on_copy)(copy_payload.clone());
                                        })
                                    }),
                            )
                            .child(
                                Button::new(("fav_del", i))
                                    .danger()
                                    .ghost()
                                    .label("删除")
                                    .with_size(gpui_kit::component::Size::XSmall)
                                    .on_click({
                                        let id = id.clone();
                                        cx.listener(move |this, _, _, cx| {
                                            (this.actions.on_remove)(id.clone());
                                            this.entries.retain(|e| e.id != id);
                                            cx.notify();
                                        })
                                    }),
                            ),
                    )
                    .into_any_element(),
            );
        }

        let body: AnyElement = if rows.is_empty() {
            EmptyState::new("暂无收藏")
                .detail("在翻译弹窗按快捷键收藏当前结果。")
                .into_any_element()
        } else {
            div()
                .id("fav_list")
                .flex_1()
                .min_h_0()
                .p_3()
                .gap_2()
                .flex()
                .flex_col()
                .overflow_y_scrollbar()
                .children(rows)
                .into_any_element()
        };

        let card = div()
            .id("fav_card")
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
                            .child("收藏"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(format!("{count} 条")),
                    )
                    .when_some(status, |el, s| {
                        el.child(div().text_xs().text_color(muted).child(s))
                    })
                    .child(div().flex_1())
                    .child(
                        Button::new("fav_export")
                            .ghost()
                            .label("导出")
                            .with_size(gpui_kit::component::Size::Small)
                            .on_click(cx.listener(|this, _, _, cx| {
                                (this.actions.on_export)();
                                this.status = Some("已复制导出文本".into());
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("fav_clear")
                            .danger()
                            .ghost()
                            .label("清空")
                            .with_size(gpui_kit::component::Size::Small)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.clear_list(&ClearList, window, cx);
                            })),
                    )
                    .child(
                        Button::new("fav_close")
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
            .id("fav_root")
            .key_context(UTILITY_KEY_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::close))
            .on_action(cx.listener(Self::clear_list))
            .size_full()
            .p(px(SHADOW_MARGIN))
            .child(card)
    }
}
