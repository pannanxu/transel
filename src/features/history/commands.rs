use gpui_kit::*;

use crate::shared::actions::{ClearList, UTILITY_KEY_CONTEXT, bind_window_close_keys};

pub fn init_keybindings(cx: &mut App) {
    bind_window_close_keys(cx, Some(UTILITY_KEY_CONTEXT));
    cx.bind_keys([KeyBinding::new(
        "secondary-backspace",
        ClearList,
        Some(UTILITY_KEY_CONTEXT),
    )]);
}
