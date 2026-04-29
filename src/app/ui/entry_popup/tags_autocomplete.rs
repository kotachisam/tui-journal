use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};

const MAX_SUGGESTIONS: usize = 8;

pub struct SuggestionState {
    matches: Vec<(i64, String)>,
    selected: usize,
}

impl SuggestionState {
    pub fn build(query: &str, tags: &[String]) -> Option<Self> {
        let matches = compute_matches(query, tags);
        if matches.is_empty() {
            None
        } else {
            Some(Self {
                matches,
                selected: 0,
            })
        }
    }

    pub fn matches(&self) -> &[(i64, String)] {
        &self.matches
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn selected_tag(&self) -> Option<&str> {
        self.matches.get(self.selected).map(|(_, tag)| tag.as_str())
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.matches.len() {
            self.selected += 1;
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }
}

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

fn compute_matches(query: &str, tags: &[String]) -> Vec<(i64, String)> {
    if query.is_empty() {
        return Vec::new();
    }
    let matcher = SkimMatcherV2::default();
    let mut scored: Vec<(i64, String)> = tags
        .iter()
        .filter_map(|tag| {
            matcher
                .fuzzy_match(tag, query)
                .map(|score| (score, tag.clone()))
        })
        .collect();
    scored.sort_by_key(|m| std::cmp::Reverse(m.0));
    scored.truncate(MAX_SUGGESTIONS);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tags() -> Vec<String> {
        vec![
            "✅ Productive".to_owned(),
            "🪞 Reflective".to_owned(),
            "🪨 Grounded".to_owned(),
            "🤝 Connected".to_owned(),
            "🟢 Aligned".to_owned(),
            "🌫️ Uncertain".to_owned(),
            "🧠 Curious".to_owned(),
            "🎨 Creative".to_owned(),
            "📝 Post".to_owned(),
        ]
    }

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
        // The active query starts at the byte index of `⚡`, which is 16
        // (after `✅` (3 bytes) + ` Productive, ` (13 bytes)).
        assert_eq!(active_query_start(line, cursor), 16);
    }

    #[test]
    fn char_to_byte_clamps_when_out_of_bounds() {
        let line = "abc";
        // char_idx beyond the string length clamps to line.len() (3).
        assert_eq!(char_index_to_byte_index(line, 99), 3);
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
        // Cursor right after "prod"
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
        // Last comma at index 4, then 2 whitespace chars; query starts at 7
        assert_eq!(active_query_start(line, line.len()), 7);
    }

    #[test]
    fn compute_matches_finds_productive_from_partial() {
        let tags = sample_tags();
        let matches = compute_matches("prod", &tags);
        assert!(!matches.is_empty(), "should match Productive");
        assert_eq!(matches[0].1, "✅ Productive");
    }

    #[test]
    fn compute_matches_finds_reflective_from_partial() {
        let tags = sample_tags();
        let matches = compute_matches("ref", &tags);
        assert!(!matches.is_empty(), "should match Reflective");
        assert_eq!(matches[0].1, "🪞 Reflective");
    }

    #[test]
    fn compute_matches_returns_empty_for_no_matches() {
        let tags = sample_tags();
        let matches = compute_matches("zzznoresult", &tags);
        assert!(matches.is_empty());
    }

    #[test]
    fn compute_matches_returns_empty_for_empty_query() {
        let tags = sample_tags();
        let matches = compute_matches("", &tags);
        assert!(matches.is_empty());
    }

    #[test]
    fn compute_matches_caps_at_eight() {
        let tags: Vec<String> = (0..50).map(|i| format!("tag-{i}")).collect();
        let matches = compute_matches("tag", &tags);
        assert!(matches.len() <= MAX_SUGGESTIONS);
    }

    #[test]
    fn suggestion_state_starts_with_zero_selected() {
        let tags = sample_tags();
        let state = SuggestionState::build("prod", &tags).expect("should match");
        assert_eq!(state.selected_index(), 0);
    }

    #[test]
    fn suggestion_state_returns_none_when_no_matches() {
        let tags = sample_tags();
        assert!(SuggestionState::build("zzznoresult", &tags).is_none());
    }

    #[test]
    fn suggestion_state_move_down_clamps_at_end() {
        let tags = sample_tags();
        let mut state = SuggestionState::build("e", &tags).expect("should match");
        let count = state.matches().len();
        for _ in 0..(count + 5) {
            state.move_down();
        }
        assert_eq!(state.selected_index(), count - 1);
    }

    #[test]
    fn delete_tag_removes_last_after_trailing_comma() {
        // "Tab-inserted then changed mind" case
        let line = "✅ Productive, ⚡ Energised, ";
        let cursor = line.chars().count();
        let (new_line, new_cursor) = delete_tag_at_cursor(line, cursor).unwrap();
        assert_eq!(new_line, "✅ Productive, ");
        assert_eq!(new_cursor, new_line.chars().count());
    }

    #[test]
    fn delete_tag_removes_middle() {
        let line = "a, b, c";
        let cursor_byte = 4; // inside " b"
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
        // Cursor right after "Energised" — inside the middle tag
        let cursor_byte = "a, ⚡ Energised".len();
        let cursor = line[..cursor_byte].chars().count();
        let (new_line, _) = delete_tag_at_cursor(line, cursor).unwrap();
        assert_eq!(new_line, "a, c");
    }

    #[test]
    fn suggestion_state_move_up_clamps_at_zero() {
        let tags = sample_tags();
        let mut state = SuggestionState::build("e", &tags).expect("should match");
        state.move_down();
        for _ in 0..10 {
            state.move_up();
        }
        assert_eq!(state.selected_index(), 0);
    }
}
