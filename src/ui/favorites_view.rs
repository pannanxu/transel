//! 收藏夹窗口：列表 / 复制 / 导出 / 删除。

use std::sync::Arc;

use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::*;

use crate::app::favorites::FavoriteEntry;
use crate::ui::text_select::selectable_plain;
use crate::ui::theme::palette;

use super::icons;

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
    pub status: Option<String>,
    pub actions: FavoritesActions,
}

impl FavoritesView {
    pub fn new(entries: Vec<FavoriteEntry>, actions: FavoritesActions) -> Self {
        Self {
            entries,
            status: None,
            actions,
        }
    }
}

impl Render for FavoritesView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(window.appearance());

        let close_btn = div()
            .id("fav_close")
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

        let export_btn = div()
            .id("fav_export")
            .px(px(10.))
            .py(px(4.))
            .rounded(px(5.))
            .bg(p.tab_inactive)
            .text_size(px(11.5))
            .text_color(p.text)
            .cursor_pointer()
            .child("导出")
            .on_click(cx.listener(|this, _, _, cx| {
                (this.actions.on_export)();
                this.status = Some("已复制导出文本".into());
                cx.notify();
            }));

        let clear_btn = div()
            .id("fav_clear")
            .px(px(10.))
            .py(px(4.))
            .rounded(px(5.))
            .bg(p.danger)
            .text_size(px(11.5))
            .text_color(hsla(0., 0., 1., 1.))
            .cursor_pointer()
            .child("清空")
            .on_click(cx.listener(|this, _, _, cx| {
                (this.actions.on_clear)();
                this.entries.clear();
                this.status = Some("已清空".into());
                cx.notify();
            }));

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
                    .child("收藏夹"),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .child(format!("{} 条", self.entries.len())),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .child(self.status.clone().unwrap_or_default()),
            )
            .child(div().flex_1())
            .child(export_btn)
            .child(clear_btn)
            .child(close_btn);

        let mut rows: Vec<AnyElement> = Vec::new();
        for (i, e) in self.entries.iter().rev().enumerate() {
            let id = e.id.clone();
            let copy_text = if e.translation.is_empty() {
                e.source.clone()
            } else {
                format!("{}\n{}", e.source, e.translation)
            };
            let src = e.source.clone();
            let tr = e.translation.clone();
            rows.push(
                div()
                    .id(("fav_row", i))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .p(px(10.))
                    .rounded(px(6.))
                    .bg(p.input_bg)
                    .border_1()
                    .border_color(p.border)
                    .child(selectable_plain(("fav_src", i as u32), &src, p.muted, 12., 18.))
                    .child(selectable_plain(
                        ("fav_tr", i as u32),
                        if tr.is_empty() { "（无译文）" } else { &tr },
                        if tr.is_empty() { p.muted } else { p.text },
                        13.5,
                        20.,
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(px(8.))
                            .child(
                                div()
                                    .id(("fav_copy", i))
                                    .px(px(8.))
                                    .py(px(2.))
                                    .rounded(px(4.))
                                    .bg(p.tab_inactive)
                                    .text_size(px(11.))
                                    .text_color(p.text)
                                    .cursor_pointer()
                                    .child("复制")
                                    .on_click({
                                        let copy_text = copy_text.clone();
                                        cx.listener(move |this, _, _, _cx| {
                                            (this.actions.on_copy)(copy_text.clone());
                                        })
                                    }),
                            )
                            .child(
                                div()
                                    .id(("fav_del", i))
                                    .px(px(8.))
                                    .py(px(2.))
                                    .rounded(px(4.))
                                    .bg(p.danger)
                                    .text_size(px(11.))
                                    .text_color(hsla(0., 0., 1., 1.))
                                    .cursor_pointer()
                                    .child("删除")
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

        let body = if rows.is_empty() {
            div()
                .flex_1()
                .p(px(20.))
                .text_size(px(13.))
                .text_color(p.muted)
                .child("暂无收藏。在翻译弹窗按 Cmd+S 可收藏当前结果。")
                .into_any_element()
        } else {
            div()
                .id("fav_list")
                .flex_1()
                .min_h(px(0.))
                .p(px(12.))
                .gap(px(8.))
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
            .bg(p.card)
            .border_1()
            .border_color(p.border)
            .rounded(px(crate::platform::CORNER_RADIUS as f32))
            .font_family(".SystemUIFont")
            .child(topbar)
            .child(body);

        div()
            .id("fav_root")
            .size_full()
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, _cx| {
                if ev.keystroke.key == "escape" {
                    (this.actions.on_close)();
                    window.remove_window();
                }
            }))
            .child(card)
    }
}
