use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub(crate) fn char_cols(c: char) -> usize {
    UnicodeWidthChar::width(c).unwrap_or(0)
}

pub(crate) fn str_cols(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

pub(crate) fn prefix_cols(s: &str, char_idx: usize) -> usize {
    s.chars().take(char_idx).map(char_cols).sum()
}

pub(crate) fn char_idx_at_col(s: &str, col: usize) -> usize {
    let mut cols = 0usize;
    for (idx, c) in s.chars().enumerate() {
        let next = cols + char_cols(c);
        if next > col {
            return idx;
        }
        cols = next;
    }
    s.chars().count()
}

pub(crate) fn to_cells(s: &str) -> Vec<char> {
    let mut cells = Vec::with_capacity(s.len());
    for c in s.chars() {
        match char_cols(c) {
            0 => {}
            1 => cells.push(c),
            n => {
                cells.push(c);
                cells.extend(std::iter::repeat_n(' ', n - 1));
            }
        }
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_chars_are_one_column() {
        for c in ['a', 'Z', '0', ' ', '-'] {
            assert_eq!(char_cols(c), 1, "{c}");
        }
    }

    #[test]
    fn emoji_and_cjk_are_two_columns() {
        for c in ['🎉', '🚀', '中', '日'] {
            assert_eq!(char_cols(c), 2, "{c}");
        }
    }

    #[test]
    fn zero_width_joiner_is_zero_columns() {
        assert_eq!(char_cols('\u{200d}'), 0);
        assert_eq!(char_cols('\u{fe0f}'), 0);
    }

    #[test]
    fn str_cols_sums_mixed_content() {
        assert_eq!(str_cols("hi"), 2);
        assert_eq!(str_cols("hi🎉"), 4);
        assert_eq!(str_cols("中文ab"), 6);
    }

    #[test]
    fn prefix_cols_counts_chars_not_bytes() {
        assert_eq!(prefix_cols("🎉ab", 0), 0);
        assert_eq!(prefix_cols("🎉ab", 1), 2);
        assert_eq!(prefix_cols("🎉ab", 2), 3);
        assert_eq!(prefix_cols("🎉ab", 3), 4);
    }

    #[test]
    fn prefix_cols_clamps_past_end() {
        assert_eq!(prefix_cols("ab", 99), 2);
    }

    #[test]
    fn char_idx_at_col_lands_before_wide_char() {
        assert_eq!(char_idx_at_col("a🎉b", 0), 0);
        assert_eq!(char_idx_at_col("a🎉b", 1), 1);
        assert_eq!(char_idx_at_col("a🎉b", 2), 1);
        assert_eq!(char_idx_at_col("a🎉b", 3), 2);
        assert_eq!(char_idx_at_col("a🎉b", 4), 3);
    }

    #[test]
    fn char_idx_at_col_clamps_past_end() {
        assert_eq!(char_idx_at_col("ab", 99), 2);
    }

    #[test]
    fn to_cells_pads_wide_chars_and_drops_zero_width() {
        assert_eq!(to_cells("ab"), vec!['a', 'b']);
        assert_eq!(to_cells("a🎉b"), vec!['a', '🎉', ' ', 'b']);
        assert_eq!(to_cells("a\u{200d}b"), vec!['a', 'b']);
    }

    #[test]
    fn to_cells_length_matches_str_cols() {
        for s in ["", "plain", "a🎉b", "中文ab", "🚀🚀"] {
            assert_eq!(to_cells(s).len(), str_cols(s), "{s}");
        }
    }

    #[test]
    fn round_trip_prefix_and_index_on_wide_text() {
        let s = "a🎉中b";
        for idx in 0..=s.chars().count() {
            let col = prefix_cols(s, idx);
            assert_eq!(char_idx_at_col(s, col), idx, "idx {idx} col {col}");
        }
    }
}
