//! 具体 OCR 引擎实现。
//!
//! - [`rapidocr`]：本地默认引擎
//! - [`baidu_cloud`] / [`youdao_cloud`]：云端占位，后续实现

pub mod baidu_cloud;
pub mod rapidocr;
pub mod youdao_cloud;

pub use baidu_cloud::BaiduCloudOcr;
pub use rapidocr::RapidOcrEngine;
pub use youdao_cloud::YoudaoCloudOcr;
