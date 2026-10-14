//! 语义间距与布局约定。
//!
//! Application layout 优先使用 GPUI rem helper（`p_2`、`gap_3`、`text_sm` 等）。
//! 下列常量仅用于：弹窗跟鼠标、截图像素换算、平台毛玻璃圆角等**物理边界**。
//! Feature 视图中出现 `px(...)` 时必须能指向此处的例外理由。

use gpui_kit::Pixels;

/// 弹窗卡片最小内容宽（逻辑像素；跟屏与字体估算用）。
pub const POPUP_MIN_W: f32 = 400.;
/// 弹窗卡片最大内容宽。
pub const POPUP_MAX_W: f32 = 720.;

/// 非 macOS 自绘阴影边距（物理像素）。
#[cfg(target_os = "macos")]
pub const SHADOW_MARGIN: f32 = 0.;
#[cfg(not(target_os = "macos"))]
pub const SHADOW_MARGIN: f32 = 12.;

/// 工具窗口默认位置偏移（无鼠标坐标时）。
pub const UTILITY_ORIGIN_X: f32 = 160.;
pub const UTILITY_ORIGIN_Y: f32 = 120.;

/// 列表类 utility 窗口默认尺寸。
pub const HISTORY_SIZE: (f32, f32) = (640., 520.);
pub const SETTINGS_SIZE: (f32, f32) = (980., 680.);
pub const FAVORITES_SIZE: (f32, f32) = (640., 560.);
pub const OCR_SIZE: (f32, f32) = (520., 420.);

/// Overlay 预览最大逻辑宽高（相对截图像素缩放）。
pub const OVERLAY_MAX_W: f32 = 900.;
pub const OVERLAY_MAX_H: f32 = 700.;

pub fn rem_px(v: f32) -> Pixels {
    gpui_kit::px(v)
}
