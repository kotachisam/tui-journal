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
    fn non_shortcode_chars_terminate_the_query() {
        for c in [' ', '.', ',', '!', '/', '(', '\t'] {
            assert!(!is_shortcode_char(c), "{c}");
        }
    }
}
