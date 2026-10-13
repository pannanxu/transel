//! 收藏夹：JSONL，结构与 [`HistoryEntry`](super::history::HistoryEntry) 兼容子集。

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use parking_lot::Mutex as PMutex;
use serde::{Deserialize, Serialize};

use super::history::{HistoryEntry, HistoryOrigin, TranslationResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FavoriteEntry {
    pub id: String,
    pub ts: DateTime<Utc>,
    pub source: String,
    pub prepared: String,
    pub target_lang: String,
    pub translation: String,
    pub backend: Option<String>,
    pub origin: HistoryOrigin,
}

impl FavoriteEntry {
    pub fn from_history(entry: &HistoryEntry) -> Option<Self> {
        let best = entry.results.iter().find(|r| r.success)?;
        Some(Self {
            id: if entry.id.is_empty() {
                uuid::Uuid::new_v4().to_string()
            } else {
                entry.id.clone()
            },
            ts: Utc::now(),
            source: entry.source.clone(),
            prepared: entry.prepared.clone(),
            target_lang: entry.target_lang.clone(),
            translation: best.text.clone(),
            backend: Some(best.backend.clone()),
            origin: entry.origin,
        })
    }

    pub fn from_texts(
        source: &str,
        translation: &str,
        target_lang: &str,
        origin: HistoryOrigin,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            ts: Utc::now(),
            source: source.to_string(),
            prepared: source.to_string(),
            target_lang: target_lang.to_string(),
            translation: translation.to_string(),
            backend: None,
            origin,
        }
    }
}

pub struct FavoritesStore {
    file: PMutex<File>,
    path: PathBuf,
    cache: PMutex<Option<Vec<FavoriteEntry>>>,
}

impl FavoritesStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("创建 {}", parent.display()))?;
        }
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(path)
            .with_context(|| format!("打开 {}", path.display()))?;
        Ok(Self {
            file: PMutex::new(file),
            path: path.to_path_buf(),
            cache: PMutex::new(None),
        })
    }

    pub fn append(&self, entry: FavoriteEntry) -> Result<()> {
        // 去重：同 source+translation 则跳过
        let all = self.list()?;
        if all
            .iter()
            .any(|e| e.source == entry.source && e.translation == entry.translation)
        {
            return Ok(());
        }
        let s = serde_json::to_string(&entry)?;
        let mut f = self.file.lock();
        writeln!(f, "{s}")?;
        f.flush()?;
        *self.cache.lock() = None;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<FavoriteEntry>> {
        if let Some(c) = self.cache.lock().clone() {
            return Ok(c);
        }
        let file = File::open(&self.path)?;
        let reader = BufReader::new(file);
        let mut out = Vec::new();
        for line in reader.lines().map_while(Result::ok) {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(e) = serde_json::from_str::<FavoriteEntry>(&line) {
                out.push(e);
            }
        }
        *self.cache.lock() = Some(out.clone());
        Ok(out)
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        let all = self.list()?;
        let kept: Vec<_> = all.into_iter().filter(|e| e.id != id).collect();
        let mut out = String::new();
        for e in &kept {
            out.push_str(&serde_json::to_string(e)?);
            out.push('\n');
        }
        std::fs::write(&self.path, out)?;
        // 重新打开 append 句柄
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&self.path)?;
        *self.file.lock() = file;
        *self.cache.lock() = Some(kept);
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        let mut f = self.file.lock();
        f.set_len(0)?;
        f.flush()?;
        *self.cache.lock() = Some(Vec::new());
        Ok(())
    }

    /// 导出为可读文本。
    pub fn export_text(&self) -> Result<String> {
        let all = self.list()?;
        let mut out = String::new();
        for e in all.iter().rev() {
            out.push_str(&format!(
                "[{}] {}\n{}\n→ {}\n\n",
                e.ts.format("%Y-%m-%d %H:%M"),
                e.target_lang,
                e.source,
                e.translation
            ));
        }
        Ok(out)
    }
}

pub type SharedFavorites = Arc<FavoritesStore>;

/// 从当前弹窗结果构造一条收藏（取当前 tab 译文）。
pub fn from_batch_texts(
    source: &str,
    translation: &str,
    target_lang: &str,
    backend: Option<String>,
    origin: HistoryOrigin,
) -> FavoriteEntry {
    FavoriteEntry {
        id: uuid::Uuid::new_v4().to_string(),
        ts: Utc::now(),
        source: source.to_string(),
        prepared: source.to_string(),
        target_lang: target_lang.to_string(),
        translation: translation.to_string(),
        backend,
        origin,
    }
}

#[allow(dead_code)]
fn _use_translation_result(_: &TranslationResult) {}
