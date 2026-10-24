//! 翻译历史窗口。

mod commands;
mod view;

pub use commands::init_keybindings;
pub use view::{HistoryActions, HistoryView};

use std::sync::Arc;

use gpui_kit::*;

use crate::app::state::AppState;
use crate::shared::tokens::HISTORY_SIZE;
use crate::shell::window::open_document_window;

pub fn open(cx: &mut App) {
    let old = cx.global_mut::<AppState>().history_window.lock().take();
    if let Some(h) = old {
        let _ = h.update(cx, |_, window, _| window.remove_window());
    }
    let history = cx.global::<AppState>().history.clone();
    let entries = history.list().unwrap_or_default();
    let hist_for_clear = history.clone();
    let (bounds, display_id) = crate::platform::centered_window_bounds(
        cx,
        None,
        size(px(HISTORY_SIZE.0), px(HISTORY_SIZE.1)),
    );
    let actions = HistoryActions {
        on_clear: Arc::new(move || {
            let _ = hist_for_clear.clear();
        }),
        ..HistoryActions::default()
    };
    match open_document_window(cx, "历史", bounds, display_id, move |window, cx| {
        cx.new(|cx| HistoryView::new(entries, actions, window, cx))
    }) {
        Ok(handle) => {
            *cx.global_mut::<AppState>().history_window.lock() = Some(handle);
        }
        Err(e) => log::error!("打开历史窗口失败: {e:#}"),
    }
}
