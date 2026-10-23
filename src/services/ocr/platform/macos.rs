//! macOS 截图（OCR 识别见 `engines/`）。

use std::path::PathBuf;
use std::process::Command;

use crate::services::ocr::platform::{ImageData, new_png_path, read_png};

pub(super) fn platform_capture_full_screen() -> anyhow::Result<(ImageData, PathBuf)> {
    let path = capture_to_file(&["-x", "-t", "png"], "full")?;
    let img = read_png(&path)?;
    Ok((img, path))
}

pub(super) fn platform_capture_region(
    x: i32,
    y: i32,
    w: u32,
    h: u32,
) -> anyhow::Result<(ImageData, PathBuf)> {
    if w > 1 && h > 1 {
        let region = format!("{x},{y},{w},{h}");
        let path = capture_to_file(&["-R", &region, "-x", "-t", "png"], "region")?;
        let img = read_png(&path)?;
        return Ok((img, path));
    }
    let path = capture_interactive_path()?;
    let img = read_png(&path)?;
    Ok((img, path))
}

const SNIP_HELPER_SRC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/macos/apple-snip.swift"
));

fn ensure_snip_helper() -> anyhow::Result<PathBuf> {
    let dir = crate::app::paths::cache_dir().join("apple-snip");
    let src_path = dir.join("apple-snip.swift");
    let bin_path = dir.join("apple-snip");
    std::fs::create_dir_all(&dir)?;

    let src_changed = std::fs::read_to_string(&src_path)
        .map(|old| old != SNIP_HELPER_SRC)
        .unwrap_or(true);

    if src_changed {
        std::fs::write(&src_path, SNIP_HELPER_SRC)?;
    }

    let need_build = src_changed
        || !bin_path.exists()
        || (|| {
            let src_m = std::fs::metadata(&src_path).ok()?.modified().ok()?;
            let bin_m = std::fs::metadata(&bin_path).ok()?.modified().ok()?;
            Some(bin_m < src_m)
        })()
        .unwrap_or(true);

    if need_build {
        log::info!("正在编译截图选区助手: {}", bin_path.display());
        let compile = Command::new("/usr/bin/swiftc")
            .args([
                "-O",
                "-o",
                bin_path.to_str().unwrap_or("apple-snip"),
                "-framework",
                "AppKit",
                src_path.to_str().unwrap_or("apple-snip.swift"),
            ])
            .output()?;

        if !compile.status.success() {
            anyhow::bail!(
                "编译截图选区助手失败: {}",
                String::from_utf8_lossy(&compile.stderr).trim()
            );
        }

        let _ = Command::new("/usr/bin/codesign")
            .args(["--force", "--sign", "-", "--timestamp=none"])
            .arg(&bin_path)
            .output();
    }

    Ok(bin_path)
}

pub(super) fn capture_interactive_path() -> anyhow::Result<PathBuf> {
    let path = new_png_path("region");

    // 优先采用高精选区截图助手（暗色平滑遮罩、高亮精细描边、角标尺寸标签与操作指引）
    if let Ok(helper_bin) = ensure_snip_helper() {
        if let Ok(res) = Command::new(&helper_bin).arg(&path).output() {
            if res.status.success() && path.exists() {
                return Ok(path);
            }
            if res.status.code() == Some(1) {
                anyhow::bail!("用户已取消截图");
            }
        }
        log::warn!("美化选区截图未完成，尝试使用系统原生截图工具");
    }

    // 兜底：原生 screencapture
    capture_to_file(&["-i", "-x", "-t", "png"], "region")
}

fn capture_to_file(extra_args: &[&str], prefix: &str) -> anyhow::Result<PathBuf> {
    let path = new_png_path(prefix);
    let mut args: Vec<&str> = extra_args.to_vec();
    let path_str = path.to_str().unwrap();
    args.push(path_str);
    let out = Command::new("/usr/sbin/screencapture")
        .args(&args)
        .output()?;
    if !out.status.success() {
        anyhow::bail!(
            "screencapture 失败: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    if !path.exists() {
        anyhow::bail!("screencapture 未生成文件（可能已取消）");
    }
    Ok(path)
}
