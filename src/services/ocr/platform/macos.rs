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

pub(crate) fn prewarm_snip_helper() {
    crate::services::ocr::engines::apple_vision::AppleVisionEngine::warmup();
}

pub(super) fn capture_interactive_path() -> anyhow::Result<PathBuf> {
    capture_to_file(&["-i", "-x", "-t", "png"], "region")
}

pub(super) fn capture_interactive_annotated_path() -> anyhow::Result<PathBuf> {
    capture_to_file(&["-i", "-x", "-t", "png"], "screenshot")
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
