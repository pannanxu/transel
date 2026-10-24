//! 多屏幕与光标定位适配。

use std::rc::Rc;
use gpui_kit::*;

#[derive(Clone, Copy, Debug)]
pub struct ScreenInfo {
    pub display_id: Option<DisplayId>,
    /// 全局坐标系下的原点与尺寸 (left, top, width, height)
    pub global_bounds: (f32, f32, f32, f32),
    /// 显示器逻辑尺寸 (width, height)
    pub size: (f32, f32),
}

/// 获取全局鼠标位置（逻辑点坐标）。
pub fn mouse_location() -> Option<(f32, f32)> {
    #[cfg(target_os = "macos")]
    {
        #[repr(C)]
        struct CGPoint {
            x: f64,
            y: f64,
        }
        #[link(name = "CoreGraphics", kind = "framework")]
        unsafe extern "C" {
            fn CGEventCreate(source: *const std::ffi::c_void) -> *mut std::ffi::c_void;
            fn CGEventGetLocation(event: *const std::ffi::c_void) -> CGPoint;
        }
        unsafe {
            let ev = CGEventCreate(std::ptr::null());
            if ev.is_null() {
                return None;
            }
            let pt = CGEventGetLocation(ev);
            core_foundation::base::CFRelease(ev as _);
            return Some((pt.x as f32, pt.y as f32));
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        use enigo::{Enigo, Mouse, Settings};
        let enigo = Enigo::new(&Settings::default()).ok()?;
        let (x, y) = enigo.location().ok()?;
        Some((x as f32, y as f32))
    }
}

/// 获取显示器的全局坐标范围 (left, top, width, height)。
pub fn display_global_bounds(display: &Rc<dyn PlatformDisplay>) -> (f32, f32, f32, f32) {
    #[cfg(target_os = "macos")]
    {
        #[repr(C)]
        struct CGPoint {
            x: f64,
            y: f64,
        }
        #[repr(C)]
        struct CGSize {
            width: f64,
            height: f64,
        }
        #[repr(C)]
        struct CGRect {
            origin: CGPoint,
            size: CGSize,
        }
        #[link(name = "CoreGraphics", kind = "framework")]
        unsafe extern "C" {
            fn CGDisplayBounds(display: u32) -> CGRect;
        }
        let raw_id = u64::from(display.id()) as u32;
        let rect = unsafe { CGDisplayBounds(raw_id) };
        (
            rect.origin.x as f32,
            rect.origin.y as f32,
            rect.size.width as f32,
            rect.size.height as f32,
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        let b = display.bounds();
        (
            f32::from(b.origin.x),
            f32::from(b.origin.y),
            f32::from(b.size.width),
            f32::from(b.size.height),
        )
    }
}

/// 获取指定显示器的屏幕信息。
pub fn display_info(display: &Rc<dyn PlatformDisplay>) -> ScreenInfo {
    let (left, top, sw, sh) = display_global_bounds(display);
    ScreenInfo {
        display_id: Some(display.id()),
        global_bounds: (left, top, sw, sh),
        size: (sw, sh),
    }
}

/// 根据给定的全局鼠标坐标（未提供时自动读取当前鼠标），找到对应的目标显示器。
pub fn find_display_for_mouse(
    cx: &App,
    mouse: Option<(f32, f32)>,
) -> (Option<Rc<dyn PlatformDisplay>>, ScreenInfo) {
    let mouse = mouse.or_else(mouse_location);
    let displays = cx.displays();
    let primary = cx.primary_display();

    let target_display = if let Some((mx, my)) = mouse {
        displays
            .iter()
            .find(|d| {
                let (left, top, w, h) = display_global_bounds(d);
                mx >= left && mx < left + w && my >= top && my < top + h
            })
            .cloned()
            .or(primary)
    } else {
        primary
    };

    let display_id = target_display.as_ref().map(|d| d.id());
    let (left, top, sw, sh) = if let Some(ref d) = target_display {
        display_global_bounds(d)
    } else {
        (0., 0., 1512., 982.)
    };

    (
        target_display,
        ScreenInfo {
            display_id,
            global_bounds: (left, top, sw, sh),
            size: (sw, sh),
        },
    )
}

/// 找到窗口所在的显示器（未找到时按鼠标所在显示器退回）。
pub fn find_display_for_window(
    window: &Window,
    cx: &App,
) -> (Option<Rc<dyn PlatformDisplay>>, ScreenInfo) {
    if let Some(d) = window.display(cx) {
        let info = display_info(&d);
        (Some(d), info)
    } else {
        find_display_for_mouse(cx, None)
    }
}

/// 纯函数：计算弹窗在目标屏幕内的窗口边界。
pub fn compute_popup_bounds(
    mouse: Option<(f32, f32)>,
    size: Size<Pixels>,
    screen_info: ScreenInfo,
    follow_mouse: bool,
) -> Bounds<Pixels> {
    let (w, h) = (f32::from(size.width), f32::from(size.height));
    let (left, top, sw, sh) = screen_info.global_bounds;
    let top_gap = (sh * 0.10).max(36.);
    let bottom_gap = (sh * 0.10).max(48.);

    #[cfg(target_os = "macos")]
    {
        // GPUI macOS 会自动加上 screen_frame.origin，因此 bounds.origin 需为屏幕局部坐标
        let (mut x, mut y) = if follow_mouse && let Some((mx, my)) = mouse {
            let local_mx = mx - left;
            let local_my = my - top;
            let mut y = local_my + 14.;
            if y + h.max(220.) > sh - bottom_gap {
                y = local_my - h.max(220.) - 8.;
            }
            (local_mx + 8., y)
        } else {
            ((sw - w) / 2., top_gap)
        };

        x = x.clamp(8., (sw - w - 8.).max(8.));
        y = y.clamp(top_gap, (sh - h - bottom_gap).max(top_gap));

        Bounds::new(point(px(x), px(y)), size)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let right = left + sw;
        let bottom = top + sh;
        let (mut x, mut y) = if follow_mouse && let Some((mx, my)) = mouse {
            let mut y = my + 14.;
            if y + h.max(220.) > bottom - bottom_gap {
                y = my - h.max(220.) - 8.;
            }
            (mx + 8., y)
        } else {
            (left + (sw - w) / 2., top + top_gap)
        };

        x = x.clamp(left + 8., (right - w - 8.).max(left + 8.));
        y = y.clamp(top + top_gap, (bottom - h - bottom_gap).max(top + top_gap));

        Bounds::new(point(px(x), px(y)), size)
    }
}

/// 纯函数：计算居中窗口在目标屏幕内的窗口边界。
pub fn compute_centered_bounds(
    size: Size<Pixels>,
    screen_info: ScreenInfo,
) -> Bounds<Pixels> {
    let (w, h) = (f32::from(size.width), f32::from(size.height));

    #[cfg(target_os = "macos")]
    {
        let (_left, _top, sw, sh) = screen_info.global_bounds;
        let x = ((sw - w) / 2.).max(8.);
        let y = ((sh - h) / 2.).max(24.);
        Bounds::new(point(px(x), px(y)), size)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let (left, top, sw, sh) = screen_info.global_bounds;
        let x = (left + (sw - w) / 2.).max(left + 8.);
        let y = (top + (sh - h) / 2.).max(top + 24.);
        Bounds::new(point(px(x), px(y)), size)
    }
}

/// 纯函数：计算 HUD 胶囊在目标屏幕内的窗口边界。
pub fn compute_hud_bounds(
    mouse: Option<(f32, f32)>,
    size: Size<Pixels>,
    screen_info: ScreenInfo,
) -> Bounds<Pixels> {
    let (w, h) = (f32::from(size.width), f32::from(size.height));
    let (left, top, sw, sh) = screen_info.global_bounds;

    #[cfg(target_os = "macos")]
    {
        let (mut x, mut y) = if let Some((mx, my)) = mouse {
            (mx - left + 12., my - top + 16.)
        } else {
            ((sw - w) / 2., (sh * 0.18).max(24.))
        };
        x = x.clamp(8., (sw - w - 8.).max(8.));
        y = y.clamp(8., (sh - h - 8.).max(8.));
        Bounds::new(point(px(x), px(y)), size)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let right = left + sw;
        let bottom = top + sh;
        let (mut x, mut y) = if let Some((mx, my)) = mouse {
            (mx + 12., my + 16.)
        } else {
            (left + (sw - w) / 2., top + (sh * 0.18).max(24.))
        };
        x = x.clamp(left + 8., (right - w - 8.).max(left + 8.));
        y = y.clamp(top + 8., (bottom - h - 8.).max(top + 8.));
        Bounds::new(point(px(x), px(y)), size)
    }
}

/// 弹窗位置计算：
/// - `follow_mouse == true`：紧贴鼠标右下方，防出界翻转
/// - `follow_mouse == false`：在鼠标所在显示器水平居中、偏上
pub fn popup_window_bounds(
    cx: &App,
    mouse: Option<(f32, f32)>,
    size: Size<Pixels>,
    follow_mouse: bool,
) -> (Bounds<Pixels>, Option<DisplayId>) {
    let (_display, screen_info) = find_display_for_mouse(cx, mouse);
    let bounds = compute_popup_bounds(mouse, size, screen_info, follow_mouse);
    (bounds, screen_info.display_id)
}

/// 在鼠标所在显示器居中窗口（带预留边距）。
pub fn centered_window_bounds(
    cx: &App,
    mouse: Option<(f32, f32)>,
    size: Size<Pixels>,
) -> (Bounds<Pixels>, Option<DisplayId>) {
    let (_display, screen_info) = find_display_for_mouse(cx, mouse);
    let bounds = compute_centered_bounds(size, screen_info);
    (bounds, screen_info.display_id)
}

/// HUD 窗口位置（静默模式提示胶囊）。
pub fn hud_window_bounds(
    cx: &App,
    mouse: Option<(f32, f32)>,
    size: Size<Pixels>,
) -> (Bounds<Pixels>, Option<DisplayId>) {
    let (_display, screen_info) = find_display_for_mouse(cx, mouse);
    let bounds = compute_hud_bounds(mouse, size, screen_info);
    (bounds, screen_info.display_id)
}

#[cfg(test)]
mod tests {
    use super::{ScreenInfo, compute_centered_bounds, compute_popup_bounds};
    use core::prelude::v1::test;
    use gpui_kit::{px, size};

    #[test]
    fn test_compute_centered_bounds() {
        let screen = ScreenInfo {
            display_id: None,
            global_bounds: (-1920., 0., 1920., 1080.),
            size: (1920., 1080.),
        };
        let bounds = compute_centered_bounds(size(px(600.), px(400.)), screen);
        #[cfg(target_os = "macos")]
        {
            // macOS 局部坐标
            assert_eq!(f32::from(bounds.origin.x), (1920. - 600.) / 2.);
            assert_eq!(f32::from(bounds.origin.y), (1080. - 400.) / 2.);
        }
        #[cfg(not(target_os = "macos"))]
        {
            // Windows/Linux 全局坐标
            assert_eq!(f32::from(bounds.origin.x), -1920. + (1920. - 600.) / 2.);
            assert_eq!(f32::from(bounds.origin.y), (1080. - 400.) / 2.);
        }
    }

    #[test]
    fn test_compute_popup_bounds_follow_mouse() {
        let screen = ScreenInfo {
            display_id: None,
            global_bounds: (-1920., 0., 1920., 1080.),
            size: (1920., 1080.),
        };
        // 鼠标在副屏中间 (-1000, 500)
        let bounds = compute_popup_bounds(
            Some((-1000., 500.)),
            size(px(400.), px(200.)),
            screen,
            true,
        );
        #[cfg(target_os = "macos")]
        {
            // 局部坐标: -1000 - (-1920) + 8 = 928
            assert_eq!(f32::from(bounds.origin.x), 928.);
            // 500 - 0 + 14 = 514
            assert_eq!(f32::from(bounds.origin.y), 514.);
        }
        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(f32::from(bounds.origin.x), -992.);
            assert_eq!(f32::from(bounds.origin.y), 514.);
        }
    }

    #[test]
    fn test_compute_popup_bounds_no_follow() {
        let screen = ScreenInfo {
            display_id: None,
            global_bounds: (-1920., 0., 1920., 1080.),
            size: (1920., 1080.),
        };
        // 鼠标在副屏，follow_mouse 为 false：在副屏居中上方
        let bounds = compute_popup_bounds(
            Some((-1000., 500.)),
            size(px(400.), px(200.)),
            screen,
            false,
        );
        #[cfg(target_os = "macos")]
        {
            assert_eq!(f32::from(bounds.origin.x), (1920. - 400.) / 2.);
            assert_eq!(f32::from(bounds.origin.y), 108.); // 1080 * 0.10
        }
        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(f32::from(bounds.origin.x), -1920. + (1920. - 400.) / 2.);
            assert_eq!(f32::from(bounds.origin.y), 108.);
        }
    }
}
