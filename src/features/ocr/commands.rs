use gpui_kit::*;

use crate::shared::actions::{CopyResult, OCR_KEY_CONTEXT, bind_window_close_keys};

pub fn init_keybindings(cx: &mut App) {
    bind_window_close_keys(cx, Some(OCR_KEY_CONTEXT));
    cx.bind_keys([KeyBinding::new(
        "secondary-c",
        CopyResult,
        Some(OCR_KEY_CONTEXT),
    )]);
}
