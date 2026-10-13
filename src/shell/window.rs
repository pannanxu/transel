//! 统一 PopUp + Root 窗口入口。

use gpui_kit::*;

use crate::platform;

pub struct PopupWindowOpts {
    pub bounds: Bounds<Pixels>,
    pub display_id: Option<DisplayId>,
    pub focus: bool,
    pub movable: bool,
    pub resizable: bool,
    pub background: WindowBackgroundAppearance,
}

/// 打开以 `Root` 包裹的 PopUp 窗口，并做平台毛玻璃 / 置顶处理。
///
/// `build` 返回内容 View Entity；本函数负责 `Root` 与透明底。
pub fn open_root_window<V: 'static + Render>(
    cx: &mut App,
    opts: PopupWindowOpts,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V> + 'static,
) -> Result<AnyWindowHandle, anyhow::Error> {
    let options = WindowOptions {
        titlebar: None,
        // PopUp = 非激活 NSPanel，系统菜单栏仍属于前一个 App，Cmd+W 关的是别人。
        // 需要键盘的窗口改用 Floating，成为 key window 并激活本应用。
        kind: if opts.focus {
            WindowKind::Floating
        } else {
            WindowKind::PopUp
        },
        focus: opts.focus,
        show: true,
        is_movable: opts.movable,
        is_resizable: opts.resizable,
        is_minimizable: false,
        display_id: opts.display_id,
        window_bounds: Some(WindowBounds::Windowed(opts.bounds)),
        window_background: opts.background,
        app_id: Some("dev.transel".into()),
        ..Default::default()
    };
    let handle = cx.open_window(options, move |window, cx| {
        let view = build(window, cx);
        cx.new(|cx| gpui_kit::base::Root::new(view, window, cx).bg(transparent_black()))
    })?;
    let _ = handle.update(cx, |_, window, _| {
        platform::setup_popup_window(window, opts.focus)
    });
    if opts.focus {
        cx.activate(true);
    }
    Ok(handle.into())
}
