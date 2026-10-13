//! 托盘图标 + 菜单。
//!
//! - macOS / Windows：GPUI 主线程创建（生命周期等同进程），通过 `MenuEvent::receiver()` 拿事件。
//! - Linux：在独立线程里跑 GTK 主循环，事件通过 `tray_events` 送回主循环。

use crate::app::state::TrayEvent;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

const INPUT_ID: &str = "input";
const OCR_ID: &str = "ocr";
const OCR_ONLY_ID: &str = "ocr_only";
const OCR_SILENT_ID: &str = "ocr_silent";
const OCR_FINDER_ID: &str = "ocr_finder";
const OVERLAY_ID: &str = "overlay";
const SILENT_REPLACE_ID: &str = "silent_replace";
const SILENT_INPUT_ID: &str = "silent_input";
const PIN_ID: &str = "pin";
const HISTORY_ID: &str = "history";
const FAVORITES_ID: &str = "favorites";
const SETTINGS_ID: &str = "settings";
const QUIT_ID: &str = "quit";

fn icon() -> Icon {
    let n = 32u32;
    let mut rgba = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = (x as f32 - 15.5, y as f32 - 15.5);
            let inside = dx * dx + dy * dy <= 15.0 * 15.0;
            rgba.extend_from_slice(if inside {
                &[0x3b, 0x82, 0xf6, 0xff]
            } else {
                &[0, 0, 0, 0]
            });
        }
    }
    Icon::from_rgba(rgba, n, n).expect("icon")
}

fn build(queue: std::sync::mpsc::Sender<TrayEvent>) -> anyhow::Result<TrayIcon> {
    let menu = Menu::new();
    let input = MenuItem::with_id(MenuId::new(INPUT_ID), "输入翻译…", true, None);
    let ocr = MenuItem::with_id(MenuId::new(OCR_ID), "截图翻译", true, None);
    let ocr_only = MenuItem::with_id(MenuId::new(OCR_ONLY_ID), "截图 OCR", true, None);
    let ocr_silent = MenuItem::with_id(MenuId::new(OCR_SILENT_ID), "静默截图 OCR", true, None);
    let ocr_finder = MenuItem::with_id(
        MenuId::new(OCR_FINDER_ID),
        #[cfg(target_os = "macos")]
        "访达选图 OCR",
        #[cfg(target_os = "windows")]
        "资源管理器选图 OCR",
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        "选中文件 OCR",
        true,
        None,
    );
    let overlay = MenuItem::with_id(MenuId::new(OVERLAY_ID), "原图翻译", true, None);
    let silent_replace =
        MenuItem::with_id(MenuId::new(SILENT_REPLACE_ID), "静默划词替换", true, None);
    let silent_input =
        MenuItem::with_id(MenuId::new(SILENT_INPUT_ID), "静默输入框替换", true, None);
    let pin = MenuItem::with_id(MenuId::new(PIN_ID), "固定 / 取消固定", true, None);
    let history = MenuItem::with_id(MenuId::new(HISTORY_ID), "历史", true, None);
    let favorites = MenuItem::with_id(MenuId::new(FAVORITES_ID), "收藏夹", true, None);
    let settings = MenuItem::with_id(MenuId::new(SETTINGS_ID), "设置…", true, None);
    let quit = MenuItem::with_id(MenuId::new(QUIT_ID), "退出", true, None);
    menu.append(&input)?;
    menu.append(&ocr)?;
    menu.append(&ocr_only)?;
    menu.append(&ocr_silent)?;
    menu.append(&ocr_finder)?;
    menu.append(&overlay)?;
    menu.append(&silent_replace)?;
    menu.append(&silent_input)?;
    menu.append(&pin)?;
    menu.append(&history)?;
    menu.append(&favorites)?;
    menu.append(&settings)?;
    menu.append(&quit)?;
    std::thread::spawn(move || {
        while let Ok(ev) = MenuEvent::receiver().recv() {
            let mapped = match ev.id.as_ref() {
                INPUT_ID => Some(TrayEvent::OpenInput),
                OCR_ID => Some(TrayEvent::Ocr),
                OCR_ONLY_ID => Some(TrayEvent::OcrOnly),
                OCR_SILENT_ID => Some(TrayEvent::OcrSilent),
                OCR_FINDER_ID => Some(TrayEvent::OcrFinder),
                OVERLAY_ID => Some(TrayEvent::OverlayTranslate),
                SILENT_REPLACE_ID => Some(TrayEvent::SilentReplace),
                SILENT_INPUT_ID => Some(TrayEvent::SilentInput),
                PIN_ID => Some(TrayEvent::PinToggle),
                HISTORY_ID => Some(TrayEvent::OpenHistory),
                FAVORITES_ID => Some(TrayEvent::OpenFavorites),
                SETTINGS_ID => Some(TrayEvent::OpenSettings),
                QUIT_ID => Some(TrayEvent::Quit),
                _ => None,
            };
            if let Some(t) = mapped {
                let _ = queue.send(t);
            }
        }
    });
    Ok(TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("划词翻译")
        .with_icon(icon())
        .build()?)
}

pub fn spawn_tray(queue: std::sync::mpsc::Sender<TrayEvent>) {
    #[cfg(target_os = "linux")]
    {
        std::thread::spawn(move || {
            if let Err(e) = gtk::init() {
                log::warn!("GTK 初始化失败，托盘不可用: {e}");
                return;
            }
            match build(queue) {
                Ok(_tray) => gtk::main(),
                Err(e) => log::warn!("创建托盘失败: {e}"),
            }
        });
    }
    #[cfg(not(target_os = "linux"))]
    {
        match build(queue) {
            Ok(tray) => std::mem::forget(tray),
            Err(e) => log::warn!("创建托盘失败: {e}"),
        }
    }
}
