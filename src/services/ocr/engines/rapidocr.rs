//! RapidOCR（PP-OCRv5 中文 mobile）本地 ONNX 引擎。
//!
//! 首次使用会从 ModelScope 下载模型到缓存目录；之后常驻内存复用 session。

use std::cmp::Ordering;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use rapidocr_core::RapidOcr;
use rapidocr_core::config::PipelineConfig;
use rapidocr_core::model::{
    ModelAssetSpec, ModelCache, ModelDownloadMode, ModelSetSpec, model_set_by_name,
};
use sha2::{Digest, Sha256};

use crate::app::paths;
use crate::services::ocr::engine::OcrEngine;
use crate::services::ocr::ort_runtime;

pub const RAPIDOCR_MODELS: &[(&str, &str)] = &[
    ("ppocrv5-ch-server", "PP-OCRv5 中文高精模型（推荐）"),
    ("ppocrv5-ch-mobile", "PP-OCRv5 中文轻量模型（默认）"),
    ("ppocrv6-medium", "PP-OCRv6 中型模型（最新）"),
    ("ppocrv6-small", "PP-OCRv6 小型模型"),
    ("ppocrv5-en-mobile", "PP-OCRv5 英文轻量模型"),
];

pub const DEFAULT_RAPIDOCR_MODEL: &str = "ppocrv5-ch-mobile";

struct ActiveRunner {
    model: String,
    threads: usize,
    ocr: RapidOcr,
}

/// 进程内共享 runner，避免文本 OCR 与带框 OCR 各加载一套模型。
static RUNNER: Mutex<Option<ActiveRunner>> = Mutex::new(None);
static GLOBAL_CONFIG: Mutex<(String, usize)> = Mutex::new((String::new(), 0));

fn resolve_threads(threads: usize) -> usize {
    if threads == 0 {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .clamp(1, 8)
    } else {
        threads.clamp(1, 32)
    }
}

/// 实时下载进度状态（供设置界面平滑刷新显示）。
#[derive(Debug, Clone, Default)]
pub struct DownloadProgressState {
    pub is_downloading: bool,
    pub model_name: String,
    pub current_filename: String,
    pub file_index: usize,
    pub total_files: usize,
    pub file_downloaded_bytes: u64,
    pub file_total_bytes: Option<u64>,
}

static DOWNLOAD_PROGRESS: Mutex<DownloadProgressState> = Mutex::new(DownloadProgressState {
    is_downloading: false,
    model_name: String::new(),
    current_filename: String::new(),
    file_index: 0,
    total_files: 0,
    file_downloaded_bytes: 0,
    file_total_bytes: None,
});

pub fn get_download_progress() -> DownloadProgressState {
    DOWNLOAD_PROGRESS.lock().clone()
}

/// 校验文件的 SHA-256 哈希值
fn verify_file_sha256(path: &Path, expected_hex: &str) -> bool {
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => hasher.update(&buf[..n]),
            Err(_) => return false,
        }
    }
    let actual_hex = format!("{:x}", hasher.finalize());
    actual_hex.eq_ignore_ascii_case(expected_hex)
}

/// 快速检查资产在本地是否存在且非空（仅做元数据探测，不阻塞执行全量哈希）
fn is_asset_present(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.len() > 0)
        .unwrap_or(false)
}

/// 检查某个资产在本地是否完整且校验通过（供下载流程验证使用，包含哈希校验）
fn is_asset_verified(path: &Path, asset: &ModelAssetSpec) -> bool {
    if !path.is_file() {
        return false;
    }
    if let Some(expected_sha) = asset.sha256 {
        verify_file_sha256(path, expected_sha)
    } else {
        std::fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false)
    }
}

/// 使用带长超时的流式下载获取 RapidOCR 模型资产，实时更新字节进度与计算哈希，写入 `.partial` 避免断点残留。
fn download_asset_file(
    asset: &ModelAssetSpec,
    dest: &Path,
    file_index: usize,
    total_files: usize,
    model_name: &str,
) -> anyhow::Result<()> {
    if is_asset_verified(dest, asset) {
        return Ok(());
    }
    let _ = std::fs::remove_file(dest);

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let partial = dest.with_extension("partial");
    let _ = std::fs::remove_file(&partial);

    log::info!(
        "开始下载 RapidOCR 资产 [{file_index}/{total_files}]: {} 从 {}",
        asset.name,
        asset.url
    );

    // 15 分钟全局超时，允许大模型在慢速网络下完整下载
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(900)))
        .build()
        .into();

    let resp = agent
        .get(asset.url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) transel",
        )
        .call()
        .map_err(|e| anyhow::anyhow!("下载请求失败 ({}): {e}", asset.url))?;

    let code = resp.status().as_u16();
    if !(200..300).contains(&code) {
        anyhow::bail!("下载 {} 失败: HTTP {code}", asset.url);
    }

    let file_total_bytes = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());

    {
        let mut prog = DOWNLOAD_PROGRESS.lock();
        prog.is_downloading = true;
        prog.model_name = model_name.to_string();
        prog.current_filename = asset.filename.to_string();
        prog.file_index = file_index;
        prog.total_files = total_files;
        prog.file_downloaded_bytes = 0;
        prog.file_total_bytes = file_total_bytes;
    }

    let mut reader = resp.into_body().into_reader();
    let mut file = std::fs::File::create(&partial)?;
    let mut buf = [0u8; 65536];
    let mut downloaded = 0u64;
    let mut hasher = Sha256::new();

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        hasher.update(&buf[..n]);
        downloaded += n as u64;
        DOWNLOAD_PROGRESS.lock().file_downloaded_bytes = downloaded;
    }

    file.flush()?;
    drop(file);

    // 长度完整性检查
    if let Some(expected_len) = file_total_bytes
        && downloaded < expected_len
    {
        let _ = std::fs::remove_file(&partial);
        anyhow::bail!(
            "下载不完整: 仅接收到 {} / {} 字节，请重新下载",
            downloaded,
            expected_len
        );
    }

    // SHA-256 完整性哈希校验
    if let Some(expected_sha) = asset.sha256 {
        let actual_sha = format!("{:x}", hasher.finalize());
        if !actual_sha.eq_ignore_ascii_case(expected_sha) {
            let _ = std::fs::remove_file(&partial);
            anyhow::bail!(
                "模型文件校验失败 (SHA-256 不匹配): 期望 {expected_sha}, 实际 {actual_sha}，已自动清理损坏文件，请重试下载"
            );
        }
    }

    std::fs::rename(&partial, dest)?;
    log::info!("RapidOCR 资产下载完成且校验通过: {}", asset.filename);
    Ok(())
}

/// 模型详细状态信息（供设置页展示及下载管理）
#[derive(Debug, Clone)]
pub struct RapidOcrModelInfo {
    pub name: String,
    pub display_name: String,
    pub is_downloaded: bool,
    pub total_size_bytes: u64,
    pub missing_files: usize,
    pub total_files: usize,
    pub dir: PathBuf,
}

static DOWNLOADING_MODEL: Mutex<Option<String>> = Mutex::new(None);

pub fn is_downloading_model(model_name: &str) -> bool {
    DOWNLOADING_MODEL.lock().as_deref() == Some(model_name)
}

pub fn is_any_model_downloading() -> bool {
    DOWNLOADING_MODEL.lock().is_some()
}

pub fn set_downloading_model(model: Option<String>) {
    *DOWNLOADING_MODEL.lock() = model;
}

/// 检查指定模型的本地缓存状态（仅做轻量元数据探测，不阻塞 UI 线程执行耗时哈希校验）。
pub fn check_model_status(model_name: &str) -> Option<RapidOcrModelInfo> {
    let spec = model_set_by_name(model_name)?;
    let cache = ModelCache::new(RapidOcrEngine::model_dir());
    let pipeline = PipelineConfig::without_cls();
    let assets = spec.assets_for_pipeline(pipeline);
    let mut total_size = 0u64;
    let mut missing_count = 0usize;
    let total_files = assets.len();

    for asset in &assets {
        let path = cache.asset_path(*asset);
        if let Ok(meta) = std::fs::metadata(&path)
            && meta.is_file()
            && meta.len() > 0
        {
            total_size += meta.len();
            continue;
        }
        missing_count += 1;
    }

    let display_name = RAPIDOCR_MODELS
        .iter()
        .find(|(id, _)| *id == model_name)
        .map(|(_, d)| *d)
        .unwrap_or(spec.name)
        .to_string();

    Some(RapidOcrModelInfo {
        name: model_name.to_string(),
        display_name,
        is_downloaded: missing_count == 0,
        total_size_bytes: total_size,
        missing_files: missing_count,
        total_files,
        dir: cache.root().to_path_buf(),
    })
}

/// 手动下载指定 RapidOCR 模型的全部缺失资产（供设置界面触发）。
pub fn download_model(model_name: &str) -> anyhow::Result<PathBuf> {
    let spec = model_set_by_name(model_name)
        .ok_or_else(|| anyhow::anyhow!("未知 RapidOCR 模型集: {model_name}"))?;
    let cache = ModelCache::new(RapidOcrEngine::model_dir());
    let pipeline = PipelineConfig::without_cls();
    let missing = cache.missing_assets_for_pipeline(spec, pipeline);
    let total_files = missing.len();

    set_downloading_model(Some(model_name.to_string()));
    {
        let mut prog = DOWNLOAD_PROGRESS.lock();
        prog.is_downloading = true;
        prog.model_name = model_name.to_string();
        prog.total_files = total_files;
        prog.file_index = 0;
        prog.file_downloaded_bytes = 0;
        prog.file_total_bytes = None;
    }

    let result = (|| -> anyhow::Result<()> {
        log::info!(
            "下载 RapidOCR 模型 ({}) 资产 ({} 个文件)",
            spec.name,
            total_files
        );
        for (i, asset) in missing.iter().enumerate() {
            let dest = cache.asset_path(*asset);
            download_asset_file(asset, &dest, i + 1, total_files, model_name)?;
        }
        cache.ensure_model_set_for_pipeline(spec, pipeline, ModelDownloadMode::Missing)?;
        Ok(())
    })();

    set_downloading_model(None);
    {
        let mut prog = DOWNLOAD_PROGRESS.lock();
        prog.is_downloading = false;
    }

    result?;
    Ok(cache.root().to_path_buf())
}

/// 单行识别结果（像素四边形，原点左上）。
#[derive(Debug, Clone)]
pub struct DetectedLine {
    pub text: String,
    pub score: f32,
    pub points: [[f32; 2]; 4],
}

pub struct RapidOcrEngine {
    model: String,
    threads: usize,
}

impl RapidOcrEngine {
    pub fn new(model: String, threads: usize) -> Arc<Self> {
        let model = if model.is_empty() {
            DEFAULT_RAPIDOCR_MODEL.to_string()
        } else {
            model
        };
        *GLOBAL_CONFIG.lock() = (model.clone(), threads);
        Arc::new(Self { model, threads })
    }

    pub fn configure_global(model: &str, threads: usize) {
        let model = if model.is_empty() {
            DEFAULT_RAPIDOCR_MODEL
        } else {
            model
        };
        *GLOBAL_CONFIG.lock() = (model.to_string(), threads);
    }

    pub fn current_config() -> (String, usize) {
        let g = GLOBAL_CONFIG.lock();
        if g.0.is_empty() {
            (DEFAULT_RAPIDOCR_MODEL.to_string(), 0)
        } else {
            g.clone()
        }
    }

    fn model_dir() -> PathBuf {
        let dir = paths::cache_dir().join("rapidocr");
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    /// 查找本地已有完整且校验通过的缓存模型集（供网络故障或未下载时降级回退）
    fn find_available_cached_model(
        cache: &ModelCache,
        pipeline: PipelineConfig,
    ) -> Option<&'static ModelSetSpec> {
        let candidates = [
            "ppocrv5-ch-mobile",
            "ppocrv5-ch-server",
            "ppocrv6-small",
            "ppocrv6-medium",
            "ppocrv5-en-mobile",
        ];
        for name in candidates {
            if let Some(spec) = model_set_by_name(name) {
                let assets = spec.assets_for_pipeline(pipeline);
                if assets
                    .iter()
                    .all(|a| is_asset_present(&cache.asset_path(*a)))
                {
                    return Some(spec);
                }
            }
        }
        None
    }

    fn ensure_runner_for<'a>(
        slot: &'a mut Option<ActiveRunner>,
        target_model: &str,
        target_threads: usize,
        allow_download: bool,
    ) -> anyhow::Result<&'a mut RapidOcr> {
        let target_model = if target_model.is_empty() {
            DEFAULT_RAPIDOCR_MODEL
        } else {
            target_model
        };
        let resolved_threads = resolve_threads(target_threads);
        let need_reload = match slot {
            Some(active) => active.model != target_model || active.threads != target_threads,
            None => true,
        };

        if need_reload {
            // 必须先于任何 ort / RapidOCR session 调用
            ort_runtime::ensure_loaded()?;
            let target_spec = model_set_by_name(target_model)
                .or_else(|| model_set_by_name(DEFAULT_RAPIDOCR_MODEL))
                .ok_or_else(|| anyhow::anyhow!("未知 RapidOCR 模型集: {target_model}"))?;
            let cache = ModelCache::new(Self::model_dir());
            let pipeline = PipelineConfig::without_cls(); // 截图一般正向，省一次分类

            // 检查目标模型是否缺少完整本地资产
            let missing: Vec<_> = target_spec
                .assets_for_pipeline(pipeline)
                .into_iter()
                .filter(|a| !is_asset_present(&cache.asset_path(*a)))
                .collect();

            let (active_spec, actual_model_name) = if missing.is_empty() {
                // 目标模型已完整缓存在本地，直接加载
                (target_spec, target_model.to_string())
            } else if !allow_download {
                // 当前操作不允许自动网络下载（如启动预热或普通截图识别），避免网络卡顿
                log::info!(
                    "RapidOCR 目标模型 ({}) 尚未下载，启动与普通识别不自动触发网络下载（请在设置中下载）",
                    target_spec.name
                );
                if let Some(fallback_spec) = Self::find_available_cached_model(&cache, pipeline) {
                    log::info!(
                        "暂使用本地已有完整模型 ({})",
                        fallback_spec.name
                    );
                    (fallback_spec, fallback_spec.name.to_string())
                } else {
                    anyhow::bail!(
                        "RapidOCR 模型 ({}) 尚未下载，且本地无可用备用模型，请在设置中点击下载",
                        target_spec.name
                    );
                }
            } else {
                // allow_download == true（用户在设置中显式点击下载）
                log::info!(
                    "准备下载 RapidOCR 模型 ({}) 缺失资产 ({} 个文件) → {}",
                    target_spec.name,
                    missing.len(),
                    cache.root().display()
                );
                for (i, asset) in missing.iter().enumerate() {
                    let dest = cache.asset_path(*asset);
                    download_asset_file(asset, &dest, i + 1, missing.len(), target_model)?;
                }
                cache.ensure_model_set_for_pipeline(
                    target_spec,
                    pipeline,
                    ModelDownloadMode::Missing,
                )?;
                (target_spec, target_model.to_string())
            };

            let mut cfg = cache.config_for(active_spec).with_pipeline(pipeline);
            cfg.inference.intra_threads = resolved_threads;
            cfg.inference.enable_cpu_mem_arena = true;
            let ocr = match RapidOcr::from_config(cfg) {
                Ok(ocr) => ocr,
                Err(e) => {
                    log::error!(
                        "RapidOCR 引擎加载失败 ({e:#})，检测到模型可能损坏，尝试清理并降级"
                    );
                    // 清理损坏的资产文件
                    for asset in target_spec.assets_for_pipeline(pipeline) {
                        let path = cache.asset_path(asset);
                        if !is_asset_verified(&path, &asset) {
                            log::warn!("清理损坏的资产文件: {}", path.display());
                            let _ = std::fs::remove_file(&path);
                        }
                    }
                    if let Some(fallback_spec) = Self::find_available_cached_model(&cache, pipeline)
                        && fallback_spec.name != target_spec.name
                    {
                        log::warn!(
                            "降级使用本地已有完整模型 ({})",
                            fallback_spec.name
                        );
                        let mut fallback_cfg =
                            cache.config_for(fallback_spec).with_pipeline(pipeline);
                        fallback_cfg.inference.intra_threads = resolved_threads;
                        fallback_cfg.inference.enable_cpu_mem_arena = true;
                        let fallback_ocr = RapidOcr::from_config(fallback_cfg)?;
                        *slot = Some(ActiveRunner {
                            model: fallback_spec.name.to_string(),
                            threads: target_threads,
                            ocr: fallback_ocr,
                        });
                        return Ok(&mut slot.as_mut().unwrap().ocr);
                    }
                    return Err(e);
                }
            };
            *slot = Some(ActiveRunner {
                model: actual_model_name,
                threads: target_threads,
                ocr,
            });
            log::info!(
                "RapidOCR 引擎已加载 (model: {}, threads: {resolved_threads})",
                active_spec.name
            );
        }
        Ok(&mut slot.as_mut().unwrap().ocr)
    }

    /// 返回带检测框的行（像素坐标，原点左上）。供原图翻译 overlay 使用。
    pub fn recognize_lines(image: &Path) -> anyhow::Result<Vec<DetectedLine>> {
        let (model, threads) = Self::current_config();
        Self::recognize_lines_with_config(image, &model, threads)
    }

    pub fn recognize_lines_with_config(
        image: &Path,
        model: &str,
        threads: usize,
    ) -> anyhow::Result<Vec<DetectedLine>> {
        let mut g = RUNNER.lock();
        let ocr = Self::ensure_runner_for(&mut g, model, threads, false)?;
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

    fn warmup(&self) {}

    fn recognize(&self, image: &Path, _language_hint: Option<&str>) -> anyhow::Result<String> {
        let mut lines = Self::recognize_lines_with_config(image, &self.model, self.threads)?;
        lines.sort_by(|a, b| {
            let ay = centroid_y(&a.points);
            let by = centroid_y(&b.points);
            ay.partial_cmp(&by)
                .unwrap_or(Ordering::Equal)
                .then_with(|| {
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
            if let Some((ref_y, row)) = rows.last_mut()
                && (cy - *ref_y).abs() < h * 0.6
            {
                row.push(text.to_string());
                continue;
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
    let max_y = points
        .iter()
        .map(|p| p[1])
        .fold(f32::NEG_INFINITY, f32::max);
    max_y - min_y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_models_are_valid() {
        for (id, _) in RAPIDOCR_MODELS {
            assert!(
                model_set_by_name(id).is_some(),
                "RapidOCR 模型集 {id} 未在 rapidocr_core 中注册"
            );
        }
    }

    #[test]
    fn resolve_threads_clamping() {
        assert!((1..=8).contains(&resolve_threads(0)));
        assert_eq!(resolve_threads(1), 1);
        assert_eq!(resolve_threads(16), 16);
        assert_eq!(resolve_threads(64), 32);
    }

    #[test]
    fn global_config_update() {
        let _engine = RapidOcrEngine::new("ppocrv6-medium".into(), 4);
        assert_eq!(
            RapidOcrEngine::current_config(),
            ("ppocrv6-medium".to_string(), 4)
        );
    }

    #[test]
    fn model_status_and_download_flags() {
        assert!(check_model_status("ppocrv5-ch-mobile").is_some());
        assert!(check_model_status("nonexistent_model").is_none());

        assert!(!is_downloading_model("ppocrv6-medium"));
        set_downloading_model(Some("ppocrv6-medium".into()));
        assert!(is_downloading_model("ppocrv6-medium"));
        assert!(!is_downloading_model("ppocrv5-ch-mobile"));
        set_downloading_model(None);
        assert!(!is_downloading_model("ppocrv6-medium"));
    }
}
