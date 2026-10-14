//! 选区读取 + 模拟复制兜底。封装为统一的 [`grab_selection`] /
//! [`grab_translate_source`]（文本 / 文件 / 图片分流）。

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::app::errors::{AppError, AppResult};
use crate::services::ocr::finder::{self, is_image_path, is_text_path};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

/// 单次翻译热键可处理的选区内容。
#[derive(Debug, Clone)]
pub enum TranslateSource {
    /// 可直接送翻译的文本（划词 / 文本文件内容）。
    Text(String),
    /// 需要 OCR 后再翻译的图片路径。
    Images(Vec<PathBuf>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrabError {
    Empty,
    NoPermission,
}

impl std::fmt::Display for GrabError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GrabError::Empty => f.write_str("没有可翻译的选中内容"),
            GrabError::NoPermission => f.write_str("缺少读取选区所需的系统权限"),
        }
    }
}

#[cfg(target_os = "macos")]
pub use macos::ensure_accessibility_permission;

/// 文本文件最大读取字节数，避免超大文件拖垮翻译。
const MAX_TEXT_FILE_BYTES: u64 = 200_000;

/// Cmd/Ctrl 快捷键字母：macOS 用虚拟键码，避免中文输入法下 `Key::Unicode` 触发 UCKeyTranslate(-25340)。
fn chord_letter(c: char) -> enigo::Key {
    #[cfg(target_os = "macos")]
    {
        // HIToolbox virtual keycodes (ANSI layout physical keys)
        use enigo::Key;
        match c.to_ascii_lowercase() {
            'a' => Key::Other(0x00),
            'c' => Key::Other(0x08),
            'v' => Key::Other(0x09),
            _ => Key::Unicode(c),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        enigo::Key::Unicode(c.to_ascii_lowercase())
    }
}

/// Option 热键触发后 Alt 可能仍按住，先松开再发 Cmd 组合键。
fn release_option_modifiers(enigo: &mut enigo::Enigo) {
    use enigo::{Direction::*, Key, Keyboard};
    let _ = enigo.key(Key::Alt, Release);
    #[cfg(target_os = "macos")]
    {
        // Option 热键偶发带上 Shift
        let _ = enigo.key(Key::Shift, Release);
    }
}

pub fn grab_selection() -> Result<String, GrabError> {
    #[cfg(target_os = "macos")]
    if !macos::is_accessibility_trusted() {
        return Err(GrabError::NoPermission);
    }
    if let Some(t) = native_selected_text() {
        return Ok(t);
    }
    copy_fallback()?
        .and_then(|s| {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        })
        .ok_or(GrabError::Empty)
}

/// 划词翻译统一入口：文本直接返回；图片路径交由调用方 OCR。
pub fn grab_translate_source() -> Result<TranslateSource, GrabError> {
    #[cfg(target_os = "macos")]
    if !macos::is_accessibility_trusted() {
        return Err(GrabError::NoPermission);
    }

    // 1) 原生选中文字（编辑器 / 浏览器划词）
    if let Some(t) = native_selected_text() {
        return Ok(TranslateSource::Text(t));
    }

    // 2) 仅当前台是文件管理器时用其选中文件。访达在后台仍保留上次选图，
    //    网页划词读不到 AX 选区时会误走 OCR。
    if let Ok(paths) = finder::selected_file_paths_if_frontmost()
        && let Some(src) = classify_paths(&paths)
    {
        return Ok(src);
    }

    // 3) 模拟复制：文件列表 → 文本 → 图片（网页复制常同时带图片 flavor）
    inspect_after_copy()
}

fn native_selected_text() -> Option<String> {
    #[cfg(target_os = "linux")]
    let native = linux::selected_text();
    #[cfg(target_os = "macos")]
    let native = macos::selected_text();
    #[cfg(target_os = "windows")]
    let native = windows::selected_text();
    native.and_then(|s| {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    })
}

fn classify_paths(paths: &[PathBuf]) -> Option<TranslateSource> {
    let mut images = Vec::new();
    let mut texts = Vec::new();
    for p in paths {
        if !p.is_file() {
            continue;
        }
        if is_image_path(p) {
            images.push(p.clone());
        } else if is_text_path(p) {
            texts.push(p.clone());
        }
    }
    if !images.is_empty() && texts.is_empty() {
        return Some(TranslateSource::Images(images));
    }
    if !texts.is_empty() && images.is_empty() {
        let body = read_text_files(&texts)?;
        return Some(TranslateSource::Text(body));
    }
    // 混选：文本优先拼在一起；图片路径附在后面由调用方 OCR 更复杂，
    // 这里简化为有文本就翻文本，否则翻图。
    if !texts.is_empty() {
        let body = read_text_files(&texts)?;
        return Some(TranslateSource::Text(body));
    }
    if !images.is_empty() {
        return Some(TranslateSource::Images(images));
    }
    None
}

fn read_text_files(paths: &[PathBuf]) -> Option<String> {
    let mut parts = Vec::new();
    for p in paths {
        match read_text_file(p) {
            Ok(s) if !s.trim().is_empty() => parts.push(s),
            Ok(_) => {}
            Err(e) => log::warn!("读取文件失败 {}: {e}", p.display()),
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n---\n\n"))
    }
}

fn read_text_file(path: &Path) -> anyhow::Result<String> {
    let meta = std::fs::metadata(path)?;
    if meta.len() > MAX_TEXT_FILE_BYTES {
        anyhow::bail!("文件过大（{} KB），请截取后再译", meta.len() / 1024);
    }
    let bytes = std::fs::read(path)?;
    // 拒绝明显二进制
    if bytes.iter().take(512).filter(|&&b| b == 0).count() > 8 {
        anyhow::bail!("不是文本文件");
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Cmd/Ctrl+C 后检查剪贴板：文件列表 → 文本 → 图片。
///
/// 浏览器划词复制常同时提供 `public.utf8-plain-text` 和选区截图 flavor；
/// 若图片优先会把划词翻译误判成 OCR。
fn inspect_after_copy() -> Result<TranslateSource, GrabError> {
    use enigo::{Direction::*, Enigo, Key, Keyboard, Settings};

    let Ok(mut cb) = arboard::Clipboard::new() else {
        return Err(GrabError::Empty);
    };
    let backup_text = cb.get_text().ok();
    let _ = cb.clear();

    let mut enigo = match Enigo::new(&Settings::default()) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("无法模拟按键: {e}");
            restore_clipboard_text(&mut cb, backup_text);
            #[cfg(target_os = "macos")]
            if matches!(e, enigo::NewConError::NoPermission) {
                return Err(GrabError::NoPermission);
            }
            return Err(GrabError::Empty);
        }
    };
    release_option_modifiers(&mut enigo);
    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;
    if enigo.key(modifier, Press).is_err() {
        restore_clipboard_text(&mut cb, backup_text);
        return Err(GrabError::Empty);
    }
    let _ = enigo.key(chord_letter('c'), Click);
    let _ = enigo.key(modifier, Release);

    let mut files_src = None;
    let mut text_src = None;
    let mut image_src = None;
    for _ in 0..12 {
        std::thread::sleep(Duration::from_millis(30));

        if files_src.is_none()
            && let Ok(files) = cb.get().file_list()
        {
            let files: Vec<PathBuf> = files.into_iter().filter(|p| p.is_file()).collect();
            if !files.is_empty() {
                files_src = classify_paths(&files);
            }
        }

        if text_src.is_none()
            && let Ok(t) = cb.get_text()
        {
            let t = t.trim();
            if !t.is_empty() {
                text_src = Some(t.to_string());
            }
        }

        if image_src.is_none()
            && let Ok(img) = cb.get_image()
        {
            match save_clipboard_image(&img) {
                Ok(path) => image_src = Some(path),
                Err(e) => log::warn!("保存剪贴板图片失败: {e:#}"),
            }
        }

        if files_src.is_some() || text_src.is_some() {
            break;
        }
    }

    restore_clipboard_text(&mut cb, backup_text);
    if let Some(src) = files_src {
        Ok(src)
    } else if let Some(t) = text_src {
        Ok(TranslateSource::Text(t))
    } else if let Some(path) = image_src {
        Ok(TranslateSource::Images(vec![path]))
    } else {
        Err(GrabError::Empty)
    }
}

fn restore_clipboard_text(cb: &mut arboard::Clipboard, backup: Option<String>) {
    match backup {
        Some(b) => {
            let _ = cb.set_text(b);
        }
        None => {
            let _ = cb.clear();
        }
    }
}

fn save_clipboard_image(img: &arboard::ImageData<'_>) -> anyhow::Result<PathBuf> {
    let w = img.width as u32;
    let h = img.height as u32;
    if w == 0 || h == 0 {
        anyhow::bail!("空图片");
    }
    let path = std::env::temp_dir().join(format!("swipelingo-clip-{}.png", uuid::Uuid::new_v4()));
    let mut f = std::fs::File::create(&path)?;
    let mut enc = png::Encoder::new(&mut f, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header()?;
    writer.write_image_data(img.bytes.as_ref())?;
    Ok(path)
}

pub fn copy_fallback() -> Result<Option<String>, GrabError> {
    use enigo::{Direction::*, Enigo, Key, Keyboard, Settings};

    let Ok(mut cb) = arboard::Clipboard::new() else {
        return Ok(None);
    };
    let backup = cb.get_text().ok();
    let _ = cb.clear();

    let mut enigo = match Enigo::new(&Settings::default()) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("无法模拟按键: {e}");
            if let Some(b) = backup {
                let _ = cb.set_text(b);
            }
            #[cfg(target_os = "macos")]
            if matches!(e, enigo::NewConError::NoPermission) {
                return Err(GrabError::NoPermission);
            }
            return Ok(None);
        }
    };
    release_option_modifiers(&mut enigo);
    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;
    if enigo.key(modifier, Press).is_err() {
        if let Some(b) = backup {
            let _ = cb.set_text(b);
        }
        return Ok(None);
    }
    let _ = enigo.key(chord_letter('c'), Click);
    let _ = enigo.key(modifier, Release);

    let mut text = None;
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(30));
        if let Ok(t) = cb.get_text()
            && !t.is_empty()
        {
            text = Some(t);
            break;
        }
    }
    match backup {
        Some(b) => {
            let _ = cb.set_text(b);
        }
        None => {
            let _ = cb.clear();
        }
    }
    Ok(text)
}

/// 把 GrabError 转成 AppError。
pub fn to_app_err(e: GrabError) -> AppError {
    match e {
        GrabError::Empty => AppError::Selection("没有选中的文字".into()),
        GrabError::NoPermission => AppError::Permission("缺少读取选区所需的系统权限".into()),
    }
}

/// 仅用于：把模拟复制后从剪贴板拿到的字符串标记为 OK。
pub type AppGrabResult = AppResult<String>;

/// 读取焦点输入框全文。
///
/// macOS 优先 AX `AXValue`，Windows 优先 UIA ValuePattern；读不到再全选 + 复制兜底。
pub fn grab_focused_field() -> Result<String, GrabError> {
    #[cfg(target_os = "macos")]
    {
        if !macos::is_accessibility_trusted() {
            return Err(GrabError::NoPermission);
        }
        if let Some(t) = macos::focused_value() {
            let t = t.trim();
            if !t.is_empty() {
                return Ok(t.to_string());
            }
        }
        log::debug!("AXValue 未读到焦点框全文，尝试 Cmd+A / Cmd+C");
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(t) = windows::focused_value() {
            let t = t.trim();
            if !t.is_empty() {
                return Ok(t.to_string());
            }
        }
        log::debug!("UIA ValuePattern 未读到焦点框全文，尝试 Ctrl+A / Ctrl+C");
    }
    select_all()?;
    copy_fallback()?
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or(GrabError::Empty)
}

/// 用译文替换当前选区。优先 AX `setSelectedText`，否则剪贴板备份 → Cmd/Ctrl+V → 恢复。
pub fn replace_selection(text: &str) -> Result<(), GrabError> {
    #[cfg(target_os = "macos")]
    {
        if !macos::is_accessibility_trusted() {
            return Err(GrabError::NoPermission);
        }
        if macos::set_selected_text(text) {
            return Ok(());
        }
    }
    paste_text(text)
}

/// 用译文替换焦点输入框全文。优先原生 API，否则全选后粘贴。
pub fn replace_focused_field(text: &str) -> Result<(), GrabError> {
    #[cfg(target_os = "macos")]
    {
        if !macos::is_accessibility_trusted() {
            return Err(GrabError::NoPermission);
        }
        if macos::set_focused_value(text) {
            return Ok(());
        }
    }
    #[cfg(target_os = "windows")]
    {
        if windows::set_focused_value(text) {
            return Ok(());
        }
    }
    select_all()?;
    paste_text(text)
}

fn select_all() -> Result<(), GrabError> {
    use enigo::{Direction::*, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| {
        log::warn!("无法模拟按键: {e}");
        #[cfg(target_os = "macos")]
        if matches!(e, enigo::NewConError::NoPermission) {
            return GrabError::NoPermission;
        }
        GrabError::Empty
    })?;
    release_option_modifiers(&mut enigo);
    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;
    let _ = enigo.key(modifier, Press);
    let _ = enigo.key(chord_letter('a'), Click);
    let _ = enigo.key(modifier, Release);
    std::thread::sleep(Duration::from_millis(40));
    Ok(())
}

/// 剪贴板备份 → 写入 text → 粘贴 → 恢复剪贴板。
fn paste_text(text: &str) -> Result<(), GrabError> {
    use enigo::{Direction::*, Enigo, Key, Keyboard, Settings};

    let Ok(mut cb) = arboard::Clipboard::new() else {
        return Err(GrabError::Empty);
    };
    let backup = cb.get_text().ok();
    if cb.set_text(text.to_string()).is_err() {
        return Err(GrabError::Empty);
    }
    std::thread::sleep(Duration::from_millis(30));

    let mut enigo = match Enigo::new(&Settings::default()) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("无法模拟按键: {e}");
            if let Some(b) = backup {
                let _ = cb.set_text(b);
            }
            #[cfg(target_os = "macos")]
            if matches!(e, enigo::NewConError::NoPermission) {
                return Err(GrabError::NoPermission);
            }
            return Err(GrabError::Empty);
        }
    };
    release_option_modifiers(&mut enigo);
    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;
    let _ = enigo.key(modifier, Press);
    let _ = enigo.key(chord_letter('v'), Click);
    let _ = enigo.key(modifier, Release);
    std::thread::sleep(Duration::from_millis(80));

    match backup {
        Some(b) => {
            let _ = cb.set_text(b);
        }
        None => {
            let _ = cb.clear();
        }
    }
    Ok(())
}
