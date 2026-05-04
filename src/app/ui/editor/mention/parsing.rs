#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedMention {
    pub char_range: std::ops::Range<usize>,
    pub id: u32,
    pub anchor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocMention {
    pub line_idx: usize,
    pub char_range: std::ops::Range<usize>,
    pub id: u32,
    pub anchor: Option<String>,
}

pub fn parse_mentions_in_line(line: &str) -> Vec<ParsedMention> {
    const PREFIX: &str = "@id:";
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '@' {
            i += 1;
            continue;
        }
        let prev_ok = i == 0 || chars[i - 1].is_whitespace();
        if !prev_ok {
            i += 1;
            continue;
        }
        let prefix_len = PREFIX.chars().count();
        if i + prefix_len > chars.len() {
            i += 1;
            continue;
        }
        let prefix_match = chars[i..i + prefix_len]
            .iter()
            .zip(PREFIX.chars())
            .all(|(a, b)| *a == b);
        if !prefix_match {
            i += 1;
            continue;
        }
        let digits_start = i + prefix_len;
        let mut j = digits_start;
        while j < chars.len() && chars[j].is_ascii_digit() {
            j += 1;
        }
        if j == digits_start {
            i += 1;
            continue;
        }
        let digit_str: String = chars[digits_start..j].iter().collect();
        let Ok(id) = digit_str.parse::<u32>() else {
            i = j;
            continue;
        };

        let (token_end, anchor) = parse_anchor_suffix(&chars, j);
        let after_token = token_end == chars.len() || !chars[token_end].is_alphanumeric();
        if !after_token {
            i = j;
            continue;
        }

        out.push(ParsedMention {
            char_range: i..token_end,
            id,
            anchor,
        });
        i = token_end;
    }
    out
}

pub fn parse_anchor_suffix_buffer(chars: &[char], start: usize) -> (usize, Option<String>) {
    parse_anchor_suffix(chars, start)
}

fn parse_anchor_suffix(chars: &[char], start: usize) -> (usize, Option<String>) {
    if start >= chars.len() || chars[start] != '(' {
        return (start, None);
    }
    let mut k = start + 1;
    if k >= chars.len() || chars[k] != '"' {
        return (start, None);
    }
    k += 1;
    let mut anchor = String::new();
    while k < chars.len() {
        match chars[k] {
            '"' => {
                if k + 1 < chars.len() && chars[k + 1] == ')' {
                    return (k + 2, Some(anchor));
                }
                return (start, None);
            }
            '\\' => {
                if k + 1 >= chars.len() {
                    return (start, None);
                }
                match chars[k + 1] {
                    '"' => anchor.push('"'),
                    '\\' => anchor.push('\\'),
                    _ => return (start, None),
                }
                k += 2;
            }
            c => {
                anchor.push(c);
                k += 1;
            }
        }
    }
    (start, None)
}

pub fn parse_mentions_in_doc(content: &str) -> Vec<DocMention> {
    let mut out = Vec::new();
    let mut in_fence = false;
    for (line_idx, line) in content.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        for parsed in parse_mentions_in_line(line) {
            out.push(DocMention {
                line_idx,
                char_range: parsed.char_range,
                id: parsed.id,
                anchor: parsed.anchor,
            });
        }
    }
    out
}

pub fn should_open_mention(line: &str, at_char_col: usize) -> bool {
    if at_char_col == 0 {
        return true;
    }
    line.chars()
        .nth(at_char_col - 1)
        .is_none_or(|c| c.is_whitespace())
}

pub fn is_break_char(c: char) -> bool {
    if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' || c == '\'' {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_at_start_of_line() {
        assert!(should_open_mention("@hello", 0));
    }

    #[test]
    fn opens_after_space() {
        assert!(should_open_mention("hi @world", 3));
    }

    #[test]
    fn does_not_open_mid_word() {
        assert!(!should_open_mention("user@example.com", 4));
    }

    #[test]
    fn does_not_open_after_letter() {
        assert!(!should_open_mention("foo@bar", 3));
    }

    #[test]
    fn break_chars_dismiss() {
        for c in [',', '.', ';', ':', '!', '?', '(', ')', '/', '[', ']'] {
            assert!(is_break_char(c), "{c} should break");
        }
    }

    #[test]
    fn alphanumeric_does_not_break() {
        for c in ['a', 'Z', '5', ' ', '-', '_', '\''] {
            assert!(!is_break_char(c), "{c} should not break");
        }
    }

    #[test]
    fn parse_token_with_anchor() {
        let result = parse_mentions_in_line("see @id:42(\"naval\") here");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 42);
        assert_eq!(result[0].anchor.as_deref(), Some("naval"));
        assert_eq!(result[0].char_range, 4..19);
    }

    #[test]
    fn parse_token_with_escaped_quote_in_anchor() {
        let result = parse_mentions_in_line("@id:1(\"with \\\"quote\\\"\")");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].anchor.as_deref(), Some("with \"quote\""));
    }

    #[test]
    fn parse_token_with_escaped_backslash_in_anchor() {
        let result = parse_mentions_in_line("@id:1(\"path\\\\here\")");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].anchor.as_deref(), Some("path\\here"));
    }

    #[test]
    fn parse_malformed_unclosed_paren_falls_back_to_bare() {
        let result = parse_mentions_in_line("@id:7(\"open and rest of line");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 7);
        assert!(result[0].anchor.is_none());
        assert_eq!(result[0].char_range, 0..5);
    }

    #[test]
    fn parse_malformed_missing_close_paren_falls_back_to_bare() {
        let result = parse_mentions_in_line("@id:7(\"closed quote\" but no paren");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 7);
        assert!(result[0].anchor.is_none());
    }

    #[test]
    fn parse_two_anchored_tokens_in_line() {
        let result = parse_mentions_in_line("@id:1(\"naval\") and @id:5(\"books\")");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].anchor.as_deref(), Some("naval"));
        assert_eq!(result[1].anchor.as_deref(), Some("books"));
    }

    #[test]
    fn parse_anchored_then_punctuation_ok() {
        let result = parse_mentions_in_line("see @id:7(\"naval\"), then more");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 7);
        assert_eq!(result[0].anchor.as_deref(), Some("naval"));
    }

    #[test]
    fn parse_single_token_at_start() {
        let result = parse_mentions_in_line("@id:159 hello");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 159);
        assert_eq!(result[0].char_range, 0..7);
    }

    #[test]
    fn parse_token_after_space() {
        let result = parse_mentions_in_line("see @id:42");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 42);
        assert_eq!(result[0].char_range, 4..10);
    }

    #[test]
    fn parse_multiple_tokens_in_line() {
        let result = parse_mentions_in_line("see @id:1 and @id:2");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, 1);
        assert_eq!(result[1].id, 2);
    }

    #[test]
    fn parse_token_followed_by_punctuation() {
        let result = parse_mentions_in_line("ref @id:7, then continue");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 7);
        assert_eq!(result[0].char_range, 4..9);
    }

    #[test]
    fn parse_rejects_email_at_pattern() {
        let result = parse_mentions_in_line("user@id:foo.com");
        assert!(result.is_empty());
    }

    #[test]
    fn parse_rejects_non_digit_id() {
        let result = parse_mentions_in_line("@id:abc rest");
        assert!(result.is_empty());
    }

    #[test]
    fn parse_rejects_digit_suffix_alphanumeric() {
        let result = parse_mentions_in_line("@id:159foo");
        assert!(result.is_empty());
    }

    #[test]
    fn parse_handles_multibyte_chars_around_token() {
        let result = parse_mentions_in_line("café @id:5 résumé");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 5);
        let chars: Vec<char> = "café @id:5 résumé".chars().collect();
        let token: String = chars[result[0].char_range.clone()].iter().collect();
        assert_eq!(token, "@id:5");
    }

    #[test]
    fn doc_scan_no_fences_finds_all_tokens() {
        let content = "first @id:1 line\nsecond @id:2 line";
        let result = parse_mentions_in_doc(content);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line_idx, 0);
        assert_eq!(result[0].id, 1);
        assert_eq!(result[1].line_idx, 1);
        assert_eq!(result[1].id, 2);
    }

    #[test]
    fn doc_scan_skips_tokens_inside_fence() {
        let content = "@id:1 outside\n```\n@id:2 inside\n```\n@id:3 outside again";
        let result = parse_mentions_in_doc(content);
        let ids: Vec<u32> = result.iter().map(|m| m.id).collect();
        assert_eq!(ids, vec![1, 3]);
    }

    #[test]
    fn doc_scan_handles_language_hint_fence() {
        let content = "@id:1\n```rust\n@id:2\n```\n@id:3";
        let result = parse_mentions_in_doc(content);
        let ids: Vec<u32> = result.iter().map(|m| m.id).collect();
        assert_eq!(ids, vec![1, 3]);
    }

    #[test]
    fn doc_scan_handles_unclosed_fence() {
        let content = "@id:1\n```\n@id:2 still inside";
        let result = parse_mentions_in_doc(content);
        let ids: Vec<u32> = result.iter().map(|m| m.id).collect();
        assert_eq!(ids, vec![1]);
    }

    #[test]
    fn doc_scan_fence_marker_line_itself_excluded() {
        let content = "```\n@id:1\n```";
        let result = parse_mentions_in_doc(content);
        assert!(result.is_empty());
    }

    #[test]
    fn doc_scan_handles_multiple_fences() {
        let content = "@id:1\n```\n@id:2\n```\n@id:3\n```py\n@id:4\n```\n@id:5";
        let result = parse_mentions_in_doc(content);
        let ids: Vec<u32> = result.iter().map(|m| m.id).collect();
        assert_eq!(ids, vec![1, 3, 5]);
    }
}
