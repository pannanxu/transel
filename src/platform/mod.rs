//! 平台特定：托盘、原生窗口、拖动。

#[cfg(target_os = "macos")]
pub mod macos;
pub mod native;
pub mod tray;

pub use native::*;
