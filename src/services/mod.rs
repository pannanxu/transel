//! 业务服务：翻译、OCR、TTS、输入等。
//!
//! 翻译由 [`translate`] 模块负责（含词典等通道实现）；OCR/TTS/输入单独成模块。

pub mod cloud;
pub mod dictionary;
pub mod input;
pub mod ocr;
pub mod translate;
pub mod tts;
