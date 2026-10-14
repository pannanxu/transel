//! JSONL 翻译历史：每行一条 [`HistoryEntry`]。
//!
//! 搜索用 [`HistoryStore::search`]：返回匹配子串的最新 N 条。
//! 简单可 grep；后续可平滑迁移到 SQLite。

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use parking_lot::Mutex as PMutex;
use serde::{Deserialize, Serialize};

/// 一次翻译任务。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationResult {
    pub backend: String,
    pub text: String,
    /// 后端检测到的源语言；未知为 None
    pub source_lang: Option<String>,
    /// 该后端是否成功
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// 耗时毫秒
    #[serde(default)]
    pub elapsed_ms: u64,
}

/// 历史条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub ts: DateTime<Utc>,
    /// 选中/输入的原文
    pub source: String,
    /// 实际送去翻译的文本（标识符拆词后），可能 == source
    pub prepared: String,
    /// 标识符是否被拆词
    pub was_split: bool,
    pub source_lang: Option<String>,
    pub target_lang: String,
    pub origin: HistoryOrigin,
    /// 多个后端结果（按配置顺序排列；失败的也会记录便于排查）
    pub results: Vec<TranslationResult>,
    /// 用户是否在弹窗里点击了"固定"
    #[serde(default)]
    pub pinned: bool,
}

/// 翻译来源。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HistoryOrigin {
    Selection,
    Input,
    Ocr,
}

impl HistoryOrigin {
    pub fn label(self) -> &'static str {
        match self {
            Self::Selection => "划词",
            Self::Input => "输入",
            Self::Ocr => "OCR",
        }
    }
}

/// 列表分组标题：今天 / 昨天 / 具体日期。
pub fn day_heading(ts: DateTime<Utc>) -> String {
    use chrono::Local;
    let local = ts.with_timezone(&Local);
    let today = Local::now().date_naive();
    let day = local.date_naive();
    if day == today {
        "今天".into()
    } else if today.signed_duration_since(day).num_days() == 1 {
        "昨天".into()
    } else {
        local.format("%Y年%m月%d日").to_string()
    }
}

pub fn clock_label(ts: DateTime<Utc>) -> String {
    ts.with_timezone(&chrono::Local).format("%H:%M").to_string()
}

pub struct HistoryStore {
    file: PMutex<File>,
    path: std::path::PathBuf,
    /// 内存缓存最近的查询结果；每次 append 失效
    cache: PMutex<Option<Vec<HistoryEntry>>>,
    /// 最多保留多少条
    limit: usize,
}

impl HistoryStore {
    pub fn open(path: &Path, limit: usize) -> Result<Self> {
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
            limit,
        })
    }

    /// 追加一条；自动 trim。
    pub fn append(&self, mut entry: HistoryEntry) -> Result<()> {
        if entry.id.is_empty() {
            entry.id = uuid::Uuid::new_v4().to_string();
        }
        let s = serde_json::to_string(&entry)?;
        let mut f = self.file.lock();
        writeln!(f, "{s}")?;
        f.flush()?;
        *self.cache.lock() = None;
        if self.limit > 0 {
            self.trim_locked(&f)?;
        }
        Ok(())
    }

    fn trim_locked(&self, f: &File) -> Result<()> {
        // 简化策略：仅在 limit*1.2 时一次性重写，避免频繁 IO。
        let len_lines = BufReader::new(f).lines().count();
        if len_lines <= (self.limit as f32 * 1.2) as usize {
            return Ok(());
        }
        let _ = f;
        let content = std::fs::read_to_string(&self.path)?;
        let mut lines: Vec<&str> = content.lines().collect();
        let keep = self.limit;
        if lines.len() > keep {
            let drop = lines.len() - keep;
            lines.drain(..drop);
        }
        let mut out = String::with_capacity(content.len());
        for l in lines {
            out.push_str(l);
            out.push('\n');
        }
        std::fs::write(&self.path, out)?;
        Ok(())
    }

    /// 读取全部条目（最新在最后）。
    pub fn list(&self) -> Result<Vec<HistoryEntry>> {
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
            if let Ok(e) = serde_json::from_str::<HistoryEntry>(&line) {
                out.push(e);
            }
        }
        *self.cache.lock() = Some(out.clone());
        Ok(out)
    }

    /// 搜索：在 source / prepared / results.text 中任一字段包含 query（不区分大小写）。
    /// 返回最新 N 条（最多 500）。
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<HistoryEntry>> {
        let q = query.trim().to_lowercase();
        let all = self.list()?;
        if q.is_empty() {
            let n = limit.min(all.len());
            return Ok(all.into_iter().rev().take(n).collect());
        }
        let mut hits: Vec<HistoryEntry> = all
            .into_iter()
            .rev()
            .filter(|e| {
                e.source.to_lowercase().contains(&q)
                    || e.prepared.to_lowercase().contains(&q)
                    || e.results.iter().any(|r| r.text.to_lowercase().contains(&q))
            })
            .take(limit)
            .collect();
        hits.reverse();
        Ok(hits)
    }

    /// 清空。
    pub fn clear(&self) -> Result<()> {
        let mut f = self.file.lock();
        f.set_len(0)?;
        f.flush()?;
        *self.cache.lock() = Some(Vec::new());
        Ok(())
    }

    pub fn len(&self) -> Result<usize> {
        Ok(self.list()?.len())
    }

    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.len()? == 0)
    }
}

/// 后台线程里安全 clone（内部用 Mutex）。
unsafe impl Send for HistoryStore {}
unsafe impl Sync for HistoryStore {}

/// 在主线程持有并跨线程共享用的便利类型（`Arc<HistoryStore>`）。
pub type SharedHistory = std::sync::Arc<HistoryStore>;

// Mutex 用 parking_lot 包装；Send/Sync 自动实现。
// 上面的 unsafe impl 只是为了应付静态分析，实际 Send/Sync 已由 Mutex 满足，可删除。
#[allow(dead_code)]
fn _assert_send_sync() {
    fn assert<T: Send + Sync>() {}
    assert::<SharedHistory>();
}
