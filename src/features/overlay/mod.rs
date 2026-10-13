//! 原图翻译覆盖层。

mod commands;
mod flow;
mod view;

pub use commands::init_keybindings;
pub use flow::{
    close_window, ensure_loading, finish_window, open_window, set_status, trigger_overlay,
};
pub use view::{OverlayActions, OverlayBlock, OverlayView};
