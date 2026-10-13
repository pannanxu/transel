//! 翻译历史：Kit Input 搜索 + compact 列表 + Action 关闭。

use std::sync::Arc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme, Sizable};
use gpui_kit::*;

use crate::app::history::HistoryEntry;
use crate::platform;
use crate::shared::actions::{ClearList, CloseWindow, UTILITY_KEY_CONTEXT};
use crate::shared::chrome::EmptyState;
use crate::shared::text_select::clamped_plain;
use crate::shared::tokens::SHADOW_MARGIN;

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

#[derive(IntoElement)]
struct HistoryRow {
    timestamp: SharedString,
    origin: SharedString,
    target: SharedString,
    source: SharedString,
    translation: SharedString,
    muted: Hsla,
    text: Hsla,
}

impl RenderOnce for HistoryRow {
    fn render(self, _: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .mx_3()
            .my_1()
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .p_2()
            .rounded(px(6.))
            .bg(_cx.theme().muted.opacity(0.35))
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(div().text_xs().text_color(self.muted).child(self.timestamp))
                    .child(div().text_xs().text_color(self.muted).child(self.origin))
                    .child(div().flex_1())
                    .child(div().text_xs().text_color(self.muted).child(self.target)),
            )
            .child(clamped_plain(self.source, self.text, 13., 19., 2))
            .child(clamped_plain(self.translation, self.muted, 13., 19., 2))
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
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("搜索原文 / 译文…"));
        let subscription = cx.subscribe(&query, |_, _, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Change) {
                cx.notify();
            }
        });
        let focus = cx.focus_handle().tab_stop(true);
        focus.focus(window, cx);
        Self {
            entries,
            query,
            actions,
            focus,
            _subscriptions: vec![subscription],
        }
    }

    fn close(&mut self, _: &CloseWindow, window: &mut Window, _cx: &mut Context<Self>) {
        (self.actions.on_close)();
        window.remove_window();
    }

    fn clear_list(&mut self, _: &ClearList, _window: &mut Window, cx: &mut Context<Self>) {
        (self.actions.on_clear)();
        self.entries.clear();
        cx.notify();
    }
}

impl Render for HistoryView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let muted = theme.muted_foreground;
        let text = theme.foreground;
        let q = self.query.read(cx).value().to_ascii_lowercase();

        let mut list: Vec<AnyElement> = Vec::new();
        for e in self.entries.iter().rev() {
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
            let primary = e
                .results
                .iter()
                .find(|r| r.success)
                .map(|r| r.text.clone())
                .unwrap_or_else(|| "(失败)".into());
            list.push(
                HistoryRow {
                    timestamp: e.ts.format("%Y-%m-%d %H:%M:%S").to_string().into(),
                    origin: format!("{:?}", e.origin).into(),
                    target: format!("→ {}", e.target_lang).into(),
                    source: e.source.clone().into(),
                    translation: primary.into(),
                    muted,
                    text,
                }
                .into_any_element(),
            );
        }

        let body: AnyElement = if list.is_empty() {
            EmptyState::new("还没有翻译记录")
                .detail("划词翻译后会出现在这里。")
                .into_any_element()
        } else {
            div()
                .id("history_list")
                .flex_1()
                .min_h_0()
                .p_1()
                .overflow_y_scrollbar()
                .children(list)
                .into_any_element()
        };

        let card = div()
            .id("history_card")
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
                            .child("历史"),
                    )
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
                    )
                    .child(
                        Button::new("hist_close")
                            .ghost()
                            .label("关闭")
                            .with_size(gpui_kit::component::Size::Small)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close(&CloseWindow, window, cx);
                            })),
                    ),
            )
            .child(div().px_3().pt_2().child(Input::new(&self.query).w_full()))
            .child(body);

        div()
            .id("history_root")
            .key_context(UTILITY_KEY_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::close))
            .on_action(cx.listener(Self::clear_list))
            .size_full()
            .p(px(SHADOW_MARGIN))
            .child(card)
    }
}
