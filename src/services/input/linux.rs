//! Linux：X11 PRIMARY 或 Wayland wl-paste。

use arboard::{Clipboard, GetExtLinux, LinuxClipboardKind};

fn is_wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE")
            .map(|v| v == "wayland")
            .unwrap_or(false)
}

fn wl_paste_primary() -> Option<String> {
    let out = std::process::Command::new("wl-paste")
        .args(["--primary", "--no-newline", "--type", "text"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

fn arboard_primary() -> Option<String> {
    let mut cb = Clipboard::new().ok()?;
    cb.get().clipboard(LinuxClipboardKind::Primary).text().ok()
}

pub fn selected_text() -> Option<String> {
    let non_empty = |s: String| if s.trim().is_empty() { None } else { Some(s) };
    if is_wayland() {
        wl_paste_primary()
            .and_then(non_empty)
            .or_else(|| arboard_primary().and_then(non_empty))
    } else {
        arboard_primary().and_then(non_empty)
    }
}
