//! 从文件管理器获取当前选中的文件路径（macOS 访达；其它平台尽力而为）。

use std::path::{Path, PathBuf};
use std::process::Command;

/// 返回文件管理器当前选中的文件路径（仅文件，不含目录）。
///
/// 访达/资源管理器未前台时也会返回上次选中；划词翻译请用
/// [`selected_file_paths_if_frontmost`]，避免网页划词误走 OCR。
pub fn selected_file_paths() -> anyhow::Result<Vec<PathBuf>> {
    #[cfg(target_os = "macos")]
    {
        selected_file_paths_macos(false)
    }
    #[cfg(target_os = "windows")]
    {
        selected_file_paths_windows(false)
    }
    #[cfg(target_os = "linux")]
    {
        selected_file_paths_linux()
    }
}

/// 仅当前台是文件管理器时返回选中文件，否则空列表。
pub fn selected_file_paths_if_frontmost() -> anyhow::Result<Vec<PathBuf>> {
    #[cfg(target_os = "macos")]
    {
        selected_file_paths_macos(true)
    }
    #[cfg(target_os = "windows")]
    {
        selected_file_paths_windows(true)
    }
    #[cfg(target_os = "linux")]
    {
        selected_file_paths_linux()
    }
}

/// 仅图片路径（访达选图 OCR 兼容入口）。
pub fn selected_image_paths() -> anyhow::Result<Vec<PathBuf>> {
    let paths: Vec<PathBuf> = selected_file_paths()?
        .into_iter()
        .filter(|p| is_image_path(p))
        .collect();
    if paths.is_empty() {
        anyhow::bail!("未选中图片文件（支持 png/jpg/jpeg/gif/webp/bmp/tiff/heic）");
    }
    Ok(paths)
}

pub fn is_image_path(p: &Path) -> bool {
    match p
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
    {
        Some(ext) => matches!(
            ext.as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "heic"
        ),
        None => false,
    }
}

/// 可读作纯文本翻译的文件。
pub fn is_text_path(p: &Path) -> bool {
    match p
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
    {
        Some(ext) => matches!(
            ext.as_str(),
            "txt"
                | "md"
                | "markdown"
                | "csv"
                | "tsv"
                | "json"
                | "xml"
                | "html"
                | "htm"
                | "log"
                | "rs"
                | "py"
                | "js"
                | "ts"
                | "tsx"
                | "jsx"
                | "go"
                | "java"
                | "c"
                | "cc"
                | "cpp"
                | "h"
                | "hpp"
                | "css"
                | "scss"
                | "yaml"
                | "yml"
                | "toml"
                | "ini"
                | "conf"
                | "cfg"
                | "sh"
                | "bash"
                | "zsh"
                | "bat"
                | "ps1"
                | "sql"
                | "swift"
                | "kt"
                | "rb"
                | "php"
                | "r"
                | "lua"
        ),
        // 无扩展名：小文件再试读
        None => true,
    }
}

/// 用 NSWorkspace 判断访达是否前台，避免 AppleScript 调 System Events
/// 触发「终端想要控制 System Events」授权框。
#[cfg(target_os = "macos")]
fn finder_is_frontmost() -> bool {
    use objc2_app_kit::NSWorkspace;

    let app = NSWorkspace::sharedWorkspace().frontmostApplication();
    let Some(app) = app else {
        return false;
    };
    let Some(id) = app.bundleIdentifier() else {
        return false;
    };
    id.to_string() == "com.apple.finder"
}

#[cfg(target_os = "macos")]
fn selected_file_paths_macos(require_frontmost: bool) -> anyhow::Result<Vec<PathBuf>> {
    if require_frontmost && !finder_is_frontmost() {
        return Ok(Vec::new());
    }
    let script = r#"
tell application "Finder"
  set sel to selection as alias list
  if (count of sel) is 0 then return ""
  set out to ""
  repeat with a in sel
    set out to out & (POSIX path of a) & linefeed
  end repeat
  return out
end tell
"#;
    let out = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .output()?;
    if !out.status.success() {
        anyhow::bail!(
            "读取访达选中失败: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(parse_path_lines(&String::from_utf8_lossy(&out.stdout)))
}

#[cfg(target_os = "windows")]
fn selected_file_paths_windows(require_frontmost: bool) -> anyhow::Result<Vec<PathBuf>> {
    // 尽力而为：读资源管理器窗口的选中项。失败则返回空（由剪贴板兜底）。
    let front_guard = if require_frontmost {
        r#"
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class FgWin {
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
}
"@
$hwnd = [FgWin]::GetForegroundWindow()
$pid = [uint32]0
[void][FgWin]::GetWindowThreadProcessId($hwnd, [ref]$pid)
$proc = (Get-Process -Id $pid -ErrorAction SilentlyContinue).ProcessName
if ($proc -ne 'explorer') { Write-Output ''; exit 0 }
"#
    } else {
        ""
    };
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
{front_guard}
$shell = New-Object -ComObject Shell.Application
$paths = @()
foreach ($w in $shell.Windows()) {{
  try {{
    if ($w.Document -eq $null) {{ continue }}
    $sel = $w.Document.SelectedItems()
    if ($sel.Count -gt 0) {{
      foreach ($i in $sel) {{ $paths += $i.Path }}
      break
    }}
  }} catch {{}}
}}
$paths -join "`n"
"#
    );
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()?;
    if !out.status.success() {
        anyhow::bail!(
            "读取资源管理器选中失败: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(parse_path_lines(&String::from_utf8_lossy(&out.stdout)))
}

#[cfg(target_os = "linux")]
fn selected_file_paths_linux() -> anyhow::Result<Vec<PathBuf>> {
    // 无统一文件管理器 API；尽力从 PRIMARY / CLIPBOARD 的 text/uri-list 取路径。
    let raw = if is_wayland() {
        cmd_stdout(
            "wl-paste",
            &["--primary", "--no-newline", "-t", "text/uri-list"],
        )
        .or_else(|| cmd_stdout("wl-paste", &["--no-newline", "-t", "text/uri-list"]))
    } else {
        cmd_stdout(
            "xclip",
            &["-selection", "primary", "-t", "text/uri-list", "-o"],
        )
        .or_else(|| {
            cmd_stdout(
                "xclip",
                &["-selection", "clipboard", "-t", "text/uri-list", "-o"],
            )
        })
    };
    Ok(raw.map(|s| parse_uri_list(&s)).unwrap_or_default())
}

#[cfg(target_os = "linux")]
fn is_wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE")
            .map(|v| v == "wayland")
            .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn cmd_stdout(bin: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(bin).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

#[cfg(target_os = "linux")]
fn parse_uri_list(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let s = l.strip_prefix("file://").unwrap_or(l);
            let decoded = urlencoding::decode(s).ok()?;
            Some(PathBuf::from(decoded.as_ref()))
        })
        .filter(|p| p.is_file())
        .collect()
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn parse_path_lines(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .collect()
}
