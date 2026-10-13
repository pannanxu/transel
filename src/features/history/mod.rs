//! 翻译历史窗口。

mod commands;
mod view;

pub use commands::init_keybindings;
pub use view::{HistoryActions, HistoryView};

use gpui_kit::*;

use crate::app::state::AppState;
use crate::shared::chrome::window_background;
use crate::shared::tokens::{HISTORY_SIZE, UTILITY_ORIGIN_X, UTILITY_ORIGIN_Y};
use crate::shell::window::{PopupWindowOpts, open_root_window};

pub fn open(cx: &mut App) {
    let old = cx.global_mut::<AppState>().history_window.lock().take();
    if let Some(h) = old {
        let _ = h.update(cx, |_, window, _| window.remove_window());
    }
    let state = cx.global::<AppState>();
    let entries = state.history.list().unwrap_or_default();
    let opts = PopupWindowOpts {
        bounds: Bounds::new(
            point(px(UTILITY_ORIGIN_X), px(UTILITY_ORIGIN_Y)),
            size(px(HISTORY_SIZE.0), px(HISTORY_SIZE.1)),
        ),
        display_id: None,
        focus: true,
        movable: true,
        resizable: true,
        background: window_background(),
    };
    match open_root_window(cx, opts, move |window, cx| {
        cx.new(|cx| HistoryView::new(entries, Default::default(), window, cx))
    }) {
        Ok(handle) => {
            *cx.global_mut::<AppState>().history_window.lock() = Some(handle);
        }
        Err(e) => log::error!("打开历史窗口失败: {e:#}"),
    }
}
