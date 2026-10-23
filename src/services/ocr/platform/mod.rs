//! 各平台截图实现。OCR 识别已拆到 `engines/`。

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use self::macos::*;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use self::windows::*;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use self::linux::*;

use std::path::{Path, PathBuf};

use crate::app::paths;

/// RGBA 图像（仅内部用）。
pub struct ImageData {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl ImageData {
    pub fn write_png(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let mut f = std::fs::File::create(path)?;
        let mut enc = png::Encoder::new(&mut f, self.width, self.height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header()?;
        w.write_image_data(&self.rgba)?;
        Ok(())
    }
}

pub fn capture_full_screen() -> anyhow::Result<(ImageData, PathBuf)> {
    platform_capture_full_screen()
}

pub fn capture_region(x: i32, y: i32, w: u32, h: u32) -> anyhow::Result<(ImageData, PathBuf)> {
    platform_capture_region(x, y, w, h)
}

/// 交互式框选，只返回 PNG 路径。
pub fn capture_interactive() -> anyhow::Result<PathBuf> {
    capture_interactive_path()
}

/// 交互式带标注工具栏截图，完成后默认复制到剪贴板，返回 PNG 路径。
pub fn capture_interactive_annotated() -> anyhow::Result<PathBuf> {
    capture_interactive_annotated_path()
}

#[cfg(target_os = "macos")]
pub fn prewarm_snip_helper() {
    macos::prewarm_snip_helper();
}

#[cfg(not(target_os = "macos"))]
pub fn prewarm_snip_helper() {}

pub fn new_png_path(prefix: &str) -> PathBuf {
    let name = format!(
        "st-{}-{}.png",
        prefix,
        chrono::Utc::now().timestamp_millis()
    );
    paths::ocr_cache_dir().join(name)
}

pub(super) fn read_png(path: &Path) -> anyhow::Result<ImageData> {
    let bytes = std::fs::read(path)?;
    let mut decoder = png::Decoder::new(bytes.as_slice());
    decoder.set_transformations(png::Transformations::ALPHA | png::Transformations::EXPAND);
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf)?;
    let bytes = &buf[..info.buffer_size()];
    let (w, h) = (info.width, info.height);
    let rgba = match info.color_type {
        png::ColorType::Rgba => bytes.to_vec(),
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity((w * h * 4) as usize);
            for chunk in bytes.chunks(3) {
                out.extend_from_slice(chunk);
                out.push(255);
            }
            out
        }
        png::ColorType::GrayscaleAlpha => {
            let mut out = Vec::with_capacity((w * h * 4) as usize);
            for chunk in bytes.chunks(2) {
                out.push(chunk[0]);
                out.push(chunk[0]);
                out.push(chunk[0]);
                out.push(chunk[1]);
            }
            out
        }
        png::ColorType::Grayscale => {
            let mut out = Vec::with_capacity((w * h * 4) as usize);
            for v in bytes {
                out.push(*v);
                out.push(*v);
                out.push(*v);
                out.push(255);
            }
            out
        }
        other => anyhow::bail!("不支持的 PNG 颜色类型: {:?}", other),
    };
    Ok(ImageData {
        width: w,
        height: h,
        rgba,
    })
}
