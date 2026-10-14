//! 静默操作 HUD：鼠标旁的加载小按钮，点击可停止。

mod view;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui_kit::*;
use once_cell::sync::Lazy;
use parking_lot::Mutex;

use crate::app::state::AppState;
use crate::shared::chrome::window_background;
use crate::shell::window::{PopupWindowOpts, open_root_window};

use view::SilentHudView;

const HUD_W: f32 = 48.;
const HUD_H: f32 = 26.;
const MOUSE_OFFSET_X: f32 = 12.;
const MOUSE_OFFSET_Y: f32 = 12.;

static CANCEL: Lazy<Mutex<Option<Arc<AtomicBool>>>> = Lazy::new(|| Mutex::new(None));

fn mouse_location() -> Option<(f32, f32)> {
    use enigo::{Enigo, Mouse, Settings};
    let enigo = Enigo::new(&Settings::default()).ok()?;
    let (x, y) = enigo.location().ok()?;
    Some((x as f32, y as f32))
}

fn hud_bounds(cx: &App, mouse: Option<(f32, f32)>) -> (Bounds<Pixels>, Option<DisplayId>) {
    let size = size(px(HUD_W), px(HUD_H));
    let (w, h) = (f32::from(size.width), f32::from(size.height));
    let displays = cx.displays();
    let primary = cx.primary_display();

    let display = if let Some((mx, my)) = mouse {
        let p = point(px(mx), px(my));
        displays
            .iter()
            .find(|d| d.bounds().contains(&p))
            .cloned()
            .or(primary)
    } else {
        primary
    };

    let id = display.as_ref().map(|d| d.id());
    let screen = display.as_ref().map(|d| d.bounds()).unwrap_or_default();
    let (left, top) = (f32::from(screen.origin.x), f32::from(screen.origin.y));
    let (sw, sh) = (f32::from(screen.size.width), f32::from(screen.size.height));
    let (right, bottom) = (left + sw, top + sh);

    let (mut x, mut y) = if let Some((mx, my)) = mouse {
        (mx + MOUSE_OFFSET_X, my + MOUSE_OFFSET_Y)
    } else {
        (left + (sw - w) / 2., top + (sh * 0.18).max(24.))
    };
    if x + w > right {
        x = right - w - 8.;
    }
    if y + h > bottom {
        y = bottom - h - 8.;
    }
    if x < left {
        x = left + 8.;
    }
    if y < top {
        y = top + 8.;
    }

    (Bounds::new(point(px(x), px(y)), size), id)
}

/// 关闭 HUD（不置取消标志）。
pub fn finish(cx: &mut App) {
    *CANCEL.lock() = None;
    let handle = cx.global_mut::<AppState>().silent_hud.lock().take();
    if let Some(h) = handle {
        let _ = h.update(cx, |_, window, _| window.remove_window());
    }
}

fn mark_cancelled(cx: &mut App) {
    if let Some(flag) = CANCEL.lock().as_ref() {
        flag.store(true, Ordering::SeqCst);
    }
    cx.global::<AppState>().translate.cancel_active();
}

/// 用户停止：置取消、取消翻译批次、关闭 HUD。
///
/// 从 HUD 点击回调里不要同步 `handle.update` 关同一扇窗（会重入失败，
/// 句柄被 take 走后窗口就再也关不掉）。
pub fn request_stop(cx: &mut App) {
    mark_cancelled(cx);
    cx.defer(|cx| {
        finish(cx);
        log::info!("静默操作已停止");
    });
}

/// 在 HUD 窗口事件里关闭：直接 `remove_window`，避免重入 `handle.update`。
pub fn stop_from_window(window: &mut Window, cx: &mut App) {
    mark_cancelled(cx);
    *CANCEL.lock() = None;
    let _ = cx.global_mut::<AppState>().silent_hud.lock().take();
    window.remove_window();
    log::info!("静默操作已停止");
}

/// 在鼠标旁展示加载按钮；返回取消标志供调用方轮询。
///
/// `mouse` 为空时会尝试现场读取光标位置。不抢焦点，避免打断输入框。
/// 若已有进行中的静默操作，会先取消上一轮。
pub fn begin(
    cx: &mut App,
    label: impl Into<SharedString>,
    mouse: Option<(f32, f32)>,
) -> Arc<AtomicBool> {
    if let Some(prev) = CANCEL.lock().as_ref() {
        prev.store(true, Ordering::SeqCst);
    }
    cx.global::<AppState>().translate.cancel_active();
    finish(cx);

    let cancel = Arc::new(AtomicBool::new(false));
    *CANCEL.lock() = Some(cancel.clone());

    let mouse = mouse.or_else(mouse_location);
    let (bounds, display_id) = hud_bounds(cx, mouse);
    let label = label.into();
    let cancel_for_view = cancel.clone();

    let opts = PopupWindowOpts {
        bounds,
        display_id,
        focus: false,
        movable: false,
        resizable: false,
        background: window_background(),
    };

    match open_root_window(cx, opts, move |_, cx| {
        cx.new(|_cx| SilentHudView::new(label, cancel_for_view))
    }) {
        Ok(handle) => {
            *cx.global_mut::<AppState>().silent_hud.lock() = Some(handle);
        }
        Err(e) => log::warn!("静默 HUD 打开失败: {e:#}"),
    }

    cancel
}

/// 是否已请求停止。
pub fn is_cancelled(flag: &AtomicBool) -> bool {
    flag.load(Ordering::SeqCst)
}
