//! 弹窗键盘命令与绑定。

use gpui_kit::*;

use crate::shared::actions::{
    CloseWindow, CopyResult, Favorite, POPUP_KEY_CONTEXT, RetryTranslate, Speak, SubmitTranslate,
    TogglePin,
};

pub fn init_keybindings(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", CloseWindow, Some(POPUP_KEY_CONTEXT)),
        KeyBinding::new("secondary-w", CloseWindow, Some(POPUP_KEY_CONTEXT)),
        KeyBinding::new("secondary-p", TogglePin, Some(POPUP_KEY_CONTEXT)),
        KeyBinding::new("secondary-c", CopyResult, Some(POPUP_KEY_CONTEXT)),
        KeyBinding::new("secondary-r", RetryTranslate, Some(POPUP_KEY_CONTEXT)),
        KeyBinding::new("secondary-s", Favorite, Some(POPUP_KEY_CONTEXT)),
        // Speak remains available for menu / future toolbar; no default chord conflict with Retry.
        KeyBinding::new("secondary-shift-r", Speak, Some(POPUP_KEY_CONTEXT)),
        KeyBinding::new("enter", SubmitTranslate, Some(POPUP_KEY_CONTEXT)),
    ]);
}
