//! 应用层：全局状态、配置、钥匙串、历史、热键、路径。
//!
//! 这些模块不依赖 GPUI，可以在任意线程安全使用。

pub mod config;
pub mod errors;
pub mod favorites;
pub mod history;
pub mod hotkey;
pub mod openai_presets;
pub mod paths;
pub mod secrets;
pub mod state;
