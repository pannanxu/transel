//! 带 bounding box 的 OCR（原图翻译用）。
//!
//! 使用 RapidOCR 检测框；坐标系与历史 Vision 约定一致：
//! 归一化矩形原点在图片**左下角**，x/y/w/h ∈ [0,1]，供 overlay 布局换算。

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::services::ocr::engines::rapidocr::RapidOcrEngine;

/// 归一化矩形：原点在图片**左下角**，x/y/w/h ∈ [0,1]。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextBox {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    #[serde(default)]
    pub confidence: f32,
}

/// 识别带框文本；失败或无结果时返回空 Vec。
pub fn recognize_boxes(image: &Path) -> Vec<TextBox> {
    match recognize_rapidocr_boxes(image) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("RapidOCR 带框识别失败: {e:#}");
            Vec::new()
        }
    }
}

fn recognize_rapidocr_boxes(image: &Path) -> anyhow::Result<Vec<TextBox>> {
    let (img_w, img_h) = image_size(image)
        .ok_or_else(|| anyhow::anyhow!("无法读取图片尺寸"))?;
    if img_w == 0 || img_h == 0 {
        anyhow::bail!("图片尺寸无效");
    }
    let w = img_w as f32;
    let h = img_h as f32;

    let lines = RapidOcrEngine::recognize_lines(image)?;
    let mut out = Vec::with_capacity(lines.len());
    for line in lines {
        let text = line.text.trim();
        if text.is_empty() {
            continue;
        }
        let pts = &line.points;
        let min_x = pts.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
        let max_x = pts.iter().map(|p| p[0]).fold(f32::NEG_INFINITY, f32::max);
        let min_y = pts.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
        let max_y = pts.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max);
        // RapidOCR：像素坐标原点在左上；转成左下原点归一化（兼容 overlay）
        let box_w = ((max_x - min_x) / w).clamp(0., 1.);
        let box_h = ((max_y - min_y) / h).clamp(0., 1.);
        let x = (min_x / w).clamp(0., 1.);
        let y = ((h - max_y) / h).clamp(0., 1.);
        out.push(TextBox {
            text: text.to_string(),
            x,
            y,
            w: box_w,
            h: box_h,
            confidence: line.score,
        });
    }
    Ok(out)
}

/// 读取 PNG 尺寸（宽高）。
pub fn image_size(path: &Path) -> Option<(u32, u32)> {
    let bytes = std::fs::read(path).ok()?;
    let decoder = png::Decoder::new(bytes.as_slice());
    let reader = decoder.read_info().ok()?;
    let info = reader.info();
    Some((info.width, info.height))
}
