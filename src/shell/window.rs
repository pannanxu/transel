//! 统一 PopUp + Root 窗口入口。

#[cfg(target_os = "linux")]
use gpui_kit::base::input::Copy;
#[cfg(target_os = "linux")]
use gpui_kit::base::{TextSelection, TextSelectionLayer};
#[cfg(target_os = "linux")]
use gpui_kit::component::ActiveTheme as _;
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

fn popup_window_options(opts: &PopupWindowOpts, client_decorations: bool) -> WindowOptions {
    WindowOptions {
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
        // 翻译弹窗使用裸 Root 自绘卡片，Linux 下不需要桌面再套一层窗口装饰；
        // 否则 KDE 会在卡片外画出标题栏和红绿灯。其它弹窗保持原行为。
        window_decorations: if cfg!(target_os = "linux") && client_decorations {
            Some(WindowDecorations::Client)
        } else {
            None
        },
        ..Default::default()
    }
}

/// 打开以 `Root` 包裹的 PopUp 窗口，并做平台毛玻璃 / 置顶处理。
///
/// `build` 返回内容 View Entity；本函数负责 `Root` 与透明底。
pub fn open_root_window<V: 'static + Render>(
    cx: &mut App,
    opts: PopupWindowOpts,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V> + 'static,
) -> Result<AnyWindowHandle, anyhow::Error> {
    let options = popup_window_options(&opts, false);
    let opaque = matches!(opts.background, WindowBackgroundAppearance::Opaque);
    let handle = cx.open_window(options, move |window, cx| {
        let view = build(window, cx);
        let bg = if opaque {
            crate::shared::theme::opaque_panel(window.appearance())
        } else {
            transparent_black()
        };
        cx.new(|cx| gpui_kit::base::Root::new(view, window, cx).bg(bg))
    })?;
    let _ = handle.update(cx, |_, window, _| {
        platform::setup_popup_window_ex(window, opts.focus, !opaque)
    });
    if opts.focus {
        cx.activate(true);
    }
    Ok(handle.into())
}

/// 打开不带组件库 `WindowState` 装饰层的 PopUp 窗口。
///
/// gpui-component 的 `WindowState` 会在客户端装饰窗口外再包一层
/// `window_border()`：Linux 下固定 20px 阴影边距 + 1px 方框。翻译弹窗自己
/// 画圆角卡片和柔和阴影，这层方框就会露在卡片外，所以这里只挂载文本选择层
/// 和复制动作，不注册组件插件。
#[cfg(target_os = "linux")]
pub fn open_bare_window<V: 'static + Render>(
    cx: &mut App,
    opts: PopupWindowOpts,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V> + 'static,
) -> Result<AnyWindowHandle, anyhow::Error> {
    let options = popup_window_options(&opts, true);
    let opaque = matches!(opts.background, WindowBackgroundAppearance::Opaque);
    let handle = cx.open_window(options, move |window, cx| {
        let view = build(window, cx);
        let bg = if opaque {
            crate::shared::theme::opaque_panel(window.appearance())
        } else {
            transparent_black()
        };
        cx.new(|_| BareRoot { view, bg })
    })?;
    let _ = handle.update(cx, |_, window, _| {
        platform::setup_popup_window_ex(window, opts.focus, !opaque)
    });
    if opts.focus {
        cx.activate(true);
    }
    Ok(handle.into())
}

#[cfg(target_os = "linux")]
struct BareRoot<V: Render> {
    view: Entity<V>,
    bg: Hsla,
}

#[cfg(target_os = "linux")]
impl<V: Render> Render for BareRoot<V> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        window.set_rem_size(theme.font_size);
        div()
            .id("bare-root")
            // 独立上下文，避免继承组件 Root 的 Tab 绑定；Copy 在 bootstrap 注册。
            .key_context(POPUP_ROOT_CONTEXT)
            .on_action(cx.listener(Self::on_copy))
            .relative()
            .size_full()
            .bg(self.bg)
            .font_family(theme.font_family.clone())
            .text_color(theme.foreground)
            .child(TextSelectionLayer)
            .child(self.view.clone())
    }
}

#[cfg(target_os = "linux")]
impl<V: Render> BareRoot<V> {
    fn on_copy(&mut self, _: &Copy, window: &mut Window, cx: &mut Context<Self>) {
        let text = TextSelection::selected_text(window, cx).trim().to_string();
        if text.is_empty() {
            cx.propagate();
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }
}

/// 裸 Root 的按键上下文，仅用于译文拖选后的复制。
#[cfg(target_os = "linux")]
pub const POPUP_ROOT_CONTEXT: &str = "TranselPopupRoot";

/// 普通应用窗口：系统标题栏（macOS 红黄绿灯）、可最小化/缩放。
pub fn open_document_window<V: 'static + Render>(
    cx: &mut App,
    title: impl Into<SharedString>,
    bounds: Bounds<Pixels>,
    display_id: Option<DisplayId>,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V> + 'static,
) -> Result<AnyWindowHandle, anyhow::Error> {
    let options = WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
            appears_transparent: false,
            traffic_light_position: None,
        }),
        kind: WindowKind::Normal,
        focus: true,
        show: true,
        is_movable: true,
        is_resizable: true,
        is_minimizable: true,
        display_id,
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_background: WindowBackgroundAppearance::Opaque,
        window_min_size: Some(size(px(640.), px(480.))),
        app_id: Some("dev.transel".into()),
        ..Default::default()
    };
    let handle = cx.open_window(options, move |window, cx| {
        let view = build(window, cx);
        let bg = crate::shared::theme::opaque_panel(window.appearance());
        cx.new(|cx| gpui_kit::base::Root::new(view, window, cx).bg(bg))
    })?;
    let _ = handle.update(cx, |_, window, _| {
        platform::setup_document_window(window);
    });
    cx.activate(true);
    Ok(handle.into())
}
