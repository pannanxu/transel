//! 统一错误类型，便于在 UI 层用一致方式呈现。

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AppError {
    Config(String),
    Secret(String),
    Network(String),
    Api { backend: String, message: String },
    Ocr(String),
    Tts(String),
    History(String),
    Selection(String),
    Permission(String),
    Other(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Config(s) => write!(f, "配置错误: {s}"),
            AppError::Secret(s) => write!(f, "凭据错误: {s}"),
            AppError::Network(s) => write!(f, "网络错误: {s}"),
            AppError::Api { backend, message } => write!(f, "{backend}: {message}"),
            AppError::Ocr(s) => write!(f, "OCR 错误: {s}"),
            AppError::Tts(s) => write!(f, "朗读错误: {s}"),
            AppError::History(s) => write!(f, "历史错误: {s}"),
            AppError::Selection(s) => write!(f, "选区错误: {s}"),
            AppError::Permission(s) => write!(f, "权限错误: {s}"),
            AppError::Other(s) => s.fmt(f),
        }
    }
}

impl std::error::Error for AppError {}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Other(format!("{e:#}"))
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Other(format!("io: {e}"))
    }
}

pub type AppResult<T> = std::result::Result<T, AppError>;
