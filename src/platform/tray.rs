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
    const LOGO: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/packaging/icon.png"));
    let (rgba, w, h) = decode_logo_rgba(LOGO);
    let size = 64u32;
    let rgba = if w == size && h == size {
        rgba
    } else {
        resize_nearest(&rgba, w, h, size, size)
    };
    Icon::from_rgba(rgba, size, size).expect("icon")
}

fn decode_logo_rgba(bytes: &[u8]) -> (Vec<u8>, u32, u32) {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().expect("icon.png");
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("icon.png frame");
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..info.buffer_size()].to_vec(),
        png::ColorType::Rgb => {
            let rgb = &buf[..info.buffer_size()];
            let mut out = Vec::with_capacity((rgb.len() / 3) * 4);
            for &[r, g, b] in rgb.as_chunks::<3>().0 {
                out.extend_from_slice(&[r, g, b, 255]);
            }
            out
        }
        other => panic!("icon.png 不支持的色型: {other:?}"),
    };
    (rgba, info.width, info.height)
}

fn resize_nearest(src: &[u8], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<u8> {
    let mut out = vec![0u8; (dw * dh * 4) as usize];
    for y in 0..dh {
        let sy = y * sh / dh;
        for x in 0..dw {
            let sx = x * sw / dw;
            let si = ((sy * sw + sx) * 4) as usize;
            let di = ((y * dw + x) * 4) as usize;
            out[di..di + 4].copy_from_slice(&src[si..si + 4]);
        }
    }
    out
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
