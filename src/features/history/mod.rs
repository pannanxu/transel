//! 翻译历史窗口。

mod commands;
mod view;

pub use commands::init_keybindings;
pub use view::{HistoryActions, HistoryView};

use std::sync::Arc;

use gpui_kit::*;

use crate::app::state::AppState;
use crate::shared::tokens::{HISTORY_SIZE, UTILITY_ORIGIN_X, UTILITY_ORIGIN_Y};
use crate::shell::window::open_document_window;

pub fn open(cx: &mut App) {
    let old = cx.global_mut::<AppState>().history_window.lock().take();
    if let Some(h) = old {
        let _ = h.update(cx, |_, window, _| window.remove_window());
    }
    let history = cx.global::<AppState>().history.clone();
    let entries = history.list().unwrap_or_default();
    let hist_for_clear = history.clone();
    let bounds = Bounds::new(
        point(px(UTILITY_ORIGIN_X), px(UTILITY_ORIGIN_Y)),
        size(px(HISTORY_SIZE.0), px(HISTORY_SIZE.1)),
    );
    let actions = HistoryActions {
        on_clear: Arc::new(move || {
            let _ = hist_for_clear.clear();
        }),
        ..HistoryActions::default()
    };
    match open_document_window(cx, "历史", bounds, move |window, cx| {
        cx.new(|cx| HistoryView::new(entries, actions, window, cx))
    }) {
        Ok(handle) => {
            *cx.global_mut::<AppState>().history_window.lock() = Some(handle);
        }
        Err(e) => log::error!("打开历史窗口失败: {e:#}"),
    }
}
