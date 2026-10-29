//! 跨平台配置/缓存目录解析。
//!
//! 平台约定：
//! - macOS
//!   - `~/Library/Application Support/dev.transel/`
//!   - `~/Library/Caches/dev.transel/`
//! - Linux
//!   - `$XDG_CONFIG_HOME/dev.transel/`
//!   - `$XDG_CACHE_HOME/dev.transel/`
//! - Windows
//!   - `%APPDATA%\dev.transel\`（`config`）
//!   - `%LOCALAPPDATA%\dev.transel\`（`cache`）

use std::path::{Path, PathBuf};

pub const APP_DIR: &str = "dev.transel";

/// 返回配置目录（不存在则创建）。
pub fn config_dir() -> PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join(APP_DIR);
    ensure(&dir);
    dir
}

/// 返回缓存目录（不存在则创建）。
pub fn cache_dir() -> PathBuf {
    let dir = dirs::cache_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join(APP_DIR);
    ensure(&dir);
    dir
}

/// 配置文件路径：`config.json`。
pub fn config_file() -> PathBuf {
    config_dir().join("config.json")
}

/// 翻译历史文件：`history.jsonl`。
pub fn history_file() -> PathBuf {
    cache_dir().join("history.jsonl")
}

/// 收藏夹文件：`favorites.jsonl`。
pub fn favorites_file() -> PathBuf {
    cache_dir().join("favorites.jsonl")
}

/// 单词词典磁盘缓存目录。
pub fn dict_cache_dir() -> PathBuf {
    let dir = cache_dir().join("dict");
    ensure(&dir);
    dir
}

/// 临时 OCR 缓存目录。
pub fn ocr_cache_dir() -> PathBuf {
    let dir = cache_dir().join("ocr");
    ensure(&dir);
    dir
}

/// 清理临时 OCR 缓存中的历史残余文件
pub fn cleanup_ocr_cache() {
    let dir = cache_dir().join("ocr");
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if let Some(name) = p.file_name().and_then(|n| n.to_str())
                && name.starts_with("st-")
                && name.ends_with(".png")
            {
                let _ = std::fs::remove_file(p);
            }
        }
    }
}

/// 图标/资源目录（运行时只读；打包时随安装包发布）。
pub fn resource_dir() -> Option<PathBuf> {
    // 优先 `$ST_RESOURCE_DIR`，其次 `assets/`，最后可执行文件旁边的 `assets/`。
    if let Some(p) = std::env::var_os("ST_RESOURCE_DIR") {
        let p = PathBuf::from(p);
        if p.is_dir() {
            return Some(p);
        }
    }
    let candidates = [
        PathBuf::from("assets"),
        std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(Path::to_path_buf))
            .map(|p| p.join("assets"))
            .unwrap_or_default(),
    ];
    candidates.into_iter().find(|p| p.is_dir())
}

fn ensure(p: &Path) {
    if let Err(e) = std::fs::create_dir_all(p) {
        log::warn!("创建目录 {} 失败: {e}", p.display());
    }
}
