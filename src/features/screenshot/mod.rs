pub mod crop;
pub mod state;
pub mod view;

use std::path::PathBuf;

use gpui_kit::*;
use once_cell::sync::Lazy;
use parking_lot::Mutex;

pub use self::state::ScreenshotMode;
pub use self::view::ScreenshotView;
use crate::services::ocr;

pub(crate) static SCREENSHOT_WINDOW: Lazy<Mutex<Option<AnyWindowHandle>>> =
    Lazy::new(|| Mutex::new(None));

pub fn trigger_screenshot(cx: &mut App) {
    close_window(cx);

    cx.spawn(async move |cx| {
        // 1. 使用系统原生选区截取指定区域
        let capture = cx
            .background_executor()
            .spawn(async { ocr::capture_interactive() })
            .await;

        let path = match capture {
            Ok(p) => p,
            Err(e) => {
                log::info!("截图已取消或未完成: {e:#}");
                return;
            }
        };

        let mouse = cx
            .background_executor()
            .spawn(async { crate::platform::mouse_location() })
            .await;

        cx.update(|cx| {
            open_screenshot_editor(cx, path, mouse);
        });
    })
    .detach();
}

pub fn trigger_ocr(cx: &mut App) {
    crate::features::ocr::trigger_ocr(cx);
}

pub fn trigger_ocr_only(cx: &mut App, silent: bool) {
    crate::features::ocr::trigger_ocr_only(cx, silent);
}

pub fn trigger_overlay(cx: &mut App) {
    crate::features::overlay::trigger_overlay(cx);
}

fn open_screenshot_editor(cx: &mut App, image_path: PathBuf, mouse: Option<(f32, f32)>) {
    let (img_w, img_h) = image::image_dimensions(&image_path).unwrap_or((800, 600));

    let (_display, screen_info) = crate::platform::find_display_for_mouse(cx, mouse);
    let (disp_w, disp_h) = screen_info.size;

    let est_scale = 2.0f32;
    let log_w = img_w as f32 / est_scale;
    let log_h = img_h as f32 / est_scale;

    let max_w = (disp_w - 60.0).max(800.0);
    let max_h = (disp_h - 80.0).max(520.0);

    // 预留四周充足的画布操作边距
    let ideal_w = log_w + 120.0;
    let ideal_h = log_h + 140.0;

    let win_w = ideal_w.clamp(800.0, max_w);
    let win_h = ideal_h.clamp(520.0, max_h);

    let (bounds, display_id) =
        crate::platform::centered_window_bounds(cx, mouse, size(px(win_w), px(win_h)));

    let options = WindowOptions {
        titlebar: None,
        kind: WindowKind::Floating,
        focus: true,
        show: true,
        is_movable: true,
        is_resizable: true,
        is_minimizable: false,
        display_id,
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_background: WindowBackgroundAppearance::Transparent,
        app_id: Some("dev.transel".into()),
        ..Default::default()
    };

    match cx.open_window(options, move |window, cx| {
        let scale = window.scale_factor();
        let actual_log_w = img_w as f32 / scale;
        let actual_log_h = img_h as f32 / scale;

        let view = cx.new(|cx| {
            ScreenshotView::new(image_path, (actual_log_w, actual_log_h), scale, window, cx)
        });
        let focus = view.read(cx).focus.clone();
        window.focus(&focus, cx);
        view
    }) {
        Ok(handle) => {
            let any_handle: AnyWindowHandle = handle.into();
            *SCREENSHOT_WINDOW.lock() = Some(any_handle);
            let _ = handle.update(cx, |view, window, _| {
                view.set_window_handle(any_handle);
                crate::platform::setup_popup_window_ex(window, true, true);
            });
            cx.activate(true);
        }
        Err(e) => {
            log::error!("打开纯 Rust 截图编辑窗口失败: {e:#}");
        }
    }
}

pub fn close_window(cx: &mut App) {
    if let Some(handle) = SCREENSHOT_WINDOW.lock().take() {
        let _ = handle.update(cx, |root_view, window, cx| {
            if let Ok(view) = root_view.downcast::<ScreenshotView>() {
                view.update(cx, |v, cx| {
                    v.close(window, cx);
                });
            } else {
                window.remove_window();
            }
        });
    }
}
