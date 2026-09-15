// CI lint baseline: pre-existing Clippy debt; remove allowances incrementally.
#![allow(clippy::empty_line_after_doc_comments, clippy::explicit_counter_loop)]

/// Unicode-safe text truncation utilities.
///
/// All string slicing in AIKS must use these functions instead of `&text[..N]`
/// to avoid panics on multi-byte characters (CJK, Emoji, etc.).

/// Truncate to at most `max_chars` Unicode scalar values.
/// Returns a string that is safe to display and pass to AI models.
pub fn truncate_chars(s: &str, max_chars: usize) -> &str {
    if s.chars().count() <= max_chars {
        return s;
    }
    // Find the byte offset of the `max_chars`-th char boundary
    let mut char_count = 0;
    let mut byte_end = 0;
    for (byte_pos, _) in s.char_indices() {
        if char_count == max_chars {
            byte_end = byte_pos;
            break;
        }
        char_count += 1;
    }
    &s[..byte_end]
}

/// Truncate to at most `max_bytes` UTF-8 bytes, ensuring a valid boundary.
pub fn truncate_utf8_bytes(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    // Walk back from max_bytes until we find a valid char boundary
    let mut end = max_bytes;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Truncate a long string, keeping a head and tail with an ellipsis in the middle.
/// `head_chars` + `tail_chars` chars maximum in the output.
pub fn truncate_middle(s: &str, head_chars: usize, tail_chars: usize) -> String {
    let total_chars = head_chars + tail_chars;
    if s.chars().count() <= total_chars {
        return s.to_string();
    }
    let head = truncate_chars(s, head_chars);
    // Get tail: collect from (len - tail_chars) chars from the end
    let all_chars: Vec<char> = s.chars().collect();
    let tail_start = all_chars.len().saturating_sub(tail_chars);
    let tail: String = all_chars[tail_start..].iter().collect();
    format!("{}\n...[中间内容已截断]...\n{}", head, tail)
}

/// Safely trim a string to a maximum number of chars for logging/display.
pub fn safe_preview(s: &str, max_chars: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        format!("{}…", truncate_chars(s, max_chars))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_chars_ascii() {
        assert_eq!(truncate_chars("hello world", 5), "hello");
        assert_eq!(truncate_chars("hi", 10), "hi");
    }

    #[test]
    fn truncate_chars_cjk() {
        let s = "中文字符测试内容";
        let result = truncate_chars(s, 4);
        assert_eq!(result, "中文字符");
        // Valid UTF-8
        assert!(std::str::from_utf8(result.as_bytes()).is_ok());
    }

    #[test]
    fn truncate_chars_emoji() {
        let s = "🔥🎉🦀🚀✨";
        let result = truncate_chars(s, 3);
        assert_eq!(result.chars().count(), 3);
        assert!(std::str::from_utf8(result.as_bytes()).is_ok());
    }

    #[test]
    fn truncate_chars_mixed() {
        let s = "hello中文🔥world";
        let result = truncate_chars(s, 8);
        assert_eq!(result.chars().count(), 8);
        assert!(std::str::from_utf8(result.as_bytes()).is_ok());
    }

    #[test]
    fn truncate_utf8_bytes_safe() {
        // 中 is 3 bytes; truncating at byte 4 would be invalid without this function
        let s = "中文字";
        let result = truncate_utf8_bytes(s, 4);
        assert!(std::str::from_utf8(result.as_bytes()).is_ok());
        assert_eq!(result, "中"); // 3 bytes fit; 4th byte invalid → truncate to 3
    }

    #[test]
    fn truncate_middle_cjk() {
        let s = "中".repeat(1000);
        let result = truncate_middle(&s, 100, 100);
        assert!(result.contains("已截断"));
        assert!(std::str::from_utf8(result.as_bytes()).is_ok());
    }

    #[test]
    fn no_panic_at_exact_boundary() {
        let s = "中".repeat(4000);
        // These should never panic
        let _ = truncate_chars(&s, 3999);
        let _ = truncate_chars(&s, 4000);
        let _ = truncate_chars(&s, 4001);
        let _ = truncate_utf8_bytes(&s, 9999);
        let _ = truncate_utf8_bytes(&s, 10000);
        let _ = truncate_utf8_bytes(&s, 10001);
    }
}
