//! 单词词典查询：使用 Free Dictionary API。
//!
//! - 命中条件：输入只含英文字母 + 空格 + 连字符，长度 < 50，**不**经过标识符拆词。
//! - 失败不报错，返回 `None`（UI 退化为普通翻译）。
//! - 缓存：仅本地磁盘 JSON 缓存（无内存缓存）。

use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::app::paths;
use crate::services::translate::ureq_agent;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Phonetic {
    pub text: Option<String>,
    pub audio: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meaning {
    #[serde(rename = "partOfSpeech")]
    pub part_of_speech: String,
    pub definitions: Vec<Definition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Definition {
    pub definition: String,
    #[serde(default)]
    pub example: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictionaryEntry {
    pub word: String,
    #[serde(default)]
    pub phonetic: Option<String>,
    #[serde(default)]
    pub phonetics: Vec<Phonetic>,
    #[serde(default)]
    pub meanings: Vec<Meaning>,
}

#[derive(Debug, Default, Clone)]
pub struct Dictionary;

impl Dictionary {
    pub fn new() -> Self {
        Self
    }

    /// 单 token 检测：是否可作为英语单词查询。
    pub fn is_word(text: &str) -> bool {
        let t = text.trim();
        if t.is_empty() || t.len() > 50 {
            return false;
        }
        let t = t.replace(['-', ' ', '\t'], "");
        if t.is_empty() {
            return false;
        }
        t.chars().all(|c| c.is_ascii_alphabetic())
    }

    pub fn lookup(&self, word: &str) -> Result<Arc<DictionaryEntry>> {
        let key = word.trim().to_ascii_lowercase();
        let cache_file =
            paths::dict_cache_dir().join(format!("{:x}.json", md5::compute(key.as_bytes())));

        if let Ok(content) = std::fs::read_to_string(&cache_file)
            && let Ok(entry) = serde_json::from_str::<DictionaryEntry>(&content)
        {
            return Ok(Arc::new(entry));
        }

        let url = format!(
            "https://api.dictionaryapi.dev/api/v2/entries/en/{}",
            urlencoding::encode(&key)
        );
        let v: serde_json::Value = ureq_agent()
            .get(&url)
            .call()
            .context("请求词典 API 失败")?
            .body_mut()
            .read_json()
            .context("解析词典响应失败")?;
        let arr = v
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("词典响应不是数组"))?;
        if arr.is_empty() {
            anyhow::bail!("无词条");
        }
        let entry: DictionaryEntry = serde_json::from_value(arr[0].clone())?;
        if let Ok(json_str) = serde_json::to_string(&entry) {
            let _ = std::fs::write(&cache_file, json_str);
        }
        Ok(Arc::new(entry))
    }

    /// 综合读音（优先 phonetic 字段，其次 phonetics 数组里第一个 text 非空）。
    pub fn phonetic(entry: &DictionaryEntry) -> Option<String> {
        if let Some(p) = &entry.phonetic
            && !p.is_empty()
        {
            return Some(p.clone());
        }
        entry
            .phonetics
            .iter()
            .filter_map(|p| p.text.clone())
            .find(|s| !s.is_empty())
    }
}
