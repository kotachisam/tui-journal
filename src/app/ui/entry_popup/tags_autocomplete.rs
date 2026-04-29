//! Tag-specific helpers for the entry popup's tags-field autocomplete.
//!
//! The fuzzy matching, navigation state, and overlay render all live in
//! `super::fuzzy_suggestions`. This module owns the tag-specific string
//! operations: detecting the "active query" inside a comma-separated list,
//! splicing in a selected tag, and one-shot deletion of the current tag.

/// Converts a character-index cursor (as returned by `TextArea::cursor`) into
/// a byte index into `line`. Necessary because tags can contain emoji, where
/// char index and byte index diverge.
pub fn char_index_to_byte_index(line: &str, char_idx: usize) -> usize {
    line.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(line.len())
}

/// Returns the substring of `line` between the last comma (or start of line)
/// and `cursor`, with leading whitespace trimmed. The "active query" is what
/// the user is currently typing as a tag, before they reach the next comma.
///
/// `cursor` is a character index (per `TextArea::cursor`).
pub fn extract_active_query(line: &str, cursor: usize) -> &str {
    let cursor_byte = char_index_to_byte_index(line, cursor);
    let prefix = &line[..cursor_byte];
    let after_comma = match prefix.rfind(',') {
        Some(idx) => &prefix[idx + 1..],
        None => prefix,
    };
    after_comma.trim_start()
}

/// Returns the byte index in `line` where the active query starts (after
/// the last comma + leading whitespace), useful for replacement insertion.
///
/// `cursor` is a character index (per `TextArea::cursor`).
pub fn active_query_start(line: &str, cursor: usize) -> usize {
    let cursor_byte = char_index_to_byte_index(line, cursor);
    let prefix = &line[..cursor_byte];
    let after_comma_idx = prefix.rfind(',').map(|i| i + 1).unwrap_or(0);
    let trim_offset = prefix[after_comma_idx..]
        .bytes()
        .take_while(|b| b.is_ascii_whitespace())
        .count();
    after_comma_idx + trim_offset
}

/// Removes the tag containing the cursor (or the previous one if the cursor
/// sits in a whitespace-only trailing segment), preserving trailing `, ` if
/// the original line had one. Returns `(new_line, new_cursor_char)` or `None`
/// if there's nothing to delete.
///
/// `cursor` is a character index (per `TextArea::cursor`).
pub fn delete_tag_at_cursor(line: &str, cursor: usize) -> Option<(String, usize)> {
    if line.is_empty() {
        return None;
    }
    let cursor_byte = char_index_to_byte_index(line, cursor);

    // Find which comma-separated segment the cursor is in by counting commas
    // strictly before the cursor's byte position.
    let mut comma_count = 0usize;
    for (b, ch) in line.char_indices() {
        if b >= cursor_byte {
            break;
        }
        if ch == ',' {
            comma_count += 1;
        }
    }
    let cursor_segment_idx = comma_count;

    let tags: Vec<&str> = line.split(',').map(str::trim).collect();
    if cursor_segment_idx >= tags.len() {
        return None;
    }

    // If the cursor's segment is empty/whitespace and there's a previous
    // segment, the user probably wants the previous tag deleted (the
    // "Tab inserted, then changed mind" case where cursor sits after `, `).
    let target_idx = if tags[cursor_segment_idx].is_empty() && cursor_segment_idx > 0 {
        cursor_segment_idx - 1
    } else if tags[cursor_segment_idx].is_empty() {
        return None;
    } else {
        cursor_segment_idx
    };

    let nonempty: Vec<&str> = tags
        .iter()
        .enumerate()
        .filter(|(i, s)| *i != target_idx && !s.is_empty())
        .map(|(_, s)| *s)
        .collect();

    let trailing_comma = line.trim_end().ends_with(',');
    let new_line = if nonempty.is_empty() {
        String::new()
    } else if trailing_comma {
        format!("{}, ", nonempty.join(", "))
    } else {
        nonempty.join(", ")
    };

    let new_cursor_char = new_line.chars().count();
    Some((new_line, new_cursor_char))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_query_with_no_comma() {
        let line = "prod";
        assert_eq!(extract_active_query(line, 4), "prod");
    }

    #[test]
    fn extract_query_handles_multibyte_emoji_in_line() {
        // Repro of the panic: line ends mid-typing after emoji-prefixed tag.
        let line = "✅ Productive, ⚡ En";
        // 18 chars: 1 (✅) + 13 ( Productive, ) + 1 (⚡) + 3 ( En) = 18
        let cursor = line.chars().count();
        assert_eq!(cursor, 18);
        assert_eq!(extract_active_query(line, cursor), "⚡ En");
    }

    #[test]
    fn active_query_start_handles_multibyte_emoji() {
        let line = "✅ Productive, ⚡ En";
        let cursor = line.chars().count();
        assert_eq!(active_query_start(line, cursor), 16);
    }

    #[test]
    fn extract_query_after_single_comma() {
        let line = "✅ Productive, ref";
        assert_eq!(extract_active_query(line, line.len()), "ref");
    }

    #[test]
    fn extract_query_trims_leading_whitespace_after_comma() {
        let line = "tag1,    prod";
        assert_eq!(extract_active_query(line, line.len()), "prod");
    }

    #[test]
    fn extract_query_empty_after_trailing_comma() {
        let line = "✅ Productive, ";
        assert_eq!(extract_active_query(line, line.len()), "");
    }

    #[test]
    fn extract_query_handles_cursor_in_middle() {
        let line = "tag1, prod, tag3";
        assert_eq!(extract_active_query(line, 10), "prod");
    }

    #[test]
    fn active_query_start_at_zero_when_no_comma() {
        let line = "prod";
        assert_eq!(active_query_start(line, 4), 0);
    }

    #[test]
    fn active_query_start_after_comma_and_whitespace() {
        let line = "tag1,  prod";
        assert_eq!(active_query_start(line, line.len()), 7);
    }

    #[test]
    fn char_to_byte_clamps_when_out_of_bounds() {
        let line = "abc";
        assert_eq!(char_index_to_byte_index(line, 99), 3);
    }

    #[test]
    fn delete_tag_removes_last_after_trailing_comma() {
        let line = "✅ Productive, ⚡ Energised, ";
        let cursor = line.chars().count();
        let (new_line, new_cursor) = delete_tag_at_cursor(line, cursor).unwrap();
        assert_eq!(new_line, "✅ Productive, ");
        assert_eq!(new_cursor, new_line.chars().count());
    }

    #[test]
    fn delete_tag_removes_middle() {
        let line = "a, b, c";
        let cursor_byte = 4;
        let cursor = line[..cursor_byte].chars().count();
        let (new_line, _) = delete_tag_at_cursor(line, cursor).unwrap();
        assert_eq!(new_line, "a, c");
    }

    #[test]
    fn delete_tag_removes_last_no_trailing_comma() {
        let line = "a, b";
        let cursor = line.chars().count();
        let (new_line, _) = delete_tag_at_cursor(line, cursor).unwrap();
        assert_eq!(new_line, "a");
    }

    #[test]
    fn delete_tag_clears_single_tag() {
        let line = "loneTag";
        let cursor = line.chars().count();
        let (new_line, new_cursor) = delete_tag_at_cursor(line, cursor).unwrap();
        assert_eq!(new_line, "");
        assert_eq!(new_cursor, 0);
    }

    #[test]
    fn delete_tag_clears_just_comma_space() {
        let line = ", ";
        let cursor = line.chars().count();
        let (new_line, _) = delete_tag_at_cursor(line, cursor).unwrap();
        assert_eq!(new_line, "");
    }

    #[test]
    fn delete_tag_returns_none_for_empty_line() {
        assert!(delete_tag_at_cursor("", 0).is_none());
    }

    #[test]
    fn delete_tag_handles_multibyte_in_middle() {
        let line = "a, ⚡ Energised, c";
        let cursor_byte = "a, ⚡ Energised".len();
        let cursor = line[..cursor_byte].chars().count();
        let (new_line, _) = delete_tag_at_cursor(line, cursor).unwrap();
        assert_eq!(new_line, "a, c");
    }
}
