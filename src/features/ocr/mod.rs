//! 独立 OCR 窗口与截图识别流程。

mod commands;
mod flow;
mod view;

pub use commands::init_keybindings;
pub use flow::{
    close_window, ensure_loading, finish_window, open_window, set_error, trigger_finder,
    trigger_ocr, trigger_ocr_only, trigger_screenshot,
};
pub use view::{OcrActions, OcrView};
