use gpui_kit::*;

use crate::shared::actions::{UTILITY_KEY_CONTEXT, bind_window_close_keys};

pub fn init_keybindings(cx: &mut App) {
    bind_window_close_keys(cx, Some(UTILITY_KEY_CONTEXT));
}
