//! 收藏夹窗口。

mod commands;
mod view;

pub use commands::init_keybindings;
pub use view::{FavoritesActions, FavoritesView};

use std::sync::Arc;

use gpui_kit::*;

use crate::app::state::AppState;
use crate::shared::tokens::{FAVORITES_SIZE, UTILITY_ORIGIN_X, UTILITY_ORIGIN_Y};
use crate::shell::window::open_document_window;

pub fn open(cx: &mut App) {
    let old = cx.global_mut::<AppState>().favorites_window.lock().take();
    if let Some(h) = old {
        let _ = h.update(cx, |_, window, _| window.remove_window());
    }
    let favorites = cx.global::<AppState>().favorites.clone();
    let entries = favorites.list().unwrap_or_default();
    let fav_for_clear = favorites.clone();
    let fav_for_export = favorites.clone();
    let fav_for_remove = favorites.clone();

    let bounds = Bounds::new(
        point(px(UTILITY_ORIGIN_X), px(UTILITY_ORIGIN_Y)),
        size(px(FAVORITES_SIZE.0), px(FAVORITES_SIZE.1)),
    );
    let actions = FavoritesActions {
        on_close: Arc::new(|| {}),
        on_clear: Arc::new(move || {
            let _ = fav_for_clear.clear();
        }),
        on_export: Arc::new(move || {
            if let Ok(text) = fav_for_export.export_text()
                && let Ok(mut cb) = arboard::Clipboard::new()
            {
                let _ = cb.set_text(text);
            }
        }),
        on_remove: Arc::new(move |id| {
            let _ = fav_for_remove.remove(&id);
        }),
        on_copy: Arc::new(|t| {
            if let Ok(mut cb) = arboard::Clipboard::new() {
                let _ = cb.set_text(t);
            }
        }),
    };
    match open_document_window(cx, "收藏", bounds, move |window, cx| {
        cx.new(|cx| FavoritesView::new(entries.clone(), actions, window, cx))
    }) {
        Ok(handle) => {
            *cx.global_mut::<AppState>().favorites_window.lock() = Some(handle);
        }
        Err(e) => log::error!("打开收藏夹失败: {e:#}"),
    }
}
