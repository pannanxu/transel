//! 翻译历史：系统窗口 + 搜索 + 按日分组列表。

use std::sync::Arc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::h_flex;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::v_flex;
use gpui_kit::component::{ActiveTheme, Sizable};
use gpui_kit::*;

use crate::app::history::{self, HistoryEntry};
use crate::app::state::AppState;
use crate::shared::actions::{ClearList, CloseWindow, UTILITY_KEY_CONTEXT};
use crate::shared::chrome::{DaySection, EmptyState, RecordRow};
use crate::shared::theme::{lang_label, opaque_panel, sync_kit_theme_opaque};

#[derive(Clone)]
pub struct HistoryActions {
    pub on_close: Arc<dyn Fn() + Send + Sync>,
    pub on_clear: Arc<dyn Fn() + Send + Sync>,
}

impl Default for HistoryActions {
    fn default() -> Self {
        Self {
            on_close: Arc::new(|| {}),
            on_clear: Arc::new(|| {}),
        }
    }
}

pub struct HistoryView {
    pub entries: Vec<HistoryEntry>,
    pub query: Entity<InputState>,
    pub actions: HistoryActions,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl HistoryView {
    pub fn new(
        entries: Vec<HistoryEntry>,
        actions: HistoryActions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        sync_kit_theme_opaque(Some(window), cx);
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("搜索原文 / 译文…"));
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
            actions,
            focus,
            _subscriptions: vec![subscription, appearance],
        }
    }

    fn close(&mut self, _: &CloseWindow, window: &mut Window, cx: &mut Context<Self>) {
        (self.actions.on_close)();
        if cx.has_global::<AppState>() {
            *cx.global_mut::<AppState>().history_window.lock() = None;
        }
        window.remove_window();
    }

    fn clear_list(&mut self, _: &ClearList, _window: &mut Window, cx: &mut Context<Self>) {
        (self.actions.on_clear)();
        self.entries.clear();
        cx.notify();
    }
}

impl Render for HistoryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let muted = theme.muted_foreground;
        let q = self.query.read(cx).value().to_ascii_lowercase();
        let total = self.entries.len();

        let mut list: Vec<AnyElement> = Vec::new();
        let mut last_day: Option<String> = None;
        let mut shown = 0usize;
        for (i, e) in self.entries.iter().rev().enumerate() {
            if !q.is_empty()
                && !e.source.to_ascii_lowercase().contains(&q)
                && !e.prepared.to_ascii_lowercase().contains(&q)
                && !e
                    .results
                    .iter()
                    .any(|r| r.text.to_ascii_lowercase().contains(&q))
            {
                continue;
            }
            let day = history::day_heading(e.ts);
            if last_day.as_deref() != Some(day.as_str()) {
                list.push(DaySection::new(day.clone()).into_any_element());
                last_day = Some(day);
            }
            let primary = e
                .results
                .iter()
                .find(|r| r.success)
                .map(|r| r.text.clone())
                .unwrap_or_else(|| "翻译失败".into());
            let langs = match e.source_lang.as_deref() {
                Some(src) if !src.is_empty() && src != "auto" => {
                    format!("{} → {}", lang_label(src), lang_label(&e.target_lang))
                }
                _ => format!("→ {}", lang_label(&e.target_lang)),
            };
            let row = RecordRow::new(
                ("hist_row", i),
                e.origin.label(),
                langs,
                history::clock_label(e.ts),
                e.source.clone(),
                primary,
            );
            list.push(row.into_any_element());
            shown += 1;
        }

        let body: AnyElement = if list.is_empty() {
            EmptyState::new(if q.is_empty() {
                "还没有翻译记录"
            } else {
                "没有匹配的记录"
            })
            .detail(if q.is_empty() {
                "划词或输入翻译后会出现在这里。"
            } else {
                "试试其他关键词。"
            })
            .centered()
            .into_any_element()
        } else {
            div()
                .id("history_list")
                .flex_1()
                .min_h_0()
                .px_2()
                .pb_3()
                .overflow_y_scrollbar()
                .children(list)
                .into_any_element()
        };

        v_flex()
            .id("history_root")
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
                                    .child("历史"),
                            )
                            .child(div().text_xs().text_color(muted).child(if q.is_empty() {
                                format!("{total} 条")
                            } else {
                                format!("{shown} / {total}")
                            }))
                            .child(div().flex_1())
                            .child(
                                Button::new("hist_clear")
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
