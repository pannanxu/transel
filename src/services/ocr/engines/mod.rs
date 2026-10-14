//! 具体 OCR 引擎实现。

pub mod baidu_cloud;
pub mod google_ocr;
pub mod rapidocr;
pub mod tencent_ocr;
pub mod volcano_ocr;
pub mod youdao_cloud;

pub use baidu_cloud::BaiduCloudOcr;
pub use google_ocr::GoogleOcr;
pub use rapidocr::RapidOcrEngine;
pub use tencent_ocr::{TencentImageTranslate, TencentOcr};
pub use volcano_ocr::VolcanoOcr;
pub use youdao_cloud::YoudaoCloudOcr;
