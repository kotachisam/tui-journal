pub const MIN_QUERY_CHARS: usize = 2;

pub fn should_open_emoji(line: &str, colon_char_col: usize) -> bool {
    if colon_char_col == 0 {
        return true;
    }
    line.chars()
        .nth(colon_char_col - 1)
        .is_none_or(|c| c.is_whitespace())
}

pub fn is_shortcode_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+')
}

pub fn reopen_anchor(line: &str, cursor_char_col: usize) -> Option<usize> {
    let chars: Vec<char> = line.chars().collect();
    if chars
        .get(cursor_char_col)
        .is_some_and(|&c| is_shortcode_char(c))
    {
        return None;
    }
    let before = chars.get(..cursor_char_col)?;
    let query_len = before
        .iter()
        .rev()
        .take_while(|&&c| is_shortcode_char(c))
        .count();
    let colon_col = cursor_char_col.checked_sub(query_len + 1)?;
    (chars[colon_col] == ':' && should_open_emoji(line, colon_col)).then_some(colon_col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_at_start_of_line() {
        assert!(should_open_emoji(":tada:", 0));
    }

    #[test]
    fn opens_after_space() {
        assert!(should_open_emoji("shipped :tada:", 8));
    }

    #[test]
    fn does_not_open_inside_a_clock_time() {
        assert!(!should_open_emoji("09:30 standup", 2));
    }

    #[test]
    fn does_not_open_after_word_character() {
        assert!(!should_open_emoji("note:this", 4));
    }

    #[test]
    fn does_not_open_inside_a_url() {
        assert!(!should_open_emoji("https://example.com", 5));
    }

    #[test]
    fn shortcode_chars_cover_real_shortcodes() {
        for c in ['a', 'Z', '0', '_', '-', '+'] {
            assert!(is_shortcode_char(c), "{c}");
        }
    }

    #[test]
    fn reopen_finds_the_colon_behind_a_trailing_token() {
        assert_eq!(reopen_anchor("last thing :dgo", 15), Some(11));
    }

    #[test]
    fn reopen_accepts_a_bare_colon() {
        assert_eq!(reopen_anchor("so :", 4), Some(3));
    }

    #[test]
    fn reopen_ignores_a_cursor_inside_a_token() {
        assert_eq!(reopen_anchor(":dog today", 2), None);
    }

    #[test]
    fn reopen_allows_a_break_char_after_the_cursor() {
        assert_eq!(reopen_anchor(":dog today", 4), Some(0));
    }

    #[test]
    fn reopen_skips_clock_times_and_mid_word_colons() {
        assert_eq!(reopen_anchor("at 09:30", 8), None);
        assert_eq!(reopen_anchor("note:this", 9), None);
    }

    #[test]
    fn reopen_needs_a_colon_before_the_token() {
        assert_eq!(reopen_anchor("plain words", 11), None);
        assert_eq!(reopen_anchor("", 0), None);
    }

    #[test]
    fn reopen_stops_at_non_shortcode_chars() {
        assert_eq!(reopen_anchor(":dog, cat", 9), None);
    }

    #[test]
    fn non_shortcode_chars_terminate_the_query() {
        for c in [' ', '.', ',', '!', '/', '(', '\t'] {
            assert!(!is_shortcode_char(c), "{c}");
        }
    }
}
