//! 翻译历史窗口：搜索框 + 列表。

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::*;

use crate::app::history::HistoryEntry;
use crate::ui::text_select::selectable_plain;
use crate::ui::theme::palette;

use super::icons;

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
    /// 搜索词（通过 Rc 共享到 on_key_down 闭包里）
    pub query: Rc<RefCell<String>>,
    pub actions: HistoryActions,
}

impl HistoryView {
    pub fn new(entries: Vec<HistoryEntry>, actions: HistoryActions) -> Self {
        Self {
            entries,
            query: Rc::new(RefCell::new(String::new())),
            actions,
        }
    }
}

impl Render for HistoryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(window.appearance());
        let focus = cx.focus_handle();
        let on_clear = self.actions.on_clear.clone();
        let query = self.query.clone();

        let close_btn = div()
            .id("close_btn")
            .size(px(22.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .cursor_pointer()
            .hover(|s| s.bg(p.hover))
            .child(svg().path(icons::CLOSE).size(px(14.)).text_color(p.muted))
            .on_click(cx.listener(|this, _, window, _cx| {
                (this.actions.on_close)();
                window.remove_window();
            }));

        let clear_btn = div()
            .id("clear_btn")
            .px(px(10.))
            .py(px(4.))
            .rounded(px(5.))
            .bg(p.danger)
            .text_size(px(11.5))
            .text_color(hsla(0., 0., 1., 1.))
            .cursor_pointer()
            .on_click(move |_, _, _| (on_clear)())
            .child("清空");

        let topbar = div()
            .flex()
            .flex_row()
            .items_center()
            .h(px(40.))
            .px(px(14.))
            .gap(px(8.))
            .border_b_1()
            .border_color(p.border)
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.title)
                    .child("翻译历史"),
            )
            .child(div().flex_1())
            .child(clear_btn)
            .child(close_btn);

        let q_display = self.query.borrow().clone();
        let search = div()
            .id("search")
            .h(px(30.))
            .mx(px(14.))
            .mt(px(8.))
            .rounded(px(6.))
            .bg(p.input_bg)
            .border_1()
            .border_color(p.border)
            .px(px(8.))
            .flex()
            .items_center()
            .text_size(px(13.))
            .text_color(if q_display.is_empty() { p.muted } else { p.text })
            .child(if q_display.is_empty() { "搜索原文 / 译文…".to_string() } else { q_display.clone() })
            .track_focus(&cx.focus_handle())
            .on_key_down(move |ev, _, _| {
                if ev.keystroke.key == "backspace" {
                    query.borrow_mut().pop();
                } else {
                    // 搜索暂不支持直接键盘输入，请使用搜索功能
                }
            });

        let q = self.query.borrow().to_ascii_lowercase();
        let mut list: Vec<AnyElement> = Vec::new();
        for (i, e) in self.entries.iter().rev().enumerate() {
            if !q.is_empty()
                && !e.source.to_ascii_lowercase().contains(&q)
                && !e.prepared.to_ascii_lowercase().contains(&q)
                && !e.results.iter().any(|r| r.text.to_ascii_lowercase().contains(&q))
            {
                continue;
            }
            let ts = e.ts.format("%Y-%m-%d %H:%M:%S").to_string();
            let primary = e
                .results
                .iter()
                .find(|r| r.success)
                .map(|r| r.text.clone())
                .unwrap_or_else(|| "(失败)".into());
            let i = i as u32;
            list.push(
                div()
                    .mx(px(14.))
                    .my(px(4.))
                    .p(px(10.))
                    .rounded(px(6.))
                    .bg(p.tab_inactive)
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.))
                            .child(div().text_size(px(10.5)).text_color(p.muted).child(ts))
                            .child(div().text_size(px(10.5)).text_color(p.muted).child(format!("{:?}", e.origin)))
                            .child(div().flex_1())
                            .child(div().text_size(px(10.5)).text_color(p.muted).child(format!("→ {}", e.target_lang))),
                    )
                    .child(selectable_plain(("hist_src", i), &e.source, p.text, 13., 19.))
                    .child(selectable_plain(("hist_tr", i), &primary, p.muted, 13., 19.))
                    .into_any_element(),
            );
        }
        if list.is_empty() {
            list.push(
                div()
                    .p(px(20.))
                    .text_size(px(12.))
                    .text_color(p.muted)
                    .child("还没有翻译记录")
                    .into_any_element(),
            );
        }

        let mut card = div()
            .id("history_card")
            .size_full()
            .flex()
            .flex_col()
            .bg(p.card)
            .border_1()
            .border_color(p.border)
            .rounded(px(crate::platform::CORNER_RADIUS as f32))
            .font_family(".SystemUIFont");
        #[cfg(not(target_os = "macos"))]
        {
            card = card.shadow(vec![BoxShadow {
                color: p.shadow,
                offset: point(px(0.), px(4.)),
                blur_radius: px(10.),
                spread_radius: px(0.),
                inset: false,
            }]);
        }
        let card = card
            .child(topbar)
            .child(search)
            .child(
                div()
                    .id("history_list")
                    .flex_1()
                    .min_h(px(0.))
                    .p(px(4.))
                    .overflow_y_scrollbar()
                    .children(list),
            );

        div()
            .id("history_root")
            .track_focus(&focus)
            .on_key_down(cx.listener(|_, ev: &KeyDownEvent, window, _cx| {
                if ev.keystroke.key == "escape" {
                    window.remove_window();
                }
            }))
            .size_full()
            .p(px(super::popup::SHADOW_MARGIN))
            .child(card)
    }
}
