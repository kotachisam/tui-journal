use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Clear, List, ListItem, ListState},
};

use backend::Entry;

const MAX_MENTION_SUGGESTIONS: usize = 8;
const MENTION_DISPLAY_TITLE_MAX_CHARS: usize = 50;

pub struct MentionState {
    pub anchor_line: usize,
    pub anchor_col: usize,
    pub query: String,
    pub candidates: Vec<MentionCandidate>,
    pub selected_idx: usize,
}

#[derive(Clone)]
pub struct MentionCandidate {
    pub id: u32,
    pub display_title: String,
}

impl MentionState {
    pub fn new(anchor_line: usize, anchor_col: usize) -> Self {
        Self {
            anchor_line,
            anchor_col,
            query: String::new(),
            candidates: Vec::new(),
            selected_idx: 0,
        }
    }

    pub fn move_up(&mut self) {
        if self.selected_idx > 0 {
            self.selected_idx -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected_idx + 1 < self.candidates.len() {
            self.selected_idx += 1;
        }
    }

    pub fn selected(&self) -> Option<&MentionCandidate> {
        self.candidates.get(self.selected_idx)
    }
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

pub fn build_search_text(title: &str, content: &str, tags: &[String]) -> String {
    const SEPARATOR: &str = " — ";
    let content_flat: String = content
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect();
    let tags_joined = tags.join(" ");
    [title.trim(), content_flat.trim(), tags_joined.trim()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(SEPARATOR)
}

pub fn resolve_display_title(title: &str, content: &str) -> String {
    let trimmed_title = title.trim();
    if !trimmed_title.is_empty() {
        return truncate(trimmed_title, MENTION_DISPLAY_TITLE_MAX_CHARS);
    }
    let first_line = content
        .lines()
        .find(|l| !l.trim().is_empty())
        .map(str::trim)
        .unwrap_or("(empty)");
    truncate(first_line, MENTION_DISPLAY_TITLE_MAX_CHARS)
}

fn truncate(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        return s.to_string();
    }
    let truncated: String = chars.iter().take(max_chars).collect();
    format!("{truncated}…")
}

pub fn build_candidates(entries: &[Entry], current_entry_id: Option<u32>) -> Vec<(u32, String, String)> {
    entries
        .iter()
        .filter(|e| Some(e.id) != current_entry_id)
        .filter(|e| e.deleted_at.is_none())
        .map(|e| {
            let display = resolve_display_title(&e.title, &e.content);
            let search = build_search_text(&e.title, &e.content, &e.tags);
            (e.id, display, search)
        })
        .collect()
}

pub fn filter_candidates(
    candidates: &[(u32, String, String)],
    query: &str,
) -> Vec<MentionCandidate> {
    let trimmed = query.trim();

    if trimmed.is_empty() {
        let mut all: Vec<MentionCandidate> = candidates
            .iter()
            .map(|(id, title, _)| MentionCandidate {
                id: *id,
                display_title: title.clone(),
            })
            .collect();
        all.sort_by_key(|c| std::cmp::Reverse(c.id));
        all.truncate(MAX_MENTION_SUGGESTIONS);
        return all;
    }

    let matcher = SkimMatcherV2::default().smart_case();
    let mut scored: Vec<(i64, u32, String)> = candidates
        .iter()
        .filter_map(|(id, title, search)| {
            matcher
                .fuzzy_match(search, trimmed)
                .map(|score| (score, *id, title.clone()))
        })
        .collect();
    scored.sort_by_key(|(score, _, _)| std::cmp::Reverse(*score));
    scored.truncate(MAX_MENTION_SUGGESTIONS);
    scored
        .into_iter()
        .map(|(_, id, display_title)| MentionCandidate { id, display_title })
        .collect()
}

pub fn format_mention_token(id: u32) -> String {
    format!("@id:{id}")
}

pub fn render_overlay(frame: &mut Frame, anchor: Rect, state: &MentionState) {
    if state.candidates.is_empty() {
        return;
    }

    let frame_area = frame.area();
    let desired_height = (state.candidates.len() as u16) + 2;
    let below_y = anchor.y + anchor.height;
    let space_below = frame_area.height.saturating_sub(below_y);

    let (overlay_y, overlay_height) = if space_below >= desired_height {
        (below_y, desired_height)
    } else if anchor.y >= desired_height {
        (anchor.y - desired_height, desired_height)
    } else {
        (below_y, space_below.max(3).min(desired_height))
    };

    let overlay_width = 50u16.min(frame_area.width.saturating_sub(anchor.x));
    let overlay_area = Rect {
        x: anchor.x,
        y: overlay_y,
        width: overlay_width,
        height: overlay_height,
    };

    let items: Vec<ListItem> = state
        .candidates
        .iter()
        .map(|c| ListItem::new(c.display_title.as_str()))
        .collect();

    let title = if state.query.is_empty() {
        "Mention — Tab/Enter insert, Esc dismiss".to_owned()
    } else {
        format!("Mention: {} — Tab/Enter insert", state.query)
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(Color::LightBlue).fg(Color::Black));

    let mut list_state = ListState::default();
    list_state.select(Some(state.selected_idx));

    frame.render_widget(Clear, overlay_area);
    frame.render_stateful_widget(list, overlay_area, &mut list_state);
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
    fn display_title_uses_title_when_present() {
        assert_eq!(resolve_display_title("My Title", "body content"), "My Title");
    }

    #[test]
    fn display_title_falls_back_to_first_body_line_when_blank() {
        assert_eq!(
            resolve_display_title("", "First line of body\nsecond"),
            "First line of body"
        );
    }

    #[test]
    fn display_title_skips_blank_body_lines() {
        assert_eq!(
            resolve_display_title("", "\n\n   \nactual content"),
            "actual content"
        );
    }

    #[test]
    fn display_title_truncates_long_text() {
        let long = "a".repeat(100);
        let resolved = resolve_display_title("", &long);
        assert!(resolved.ends_with('…'));
        assert!(resolved.chars().count() <= MENTION_DISPLAY_TITLE_MAX_CHARS + 1);
    }

    #[test]
    fn display_title_handles_empty_entry() {
        assert_eq!(resolve_display_title("", ""), "(empty)");
    }

    #[test]
    fn filter_with_empty_query_returns_all_recent_first() {
        let candidates = vec![
            (1u32, "First".into(), "First first".into()),
            (5u32, "Fifth".into(), "Fifth fifth".into()),
            (3u32, "Third".into(), "Third third".into()),
        ];
        let result = filter_candidates(&candidates, "");
        assert_eq!(result[0].id, 5);
        assert_eq!(result[1].id, 3);
        assert_eq!(result[2].id, 1);
    }

    #[test]
    fn filter_matches_by_search_text_not_just_title() {
        let candidates = vec![
            (1u32, "Untitled".into(), "Untitled — body about naval ravikant".into()),
            (2u32, "Other".into(), "Other — unrelated content".into()),
        ];
        let result = filter_candidates(&candidates, "naval");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 1);
    }

    #[test]
    fn filter_handles_multi_word_query() {
        let candidates = vec![
            (1u32, "Ent A".into(), "alpha beta gamma".into()),
            (2u32, "Ent B".into(), "alpha gamma".into()),
        ];
        let result = filter_candidates(&candidates, "alpha beta");
        assert!(!result.is_empty());
        assert_eq!(result[0].id, 1);
    }

    #[test]
    fn format_token_shape() {
        assert_eq!(format_mention_token(246), "@id:246");
    }
}
