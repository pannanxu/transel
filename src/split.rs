//! 标识符拆词：把 `getUserHTTPResponse_code-value` 这类代码标识符拆成
//! `get user HTTP response code value`，再交给翻译。普通文本原样返回。
//!
//! 规则：
//! - 只处理“单个 token”：不含空白，只由 ASCII 字母、数字和 `_ - . $` 组成，至少含一个字母，
//!   且确实带有分隔符或大小写切换（否则 `hello`、`HTTP` 原样返回）。
//! - 分隔符 `_ - . $` 切分；驼峰在 小写→大写 处切分；连续大写后接小写时，
//!   最后一个大写归下一个词（`HTTPServer` → `HTTP Server`）。
//! - 数字跟随前面的词（`utf8Decoder` → `utf8 decoder`、`base64` 不拆）。
//! - 输出小写，长度 ≥ 2 的全大写词视为缩写保持大写；
//!   但若整个标识符都是全大写（SCREAMING_SNAKE），则全部转小写。

/// 若 `text` 像标识符则返回拆分后的短语，否则 `None`。
pub fn split_identifier(text: &str) -> Option<String> {
    let t = text.trim();
    if t.is_empty() || t.len() > 200 {
        return None;
    }
    if !t
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '$'))
    {
        return None;
    }
    if !t.chars().any(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    // 以点号结尾的缩写/句子（"e.g."）不算
    if t.ends_with('.') {
        return None;
    }

    let words = raw_words(t);
    if words.len() < 2 {
        return None;
    }

    let all_upper = words.iter().all(|w| is_upper_word(w));
    let out: Vec<String> = words
        .iter()
        .map(|w| {
            if !all_upper
                && is_upper_word(w)
                && w.chars().filter(|c| c.is_ascii_alphabetic()).count() >= 2
            {
                w.clone()
            } else {
                w.to_ascii_lowercase()
            }
        })
        .collect();
    Some(out.join(" "))
}

/// 返回 (用于翻译的文本, 是否发生了拆分)。
pub fn prepare(text: &str) -> (String, bool) {
    match split_identifier(text) {
        Some(s) => (s, true),
        None => (text.trim().to_string(), false),
    }
}

fn is_upper_word(w: &str) -> bool {
    w.chars().any(|c| c.is_ascii_uppercase()) && !w.chars().any(|c| c.is_ascii_lowercase())
}

fn raw_words(t: &str) -> Vec<String> {
    let mut words = Vec::new();
    for part in t.split(['_', '-', '.', '$']).filter(|p| !p.is_empty()) {
        split_camel(part, &mut words);
    }
    words
}

fn split_camel(part: &str, out: &mut Vec<String>) {
    let chars: Vec<char> = part.chars().collect();
    let mut cur = String::new();
    for i in 0..chars.len() {
        let c = chars[i];
        if i > 0 && !cur.is_empty() {
            let prev = chars[i - 1];
            let next = chars.get(i + 1).copied();
            let boundary = c.is_ascii_uppercase()
                && (prev.is_ascii_lowercase()
                    || prev.is_ascii_digit()
                    || (prev.is_ascii_uppercase() && next.is_some_and(|n| n.is_ascii_lowercase())));
            if boundary {
                out.push(std::mem::take(&mut cur));
            }
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
}

#[cfg(test)]
mod tests {
    use super::split_identifier as s;

    fn ok(input: &str, expected: &str) {
        assert_eq!(s(input).as_deref(), Some(expected), "input: {input}");
    }

    #[test]
    fn snake_and_screaming() {
        ok("user_name", "user name");
        ok("max_retry_count", "max retry count");
        ok("MAX_RETRY_COUNT", "max retry count");
        ok("__private_field__", "private field");
        ok("HTTP_STATUS", "http status");
    }

    #[test]
    fn camel_and_pascal() {
        ok("getUserName", "get user name");
        ok("GetUserName", "get user name");
        ok("isOK", "is OK");
        ok("parseJSON", "parse JSON");
        ok("iPhone", "i phone");
    }

    #[test]
    fn acronyms() {
        ok("HTTPServer", "HTTP server");
        ok("XMLHttpRequest", "XML http request");
        ok("getHTTPResponseCode", "get HTTP response code");
        ok("IOError", "IO error");
        ok("userID", "user ID");
    }

    #[test]
    fn digits() {
        ok("utf8Decoder", "utf8 decoder");
        ok("base64_encode", "base64 encode");
        ok("Vec3D", "vec3 d");
        ok("404NotFound", "404 not found");
        ok("sha256Sum", "sha256 sum");
        ok("x86_64", "x86 64");
    }

    #[test]
    fn kebab_dot_and_mixed() {
        ok("user-profile-card", "user profile card");
        ok("app.config.maxSize", "app config max size");
        ok(
            "getUserHTTPResponse_code-value",
            "get user HTTP response code value",
        );
        ok("$scopeApply", "scope apply");
        ok("background-color", "background color");
    }

    #[test]
    fn prose_unchanged() {
        assert_eq!(s("hello"), None);
        assert_eq!(s("HTTP"), None);
        assert_eq!(s("Hello"), None);
        assert_eq!(s("Hello world"), None);
        assert_eq!(s("This is a sentence."), None);
        assert_eq!(s("你好世界"), None);
        assert_eq!(s("get_用户"), None);
        assert_eq!(s("e.g."), None);
        assert_eq!(s("2024"), None);
        assert_eq!(s("1.2.3"), None);
        assert_eq!(s(""), None);
        assert_eq!(s("   "), None);
        assert_eq!(s("foo(bar)"), None);
    }

    #[test]
    fn trims_whitespace() {
        ok("  snake_case\n", "snake case");
    }

    #[test]
    fn prepare_flags_split() {
        assert_eq!(super::prepare("fooBar"), ("foo bar".to_string(), true));
        assert_eq!(
            super::prepare(" plain text "),
            ("plain text".to_string(), false)
        );
    }
}
