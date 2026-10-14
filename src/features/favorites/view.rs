//! 收藏夹：系统窗口 + 搜索 + 按日分组列表。

use std::sync::Arc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::h_flex;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::v_flex;
use gpui_kit::component::{ActiveTheme, Sizable};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::app::favorites::FavoriteEntry;
use crate::app::history;
use crate::app::state::AppState;
use crate::shared::actions::{ClearList, CloseWindow, UTILITY_KEY_CONTEXT};
use crate::shared::chrome::{DaySection, EmptyState, RecordRow};
use crate::shared::theme::{lang_label, opaque_panel, sync_kit_theme_opaque};

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
    pub query: Entity<InputState>,
    pub status: Option<SharedString>,
    pub actions: FavoritesActions,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl FavoritesView {
    pub fn new(
        entries: Vec<FavoriteEntry>,
        actions: FavoritesActions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        sync_kit_theme_opaque(Some(window), cx);
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("搜索收藏…"));
        let subscription = cx.subscribe(&query, |_, _, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Change) {
                cx.notify();
            }
        });
        let appearance = cx.observe_window_appearance(window, |_, window, cx| {
            sync_kit_theme_opaque(Some(window), cx);
            cx.notify();
        });
        let focus = cx.focus_handle().tab_stop(true);
        focus.focus(window, cx);
        Self {
            entries,
            query,
            status: None,
            actions,
            focus,
            _subscriptions: vec![subscription, appearance],
        }
    }

    fn close(&mut self, _: &CloseWindow, window: &mut Window, cx: &mut Context<Self>) {
        (self.actions.on_close)();
        if cx.has_global::<AppState>() {
            *cx.global_mut::<AppState>().favorites_window.lock() = None;
        }
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let muted = theme.muted_foreground;
        let q = self.query.read(cx).value().to_ascii_lowercase();
        let total = self.entries.len();
        let status = self.status.clone();

        let mut list: Vec<AnyElement> = Vec::new();
        let mut last_day: Option<String> = None;
        let mut shown = 0usize;
        for (i, e) in self.entries.iter().rev().enumerate() {
            if !q.is_empty()
                && !e.source.to_ascii_lowercase().contains(&q)
                && !e.prepared.to_ascii_lowercase().contains(&q)
                && !e.translation.to_ascii_lowercase().contains(&q)
            {
                continue;
            }
            let day = history::day_heading(e.ts);
            if last_day.as_deref() != Some(day.as_str()) {
                list.push(DaySection::new(day.clone()).into_any_element());
                last_day = Some(day);
            }
            let tr_empty = e.translation.is_empty();
            let tr_label: SharedString = if tr_empty {
                "（无译文）".into()
            } else {
                e.translation.clone().into()
            };
            let copy_payload = if tr_empty {
                e.source.clone()
            } else {
                format!("{}\n{}", e.source, e.translation)
            };
            let id = e.id.clone();
            let langs = format!("→ {}", lang_label(&e.target_lang));
            let mut row = RecordRow::new(
                ("fav_row", i),
                e.origin.label(),
                langs,
                history::clock_label(e.ts),
                e.source.clone(),
                tr_label,
            )
            .trailing(
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
            .trailing(
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
            );
            if tr_empty {
                row = row.empty_translation();
            }
            list.push(row.into_any_element());
            shown += 1;
        }

        let body: AnyElement = if list.is_empty() {
            EmptyState::new(if q.is_empty() {
                "暂无收藏"
            } else {
                "没有匹配的收藏"
            })
            .detail(if q.is_empty() {
                "在翻译弹窗按快捷键收藏当前结果。"
            } else {
                "试试其他关键词。"
            })
            .centered()
            .into_any_element()
        } else {
            div()
                .id("fav_list")
                .flex_1()
                .min_h_0()
                .px_2()
                .pb_3()
                .overflow_y_scrollbar()
                .children(list)
                .into_any_element()
        };

        v_flex()
            .id("fav_root")
            .key_context(UTILITY_KEY_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::close))
            .on_action(cx.listener(Self::clear_list))
            .size_full()
            .overflow_hidden()
            .bg(opaque_panel(window.appearance()))
            .font_family(".SystemUIFont")
            .text_color(theme.foreground)
            .child(
                v_flex()
                    .px_4()
                    .pt_3()
                    .pb_2()
                    .gap_2()
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("收藏"),
                            )
                            .child(div().text_xs().text_color(muted).child(if q.is_empty() {
                                format!("{total} 条")
                            } else {
                                format!("{shown} / {total}")
                            }))
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
                            ),
                    )
                    .child(Input::new(&self.query).small().w_full()),
            )
            .child(body)
    }
}
