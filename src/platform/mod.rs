//! 平台特定：托盘、原生窗口、拖动、开机自启、多屏幕适配。

pub mod autostart;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod native;
pub mod screen;
pub mod tray;

pub use native::*;
pub use screen::*;
