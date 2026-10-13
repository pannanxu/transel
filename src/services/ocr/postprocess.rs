//! OCR 结果后处理：智能分段、二维码优先展示等。

/// 智能分段：把 OCR 断行还原为段落。
///
/// 规则（启发式）：
/// - 空行分隔段落
/// - 行尾是中英文句末标点则另起一段
/// - 否则把下一行用空格（拉丁）或直接拼接（CJK）接到当前行
pub fn smart_paragraph(raw: &str) -> String {
    let lines: Vec<&str> = raw.lines().map(|l| l.trim_end()).collect();
    if lines.is_empty() {
        return String::new();
    }
    let mut paras: Vec<String> = Vec::new();
    let mut cur = String::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !cur.is_empty() {
                paras.push(std::mem::take(&mut cur));
            }
            continue;
        }
        if cur.is_empty() {
            cur.push_str(trimmed);
            continue;
        }
        if ends_sentence(&cur) {
            paras.push(std::mem::take(&mut cur));
            cur.push_str(trimmed);
        } else if is_cjk_heavy(&cur) || is_cjk_heavy(trimmed) {
            cur.push_str(trimmed);
        } else {
            cur.push(' ');
            cur.push_str(trimmed);
        }
    }
    if !cur.is_empty() {
        paras.push(cur);
    }
    paras.join("\n\n")
}

fn ends_sentence(s: &str) -> bool {
    matches!(
        s.chars().rev().find(|c| !c.is_whitespace()),
        Some(
            '。' | '！'
                | '？'
                | '；'
                | '!'
                | '?'
                | ';'
                | '.'
                | '…'
                | '」'
                | '』'
                | '"'
                | '\''
                | '”'
                | '’'
        )
    )
}

fn is_cjk_heavy(s: &str) -> bool {
    let mut cjk = 0usize;
    let mut other = 0usize;
    for c in s.chars() {
        if c.is_whitespace() || c.is_ascii_punctuation() {
            continue;
        }
        if ('\u{4E00}'..='\u{9FFF}').contains(&c)
            || ('\u{3400}'..='\u{4DBF}').contains(&c)
            || ('\u{F900}'..='\u{FAFF}').contains(&c)
            || ('\u{3040}'..='\u{30FF}').contains(&c)
            || ('\u{AC00}'..='\u{D7AF}').contains(&c)
        {
            cjk += 1;
        } else {
            other += 1;
        }
    }
    cjk > 0 && cjk >= other
}

/// 把二维码内容与正文合并展示：有二维码时放在最前。
pub fn merge_with_qr(text: &str, qr: &[String]) -> String {
    if qr.is_empty() {
        return text.to_string();
    }
    let mut parts: Vec<String> = Vec::new();
    for (i, q) in qr.iter().enumerate() {
        let label = if qr.len() == 1 {
            "二维码".to_string()
        } else {
            format!("二维码 {}", i + 1)
        };
        parts.push(format!("[{label}]\n{q}"));
    }
    let body = text.trim();
    if !body.is_empty() {
        parts.push(body.to_string());
    }
    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_cjk_broken_lines() {
        let raw = "这是第一行\n这是第二行。\n\n新段落";
        let out = smart_paragraph(raw);
        assert!(out.contains("这是第一行这是第二行。"));
        assert!(out.contains("新段落"));
    }

    #[test]
    fn joins_latin_with_space() {
        let raw = "Hello\nworld.";
        assert_eq!(smart_paragraph(raw), "Hello world.");
    }
}
