//! 跨窗口共享的键盘命令（各 Feature 也可定义局部 Action）。

use gpui_kit::{App, KeyBinding, Menu, MenuItem, actions};

actions!(
    transel,
    [
        /// 关闭当前浮层窗口
        CloseWindow,
        /// 退出应用
        QuitApp,
        /// 切换弹窗固定
        TogglePin,
        /// 复制当前译文 / 选中文本
        CopyResult,
        /// 朗读当前结果
        Speak,
        /// 收藏当前结果
        Favorite,
        /// 提交输入翻译
        SubmitTranslate,
        /// 重新翻译
        RetryTranslate,
        /// 清空列表（历史 / 收藏）
        ClearList,
    ]
);

pub const POPUP_KEY_CONTEXT: &str = "TranslatePopup";
pub const UTILITY_KEY_CONTEXT: &str = "UtilityWindow";
pub const OCR_KEY_CONTEXT: &str = "OcrWindow";
pub const OVERLAY_KEY_CONTEXT: &str = "OverlayWindow";

/// Esc + Cmd/Ctrl+W 关闭当前窗口。无标题 PopUp 吃不到系统菜单的 Close，必须自己绑。
pub fn bind_window_close_keys(cx: &mut App, context: Option<&'static str>) {
    cx.bind_keys([
        KeyBinding::new("escape", CloseWindow, context),
        KeyBinding::new("secondary-w", CloseWindow, context),
    ]);
}

/// 关掉当前活动窗口。PopUp/Floating 都是 NSPanel，`App::active_window()` 只认 NSWindow 子类，会得到 None。
pub fn close_front_window(cx: &mut App) {
    let handles = cx.windows();
    let active = handles.iter().copied().find(|handle| {
        handle
            .update(cx, |_, window, _| window.is_window_active())
            .ok()
            .unwrap_or(false)
    });
    let handle = active.or_else(|| handles.into_iter().next());
    if let Some(handle) = handle {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
}

/// 用应用菜单接管 Cmd+W / Cmd+Q，避免系统「关闭窗口」走 `performClose:` 却关不掉无标题栏面板。
pub fn install_app_menus(cx: &mut App) {
    cx.set_menus(vec![Menu::new("Transel").items([
        MenuItem::action("关闭窗口", CloseWindow),
        MenuItem::separator(),
        MenuItem::action("退出 Transel", QuitApp),
    ])]);
}
