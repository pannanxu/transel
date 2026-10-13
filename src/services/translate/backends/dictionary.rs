//! 英语词典通道：Free Dictionary API，与其它翻译后端一样走 [`Translator`]。
//!
//! 非单词输入时返回错误（通道卡片显示失败），不切换独立 UI 模式。

use std::sync::Arc;

use anyhow::{Result, anyhow};

use crate::services::dictionary::{Dictionary, DictionaryEntry};
use crate::services::translate::{SingleResult, Translator};

pub struct DictionaryTranslator {
    dict: Arc<Dictionary>,
}

impl DictionaryTranslator {
    pub fn new() -> Self {
        Self {
            dict: Arc::new(Dictionary::new()),
        }
    }

    pub fn with_dictionary(dict: Arc<Dictionary>) -> Self {
        Self { dict }
    }

    fn format_entry(entry: &DictionaryEntry) -> String {
        let mut out = String::new();
        if let Some(p) = Dictionary::phonetic(entry) {
            out.push('/');
            out.push_str(&p);
            out.push_str("/\n\n");
        }
        for m in &entry.meanings {
            out.push_str("**");
            out.push_str(&m.part_of_speech);
            out.push_str("**\n");
            for (i, d) in m.definitions.iter().take(3).enumerate() {
                out.push_str(&format!("{}. {}\n", i + 1, d.definition));
                if let Some(ex) = &d.example {
                    out.push_str(&format!("   例: {ex}\n"));
                }
            }
            out.push('\n');
        }
        out.trim().to_string()
    }
}

impl Default for DictionaryTranslator {
    fn default() -> Self {
        Self::new()
    }
}

impl Translator for DictionaryTranslator {
    fn name(&self) -> &str {
        "词典"
    }

    fn translate_blocking(
        &self,
        text: &str,
        _target: &str,
        _source_hint: Option<&str>,
    ) -> Result<SingleResult> {
        if !Dictionary::is_word(text) {
            return Err(anyhow!("仅支持英语单词查询"));
        }
        let entry = self.dict.lookup(text)?;
        let formatted = Self::format_entry(&entry);
        if formatted.is_empty() {
            return Err(anyhow!("无词条"));
        }
        Ok(SingleResult {
            backend: self.name().to_string(),
            text: formatted,
            source_lang: Some("en".into()),
            elapsed_ms: 0,
            ok: true,
            error: None,
            pending: false,
        })
    }
}
