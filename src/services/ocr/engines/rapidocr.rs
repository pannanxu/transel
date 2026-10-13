//! RapidOCR（PP-OCRv5 中文 mobile）本地 ONNX 引擎。
//!
//! 首次使用会从 ModelScope 下载模型到缓存目录；之后常驻内存复用 session。

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use rapidocr_core::config::PipelineConfig;
use rapidocr_core::model::{model_set_by_name, ModelCache, ModelDownloadMode};
use rapidocr_core::RapidOcr;

use crate::app::paths;
use crate::services::ocr::engine::OcrEngine;
use crate::services::ocr::ort_runtime;

const MODEL_SET: &str = "ppocrv5-ch-mobile";

/// 进程内共享 runner，避免文本 OCR 与带框 OCR 各加载一套模型。
static RUNNER: Mutex<Option<RapidOcr>> = Mutex::new(None);

/// 单行识别结果（像素四边形，原点左上）。
#[derive(Debug, Clone)]
pub struct DetectedLine {
    pub text: String,
    pub score: f32,
    pub points: [[f32; 2]; 4],
}

pub struct RapidOcrEngine;

impl RapidOcrEngine {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }

    fn model_dir() -> PathBuf {
        let dir = paths::cache_dir().join("rapidocr");
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn ensure_runner(slot: &mut Option<RapidOcr>) -> anyhow::Result<&mut RapidOcr> {
        if slot.is_none() {
            // 必须先于任何 ort / RapidOCR session 调用
            ort_runtime::ensure_loaded()?;
            let model_set = model_set_by_name(MODEL_SET)
                .ok_or_else(|| anyhow::anyhow!("未知 RapidOCR 模型集: {MODEL_SET}"))?;
            let cache = ModelCache::new(Self::model_dir());
            let pipeline = PipelineConfig::without_cls(); // 截图一般正向，省一次分类
            log::info!(
                "准备 RapidOCR 模型 ({MODEL_SET}) → {}",
                cache.root().display()
            );
            cache.ensure_model_set_for_pipeline(
                model_set,
                pipeline,
                ModelDownloadMode::Missing,
            )?;
            let cfg = cache.config_for(model_set).with_pipeline(pipeline);
            let ocr = RapidOcr::from_config(cfg)?;
            *slot = Some(ocr);
            log::info!("RapidOCR 引擎已加载");
        }
        Ok(slot.as_mut().unwrap())
    }

    /// 返回带检测框的行（像素坐标，原点左上）。供原图翻译 overlay 使用。
    pub fn recognize_lines(image: &Path) -> anyhow::Result<Vec<DetectedLine>> {
        let mut g = RUNNER.lock();
        let ocr = Self::ensure_runner(&mut g)?;
        let out = ocr
            .run_path(image)
            .map_err(|e| anyhow::anyhow!("RapidOCR 识别失败: {e:#}"))?;
        Ok(out
            .lines
            .into_iter()
            .map(|line| DetectedLine {
                text: line.text,
                score: line.score,
                points: line.bbox.points,
            })
            .collect())
    }
}

impl OcrEngine for RapidOcrEngine {
    fn id(&self) -> &'static str {
        "rapidocr"
    }

    fn display_name(&self) -> &'static str {
        "RapidOCR"
    }

    fn warmup(&self) {
        let mut g = RUNNER.lock();
        if let Err(e) = Self::ensure_runner(&mut g) {
            log::warn!("RapidOCR 预热失败（首次识别时会重试）: {e:#}");
        }
    }

    fn recognize(&self, image: &Path, _language_hint: Option<&str>) -> anyhow::Result<String> {
        let mut lines = Self::recognize_lines(image)?;
        lines.sort_by(|a, b| {
            let ay = centroid_y(&a.points);
            let by = centroid_y(&b.points);
            ay.partial_cmp(&by).unwrap_or(Ordering::Equal).then_with(|| {
                let ax = centroid_x(&a.points);
                let bx = centroid_x(&b.points);
                ax.partial_cmp(&bx).unwrap_or(Ordering::Equal)
            })
        });

        let mut rows: Vec<(f32, Vec<String>)> = Vec::new();
        for line in lines {
            let text = line.text.trim();
            if text.is_empty() {
                continue;
            }
            let cy = centroid_y(&line.points);
            let h = box_height(&line.points).max(8.0);
            if let Some((ref_y, row)) = rows.last_mut() {
                if (cy - *ref_y).abs() < h * 0.6 {
                    row.push(text.to_string());
                    continue;
                }
            }
            rows.push((cy, vec![text.to_string()]));
        }
        Ok(rows
            .into_iter()
            .map(|(_, parts)| parts.join(""))
            .collect::<Vec<_>>()
            .join("\n"))
    }
}

fn centroid_x(points: &[[f32; 2]; 4]) -> f32 {
    points.iter().map(|p| p[0]).sum::<f32>() / 4.0
}

fn centroid_y(points: &[[f32; 2]; 4]) -> f32 {
    points.iter().map(|p| p[1]).sum::<f32>() / 4.0
}

fn box_height(points: &[[f32; 2]; 4]) -> f32 {
    let min_y = points.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
    let max_y = points.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max);
    max_y - min_y
}
