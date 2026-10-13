//! 设置面板。

mod commands;
mod view;

pub use commands::init_keybindings;
pub use view::{SettingsActions, SettingsDraft, SettingsView};

use std::sync::Arc;

use gpui_kit::*;

use crate::app::state::AppState;
use crate::shared::chrome::window_background;
use crate::shared::tokens::{SETTINGS_SIZE, UTILITY_ORIGIN_X, UTILITY_ORIGIN_Y};
use crate::shell::window::{PopupWindowOpts, open_root_window};

pub fn open(cx: &mut App) {
    let old = cx.global_mut::<AppState>().settings_window.lock().take();
    if let Some(h) = old {
        let _ = h.update(cx, |_, window, _| window.remove_window());
    }
    let cfg = cx.global::<AppState>().config().clone();
    let history = cx.global::<AppState>().history.clone();
    let secret_store = cx.global::<AppState>()._secret_store.clone();
    let actions = SettingsActions {
        on_clear_history: Arc::new(move || {
            if let Err(e) = history.clear() {
                log::warn!("清空历史失败: {e:#}");
            } else {
                log::info!("历史已清空");
            }
        }),
        on_secret_input: Arc::new(move |backend_id| {
            let _ = &secret_store;
            log::info!("后端 {backend_id} 密钥已更新");
        }),
        ..SettingsActions::default()
    };
    let opts = PopupWindowOpts {
        bounds: Bounds::new(
            point(px(UTILITY_ORIGIN_X), px(UTILITY_ORIGIN_Y)),
            size(px(SETTINGS_SIZE.0), px(SETTINGS_SIZE.1)),
        ),
        display_id: None,
        focus: true,
        movable: true,
        resizable: true,
        background: window_background(),
    };
    match open_root_window(cx, opts, move |window, cx| {
        cx.new(|cx| SettingsView::new(cfg, actions, window, cx))
    }) {
        Ok(handle) => {
            *cx.global_mut::<AppState>().settings_window.lock() = Some(handle);
        }
        Err(e) => log::error!("打开设置失败: {e:#}"),
    }
}
