//! 平台原生窗口处理：macOS 毛玻璃 + 圆角裁剪；窗口拖动。

use gpui_kit::Window;

#[cfg(target_os = "linux")]
pub use linux_impl::ManualDrag;
#[cfg(not(target_os = "linux"))]
pub struct ManualDrag;

/// 开始拖动窗口。
pub fn start_drag(window: &Window) -> Option<ManualDrag> {
    #[cfg(target_os = "windows")]
    {
        windows_impl::start_drag(window);
        None
    }
    #[cfg(target_os = "macos")]
    {
        window.start_window_move();
        None
    }
    #[cfg(target_os = "linux")]
    {
        let d = linux_impl::start(window);
        if d.is_none() {
            window.start_window_move();
        }
        d
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        window.start_window_move();
        None
    }
}

/// 手动拖动中：按当前指针位置移动窗口。
pub fn drag_move(drag: &ManualDrag) {
    #[cfg(target_os = "linux")]
    linux_impl::drag_move(drag);
    #[cfg(not(target_os = "linux"))]
    let _ = drag;
}

/// 弹窗打开后调用一次：macOS 毛玻璃圆角；Windows 置顶/不进任务栏；Linux 请求 ABOVE。
pub fn setup_popup_window(window: &Window, activate: bool) {
    setup_popup_window_ex(window, activate, true);
}

/// `vibrancy=false` 时不装毛玻璃，窗口不透明（设置面板用）。
pub fn setup_popup_window_ex(window: &Window, activate: bool, vibrancy: bool) {
    #[cfg(target_os = "macos")]
    {
        let _ = activate;
        if vibrancy {
            macos_impl::install_vibrancy(window);
        } else {
            macos_impl::setup_opaque(window);
        }
        macos_impl::apply_pin(window, false);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = vibrancy;
    #[cfg(target_os = "windows")]
    windows_impl::setup_popup(window, activate);
    #[cfg(target_os = "linux")]
    {
        let _ = activate;
        linux_impl::setup_popup(window);
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = (window, activate, vibrancy);
    }
}

/// 固定时保持浮动置顶、失焦不隐藏；取消固定后仍由视图决定是否关闭。
pub fn apply_popup_pin(window: &Window, _pinned: bool) {
    #[cfg(target_os = "macos")]
    macos_impl::apply_pin(window, _pinned);
    #[cfg(target_os = "windows")]
    windows_impl::setup_popup(window, true);
    #[cfg(target_os = "linux")]
    linux_impl::setup_popup(window);
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = (window, pinned);
    }
}

/// 窗口尺寸变化后调用：让 macOS 按新的圆角形状重算阴影。
pub fn refresh_shadow(window: &Window) {
    #[cfg(target_os = "macos")]
    macos_impl::invalidate_shadow(window);
    #[cfg(not(target_os = "macos"))]
    let _ = window;
}

pub const CORNER_RADIUS: f64 = 12.0;

#[cfg(target_os = "macos")]
mod macos_impl {
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{MainThreadMarker, MainThreadOnly, msg_send};
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSFloatingWindowLevel, NSView, NSVisualEffectBlendingMode,
        NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindow, NSWindowButton,
        NSWindowOrderingMode, NSWindowStyleMask,
    };
    use objc2_foundation::NSString;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    use super::CORNER_RADIUS;

    fn gpui_view(window: &gpui_kit::Window) -> Option<Retained<NSView>> {
        let handle = HasWindowHandle::window_handle(window).ok()?;
        let RawWindowHandle::AppKit(h) = handle.as_raw() else {
            return None;
        };
        unsafe { Retained::retain(h.ns_view.as_ptr().cast::<NSView>()) }
    }

    unsafe fn round_layer(view: &NSView) {
        view.setWantsLayer(true);
        let layer: *mut AnyObject = unsafe { msg_send![view, layer] };
        if let Some(layer) = unsafe { layer.as_ref() } {
            let curve = NSString::from_str("continuous");
            unsafe {
                let _: () = msg_send![layer, setCornerRadius: CORNER_RADIUS];
                let _: () = msg_send![layer, setMasksToBounds: true];
                let _: () = msg_send![layer, setCornerCurve: &*curve];
            }
        }
    }

    pub fn install_vibrancy(window: &gpui_kit::Window) {
        let Some(mtm) = MainThreadMarker::new() else {
            log::warn!("install_vibrancy 必须在主线程调用");
            return;
        };
        let Some(view) = gpui_view(window) else {
            return;
        };
        let Some(nswindow) = view.window() else {
            return;
        };
        unsafe {
            let Some(content) = nswindow.contentView() else {
                return;
            };
            let effect =
                NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), content.bounds());
            effect.setMaterial(NSVisualEffectMaterial::Popover);
            effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
            effect.setState(NSVisualEffectState::Active);
            effect.setAutoresizingMask(
                NSAutoresizingMaskOptions::ViewWidthSizable
                    | NSAutoresizingMaskOptions::ViewHeightSizable,
            );
            round_layer(&effect);
            content.addSubview_positioned_relativeTo(
                &effect,
                NSWindowOrderingMode::Below,
                Some(&view),
            );
            round_layer(&content);
            nswindow.setOpaque(false);
            nswindow.setHasShadow(true);
            // titlebar: None 时 GPUI 不会加 Closable，系统菜单 Cmd+W 会吞键却关不掉。
            nswindow.setStyleMask(nswindow.styleMask() | NSWindowStyleMask::Closable);
            for btn in [
                NSWindowButton::CloseButton,
                NSWindowButton::MiniaturizeButton,
                NSWindowButton::ZoomButton,
            ] {
                if let Some(b) = nswindow.standardWindowButton(btn) {
                    b.setHidden(true);
                }
            }
            nswindow.invalidateShadow();
        }
    }

    pub fn setup_opaque(window: &gpui_kit::Window) {
        let Some(view) = gpui_view(window) else {
            return;
        };
        let Some(nswindow) = view.window() else {
            return;
        };
        unsafe {
            if let Some(content) = nswindow.contentView() {
                round_layer(&content);
            }
            nswindow.setOpaque(true);
            nswindow.setHasShadow(true);
            nswindow.setStyleMask(nswindow.styleMask() | NSWindowStyleMask::Closable);
            for btn in [
                NSWindowButton::CloseButton,
                NSWindowButton::MiniaturizeButton,
                NSWindowButton::ZoomButton,
            ] {
                if let Some(b) = nswindow.standardWindowButton(btn) {
                    b.setHidden(true);
                }
            }
            nswindow.invalidateShadow();
        }
    }

    fn nswindow_of(window: &gpui_kit::Window) -> Option<Retained<NSWindow>> {
        gpui_view(window)?.window()
    }

    /// NSPanel 默认 hidesOnDeactivate=YES，点回浏览器会把翻译窗藏掉。
    /// 固定后关掉该行为并保持 Floating 层级。
    pub fn apply_pin(window: &gpui_kit::Window, pinned: bool) {
        let Some(nswindow) = nswindow_of(window) else {
            return;
        };
        nswindow.setHidesOnDeactivate(false);
        nswindow.setLevel(NSFloatingWindowLevel);
        if pinned {
            nswindow.orderFrontRegardless();
        }
    }

    pub fn invalidate_shadow(window: &gpui_kit::Window) {
        if let Ok(handle) = HasWindowHandle::window_handle(window)
            && let RawWindowHandle::AppKit(h) = handle.as_raw()
        {
            let view = unsafe { Retained::retain(h.ns_view.as_ptr().cast::<NSView>()) };
            if let Some(v) = view
                && let Some(w) = v.window()
            {
                w.invalidateShadow();
            }
        }
    }
}

#[cfg(target_os = "windows")]
mod windows_impl {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
    use windows::Win32::UI::WindowsAndMessaging::{HTCAPTION, PostMessageW, WM_NCLBUTTONDOWN};

    pub fn start_drag(window: &gpui_kit::Window) {
        let Some(hwnd) = hwnd_of(window) else {
            return;
        };
        unsafe {
            let _ = ReleaseCapture();
            let _ = PostMessageW(
                Some(hwnd),
                WM_NCLBUTTONDOWN,
                WPARAM(HTCAPTION as usize),
                LPARAM(0),
            );
        }
    }

    pub fn setup_popup(window: &gpui_kit::Window, activate: bool) {
        use windows::Win32::UI::WindowsAndMessaging::{
            GWL_EXSTYLE, GetWindowLongPtrW, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
            SetWindowLongPtrW, SetWindowPos, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
        };
        let Some(hwnd) = hwnd_of(window) else {
            return;
        };
        unsafe {
            let mut ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            ex |= WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0;
            if !activate {
                ex |= WS_EX_NOACTIVATE.0;
            }
            let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex as isize);
            let mut flags = SWP_NOMOVE | SWP_NOSIZE;
            if !activate {
                flags |= SWP_NOACTIVATE;
            }
            let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, flags);
        }
    }

    fn hwnd_of(window: &gpui_kit::Window) -> Option<HWND> {
        let handle = HasWindowHandle::window_handle(window).ok()?;
        let RawWindowHandle::Win32(h) = handle.as_raw() else {
            return None;
        };
        Some(HWND(h.hwnd.get() as *mut core::ffi::c_void))
    }
}

#[cfg(target_os = "linux")]
mod linux_impl {
    use std::cell::OnceCell;

    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{ConfigureWindowAux, ConnectionExt as _};
    use x11rb::rust_connection::RustConnection;

    pub struct ManualDrag {
        window: u32,
        pointer0: (i32, i32),
        origin0: (i32, i32),
    }

    thread_local! {
        static CONN: OnceCell<Option<(RustConnection, u32)>> = const { OnceCell::new() };
    }

    fn with_conn<R>(f: impl FnOnce(&RustConnection, u32) -> Option<R>) -> Option<R> {
        CONN.with(|c| {
            let c = c.get_or_init(|| {
                let (conn, screen) = x11rb::connect(None).ok()?;
                let root = conn.setup().roots[screen].root;
                Some((conn, root))
            });
            let (conn, root) = c.as_ref()?;
            f(conn, *root)
        })
    }

    fn pointer(conn: &RustConnection, root: u32) -> Option<(i32, i32)> {
        let r = conn.query_pointer(root).ok()?.reply().ok()?;
        Some((r.root_x as i32, r.root_y as i32))
    }

    pub fn start(window: &gpui_kit::Window) -> Option<ManualDrag> {
        let handle = HasWindowHandle::window_handle(window).ok()?;
        let xid = match handle.as_raw() {
            RawWindowHandle::Xlib(h) => h.window as u32,
            RawWindowHandle::Xcb(h) => h.window.get(),
            _ => return None,
        };
        with_conn(|conn, root| {
            let pos = conn
                .translate_coordinates(xid, root, 0, 0)
                .ok()?
                .reply()
                .ok()?;
            Some(ManualDrag {
                window: xid,
                pointer0: pointer(conn, root)?,
                origin0: (pos.dst_x as i32, pos.dst_y as i32),
            })
        })
    }

    pub fn drag_move(d: &ManualDrag) {
        with_conn(|conn, root| {
            let (px, py) = pointer(conn, root)?;
            let x = d.origin0.0 + px - d.pointer0.0;
            let y = d.origin0.1 + py - d.pointer0.1;
            conn.configure_window(d.window, &ConfigureWindowAux::new().x(x).y(y))
                .ok()?;
            conn.flush().ok()
        });
    }

    pub fn setup_popup(window: &gpui_kit::Window) {
        use x11rb::protocol::xproto::{ClientMessageEvent, EventMask};

        let handle = HasWindowHandle::window_handle(window).ok();
        let Some(handle) = handle else {
            return;
        };
        let xid = match handle.as_raw() {
            RawWindowHandle::Xlib(h) => h.window as u32,
            RawWindowHandle::Xcb(h) => h.window.get(),
            _ => return,
        };
        with_conn(|conn, root| {
            let intern = |name: &[u8]| {
                conn.intern_atom(false, name)
                    .ok()?
                    .reply()
                    .ok()
                    .map(|r| r.atom)
            };
            let net_state = intern(b"_NET_WM_STATE")?;
            let above = intern(b"_NET_WM_STATE_ABOVE")?;
            let skip = intern(b"_NET_WM_STATE_SKIP_TASKBAR")?;
            let event = ClientMessageEvent::new(32, xid, net_state, [1, above, skip, 0, 0]);
            conn.send_event(
                false,
                root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                event,
            )
            .ok()?;
            conn.flush().ok()
        });
    }
}
