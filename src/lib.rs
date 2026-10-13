//! Transel — 跨平台划词翻译（GPUI + gpui-kit）。
//!
//! 模块划分（按 GPUI Kit 编码指南：feature 组织业务能力，shell 只组合）：
//! - [`shell`]     App Shell：初始化、事件循环、统一开窗
//! - [`features`]  弹窗 / 设置 / 历史 / 收藏 / OCR / 原图翻译
//! - [`shared`]    主题、图标、卡片壳、共享 Action
//! - [`app`]       配置、钥匙串、历史、热键、路径（非 GPUI）
//! - [`services`]  翻译 / OCR / TTS / 词典
//! - [`platform`]  托盘、原生窗口
//! - [`split`]     标识符拆词

pub mod app;
pub mod features;
pub mod platform;
pub mod services;
pub mod shared;
pub mod shell;
pub mod split;

pub use app::config::Config;
pub use app::state::AppState;
