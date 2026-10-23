//! 具体 OCR 引擎实现。

pub mod apple_vision;
pub mod baidu_cloud;
pub mod google_ocr;
pub mod rapidocr;
pub mod tencent_ocr;
pub mod volcano_ocr;
pub mod youdao_cloud;

pub use apple_vision::AppleVisionEngine;
pub use baidu_cloud::BaiduCloudOcr;
pub use google_ocr::GoogleOcr;
pub use rapidocr::{
    DownloadProgressState, RAPIDOCR_MODELS, RapidOcrEngine, RapidOcrModelInfo, check_model_status,
    download_model, get_download_progress, is_any_model_downloading, is_downloading_model,
    set_downloading_model,
};
pub use tencent_ocr::{TencentImageTranslate, TencentOcr};
pub use volcano_ocr::VolcanoOcr;
pub use youdao_cloud::YoudaoCloudOcr;
