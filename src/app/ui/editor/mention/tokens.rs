pub fn format_mention_token(id: u32, anchor: Option<&str>) -> String {
    match anchor.filter(|s| !s.is_empty()) {
        Some(s) => {
            let mut escaped = String::with_capacity(s.len());
            for c in s.chars() {
                match c {
                    '\\' => escaped.push_str("\\\\"),
                    '"' => escaped.push_str("\\\""),
                    other => escaped.push(other),
                }
            }
            format!("@id:{id}(\"{escaped}\")")
        }
        None => format!("@id:{id}"),
    }
}

pub fn find_anchor_line(content: &str, anchor: &str) -> Option<u16> {
    if anchor.is_empty() {
        return None;
    }
    let needle = anchor.to_lowercase();
    content
        .lines()
        .enumerate()
        .find_map(|(idx, line)| line.to_lowercase().contains(&needle).then_some(idx as u16))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_token_shape() {
        assert_eq!(format_mention_token(246, None), "@id:246");
    }

    #[test]
    fn format_token_with_anchor() {
        assert_eq!(
            format_mention_token(246, Some("naval")),
            "@id:246(\"naval\")"
        );
    }

    #[test]
    fn format_token_escapes_quote_and_backslash() {
        assert_eq!(
            format_mention_token(1, Some("a\"b\\c")),
            "@id:1(\"a\\\"b\\\\c\")"
        );
    }

    #[test]
    fn format_token_empty_anchor_treated_as_none() {
        assert_eq!(format_mention_token(1, Some("")), "@id:1");
    }

    #[test]
    fn find_anchor_line_basic_hit() {
        assert_eq!(
            find_anchor_line("foo\nbar naval baz\nqux", "naval"),
            Some(1)
        );
    }

    #[test]
    fn find_anchor_line_case_insensitive() {
        assert_eq!(find_anchor_line("Foo Naval Bar", "naval"), Some(0));
        assert_eq!(find_anchor_line("foo NAVAL bar", "Naval"), Some(0));
    }

    #[test]
    fn find_anchor_line_not_found() {
        assert_eq!(find_anchor_line("foo bar baz", "naval"), None);
    }

    #[test]
    fn find_anchor_line_empty_anchor_returns_none() {
        assert_eq!(find_anchor_line("anything here", ""), None);
    }

    #[test]
    fn find_anchor_line_returns_first_match_only() {
        assert_eq!(
            find_anchor_line("naval one\nnaval two\nnaval three", "naval"),
            Some(0)
        );
    }
}
