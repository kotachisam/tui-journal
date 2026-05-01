use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState},
};

use backend::Entry;

use crate::settings::DateFormat;

const MAX_MENTION_SUGGESTIONS: usize = 8;
const SNIPPET_WINDOW_CHARS: usize = 70;
const SNIPPET_LEAD_CONTEXT: usize = 12;
const OVERLAY_WIDTH: u16 = 80;
const BODY_SCORE_WEIGHT: i64 = 2;

pub struct MentionState {
    pub anchor_line: usize,
    pub anchor_col: usize,
    pub query: String,
    pub candidates: Vec<MentionCandidate>,
    pub selected_idx: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MentionCandidate {
    pub id: u32,
    pub snippet: String,
    pub match_indices: Vec<usize>,
    pub date_display: String,
}

#[derive(Clone)]
pub struct CandidateSource {
    pub id: u32,
    pub body_flat: String,
    pub body_first_line: String,
    pub title: String,
    pub tags_joined: String,
    pub date_display: String,
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

fn flatten_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_space && !out.is_empty() {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

fn first_non_blank_line(s: &str) -> String {
    s.lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_string())
        .unwrap_or_else(|| "(empty)".to_string())
}

pub fn build_candidates(
    entries: &[Entry],
    current_entry_id: Option<u32>,
    date_format: &DateFormat,
) -> Vec<CandidateSource> {
    entries
        .iter()
        .filter(|e| Some(e.id) != current_entry_id)
        .filter(|e| e.deleted_at.is_none())
        .map(|e| CandidateSource {
            id: e.id,
            body_flat: flatten_whitespace(&e.content),
            body_first_line: first_non_blank_line(&e.content),
            title: e.title.trim().to_string(),
            tags_joined: e.tags.join(" "),
            date_display: date_format.display(&e.date),
        })
        .collect()
}

fn extract_snippet(
    body_flat: &str,
    body_first_line: &str,
    query: &str,
) -> (String, Vec<usize>) {
    let chars: Vec<char> = body_flat.chars().collect();
    if chars.is_empty() || query.trim().is_empty() {
        return (truncate_chars(body_first_line, SNIPPET_WINDOW_CHARS), Vec::new());
    }

    let matcher = SkimMatcherV2::default().smart_case();
    let Some((_, indices)) = matcher.fuzzy_indices(body_flat, query.trim()) else {
        return (truncate_chars(body_first_line, SNIPPET_WINDOW_CHARS), Vec::new());
    };
    let Some(&first_match) = indices.first() else {
        return (truncate_chars(body_first_line, SNIPPET_WINDOW_CHARS), Vec::new());
    };

    let total = chars.len();
    let start = first_match.saturating_sub(SNIPPET_LEAD_CONTEXT);
    let end = (start + SNIPPET_WINDOW_CHARS).min(total);
    let start = end.saturating_sub(SNIPPET_WINDOW_CHARS).min(start);

    let leading = if start > 0 { 1 } else { 0 };
    let mut out = String::new();
    if leading == 1 {
        out.push('…');
    }
    out.extend(chars[start..end].iter());
    if end < total {
        out.push('…');
    }

    let match_indices: Vec<usize> = indices
        .into_iter()
        .filter(|&i| i >= start && i < end)
        .map(|i| i - start + leading)
        .collect();

    (out, match_indices)
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        return s.to_string();
    }
    let prefix: String = chars.iter().take(max_chars).collect();
    format!("{prefix}…")
}

fn score_candidate(matcher: &SkimMatcherV2, src: &CandidateSource, query: &str) -> Option<i64> {
    let body_score = matcher
        .fuzzy_match(&src.body_flat, query)
        .map(|s| s * BODY_SCORE_WEIGHT);
    let other_haystack = if src.title.is_empty() {
        src.tags_joined.clone()
    } else if src.tags_joined.is_empty() {
        src.title.clone()
    } else {
        format!("{} {}", src.title, src.tags_joined)
    };
    let other_score = if other_haystack.is_empty() {
        None
    } else {
        matcher.fuzzy_match(&other_haystack, query)
    };
    match (body_score, other_score) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

pub fn filter_candidates(sources: &[CandidateSource], query: &str) -> Vec<MentionCandidate> {
    let trimmed = query.trim();

    if trimmed.is_empty() {
        let mut all: Vec<&CandidateSource> = sources.iter().collect();
        all.sort_by_key(|s| std::cmp::Reverse(s.id));
        return all
            .into_iter()
            .take(MAX_MENTION_SUGGESTIONS)
            .map(|s| {
                let (snippet, match_indices) =
                    extract_snippet(&s.body_flat, &s.body_first_line, "");
                MentionCandidate {
                    id: s.id,
                    snippet,
                    match_indices,
                    date_display: s.date_display.clone(),
                }
            })
            .collect();
    }

    let matcher = SkimMatcherV2::default().smart_case();
    let mut scored: Vec<(i64, &CandidateSource)> = sources
        .iter()
        .filter_map(|s| score_candidate(&matcher, s, trimmed).map(|score| (score, s)))
        .collect();
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored.truncate(MAX_MENTION_SUGGESTIONS);
    scored
        .into_iter()
        .map(|(_, s)| {
            let (snippet, match_indices) =
                extract_snippet(&s.body_flat, &s.body_first_line, trimmed);
            MentionCandidate {
                id: s.id,
                snippet,
                match_indices,
                date_display: s.date_display.clone(),
            }
        })
        .collect()
}

pub fn format_mention_token(id: u32) -> String {
    format!("@id:{id}")
}

fn highlight_snippet<'a>(snippet: &'a str, match_indices: &[usize]) -> Line<'a> {
    if match_indices.is_empty() {
        return Line::from(snippet);
    }
    let match_set: std::collections::BTreeSet<usize> = match_indices.iter().copied().collect();
    let highlight = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let plain = Style::default();

    let mut spans: Vec<Span<'a>> = Vec::new();
    let mut buf = String::new();
    let mut buf_is_match = false;
    let flush = |buf: &mut String, is_match: bool, spans: &mut Vec<Span<'a>>| {
        if buf.is_empty() {
            return;
        }
        let style = if is_match { highlight } else { plain };
        spans.push(Span::styled(std::mem::take(buf), style));
    };

    for (i, ch) in snippet.chars().enumerate() {
        let is_match = match_set.contains(&i);
        if !buf.is_empty() && is_match != buf_is_match {
            flush(&mut buf, buf_is_match, &mut spans);
        }
        buf.push(ch);
        buf_is_match = is_match;
    }
    flush(&mut buf, buf_is_match, &mut spans);
    Line::from(spans)
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

    let max_width = frame_area.width.saturating_sub(2).max(20);
    let overlay_width = OVERLAY_WIDTH.min(max_width);
    let overlay_x = if anchor.x + overlay_width > frame_area.width {
        frame_area.width.saturating_sub(overlay_width)
    } else {
        anchor.x
    };
    let overlay_area = Rect {
        x: overlay_x,
        y: overlay_y,
        width: overlay_width,
        height: overlay_height,
    };

    let items: Vec<ListItem> = state
        .candidates
        .iter()
        .map(|c| ListItem::new(highlight_snippet(&c.snippet, &c.match_indices)))
        .collect();

    let date_display = state
        .selected()
        .map(|c| c.date_display.as_str())
        .unwrap_or("");
    let title = if date_display.is_empty() {
        "Mention — Tab/Enter insert, Esc dismiss".to_owned()
    } else {
        format!("{date_display} — Tab/Enter insert, Esc dismiss")
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
    use chrono::{TimeZone, Utc};

    fn entry(id: u32, title: &str, content: &str, tags: &[&str]) -> Entry {
        Entry::new(
            id,
            Utc.with_ymd_and_hms(2026, 5, 15, 12, 0, 0).unwrap(),
            title.to_string(),
            content.to_string(),
            tags.iter().map(|s| s.to_string()).collect(),
            None,
        )
    }

    fn dd_mm_yyyy() -> DateFormat {
        DateFormat::new("DD-MM-YYYY")
    }

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
    fn flattens_whitespace_to_single_spaces() {
        assert_eq!(
            flatten_whitespace("foo\n\n  bar\tbaz "),
            "foo bar baz"
        );
    }

    #[test]
    fn first_line_picks_first_non_blank() {
        assert_eq!(
            first_non_blank_line("\n\n   \nactual content\nmore"),
            "actual content"
        );
    }

    #[test]
    fn first_line_handles_empty() {
        assert_eq!(first_non_blank_line(""), "(empty)");
    }

    #[test]
    fn snippet_with_empty_query_returns_first_line() {
        let body_flat = "First line of body second line";
        let first = "First line of body";
        let (snippet, indices) = extract_snippet(body_flat, first, "");
        assert_eq!(snippet, "First line of body");
        assert!(indices.is_empty());
    }

    #[test]
    fn snippet_with_match_extracts_window_around_match() {
        let body: String = "lorem ipsum ".repeat(20) + "naval ravikant " + &"more text ".repeat(20);
        let (snippet, indices) = extract_snippet(&body, "lorem ipsum", "naval");
        assert!(snippet.contains("naval"), "snippet missing match: {snippet}");
        assert!(snippet.starts_with('…'), "expected leading ellipsis: {snippet}");
        let char_count = snippet.chars().count();
        assert!(char_count <= SNIPPET_WINDOW_CHARS + 2, "too long: {char_count}");
        assert!(!indices.is_empty(), "expected highlight indices");
    }

    #[test]
    fn snippet_falls_back_to_first_line_when_match_only_on_other_fields() {
        let body_flat = "body with no match here";
        let first = "body with no match here";
        let (snippet, indices) = extract_snippet(body_flat, first, "tagonly");
        assert_eq!(snippet, "body with no match here");
        assert!(indices.is_empty());
    }

    #[test]
    fn snippet_handles_multibyte_chars() {
        let body = "préface naïve résumé café — naval ravikant — fin";
        let (snippet, _) = extract_snippet(body, "préface naïve", "naval");
        assert!(snippet.contains("naval"));
    }

    #[test]
    fn snippet_match_indices_point_to_correct_chars_when_no_ellipsis() {
        let body = "naval ravikant on twitter";
        let first = body;
        let (snippet, indices) = extract_snippet(body, first, "naval");
        assert_eq!(snippet, body);
        let chars: Vec<char> = snippet.chars().collect();
        for &i in &indices {
            assert!("naval".contains(chars[i]), "char at {i} not in 'naval'");
        }
    }

    #[test]
    fn snippet_match_indices_offset_for_leading_ellipsis() {
        let body: String = "lorem ipsum ".repeat(20) + "naval";
        let (snippet, indices) = extract_snippet(&body, "lorem ipsum", "naval");
        assert!(snippet.starts_with('…'));
        let chars: Vec<char> = snippet.chars().collect();
        assert_eq!(chars[0], '…');
        for &i in &indices {
            assert!(i > 0, "matched char index {i} should not include the ellipsis");
        }
    }

    #[test]
    fn highlight_snippet_with_no_indices_returns_single_unstyled_span() {
        let line = highlight_snippet("plain text", &[]);
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].content, "plain text");
    }

    #[test]
    fn highlight_snippet_alternates_styled_and_plain_runs() {
        let line = highlight_snippet("naval ravikant", &[0, 1, 2, 3, 4]);
        assert_eq!(line.spans.len(), 2);
        assert_eq!(line.spans[0].content, "naval");
        assert_eq!(line.spans[0].style.fg, Some(Color::Yellow));
        assert_eq!(line.spans[1].content, " ravikant");
        assert_eq!(line.spans[1].style.fg, None);
    }

    #[test]
    fn build_candidates_skips_current_entry() {
        let entries = vec![entry(1, "", "alpha", &[]), entry(2, "", "beta", &[])];
        let sources = build_candidates(&entries, Some(1), &dd_mm_yyyy());
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].id, 2);
    }

    #[test]
    fn build_candidates_formats_date_via_setting() {
        let entries = vec![entry(1, "", "body", &[])];
        let sources = build_candidates(&entries, None, &dd_mm_yyyy());
        assert_eq!(sources[0].date_display, "15-05-2026");
    }

    #[test]
    fn filter_empty_query_returns_recency_ordered() {
        let entries = vec![
            entry(1, "", "first body", &[]),
            entry(5, "", "fifth body", &[]),
            entry(3, "", "third body", &[]),
        ];
        let sources = build_candidates(&entries, None, &dd_mm_yyyy());
        let result = filter_candidates(&sources, "");
        assert_eq!(result[0].id, 5);
        assert_eq!(result[1].id, 3);
        assert_eq!(result[2].id, 1);
    }

    #[test]
    fn filter_matches_body_content() {
        let entries = vec![
            entry(1, "", "body about naval ravikant on twitter", &[]),
            entry(2, "", "unrelated thoughts", &[]),
        ];
        let sources = build_candidates(&entries, None, &dd_mm_yyyy());
        let result = filter_candidates(&sources, "naval");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 1);
        assert!(result[0].snippet.contains("naval"));
    }

    #[test]
    fn filter_prefers_body_match_over_title_match() {
        let entries = vec![
            entry(1, "alpha", "completely different content", &[]),
            entry(2, "unrelated", "alpha appears in this body text", &[]),
        ];
        let sources = build_candidates(&entries, None, &dd_mm_yyyy());
        let result = filter_candidates(&sources, "alpha");
        assert_eq!(result[0].id, 2, "body match should outrank title match");
    }

    #[test]
    fn filter_matches_via_tags_when_body_misses() {
        let entries = vec![entry(1, "", "no relevant content", &["naval"])];
        let sources = build_candidates(&entries, None, &dd_mm_yyyy());
        let result = filter_candidates(&sources, "naval");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 1);
    }

    #[test]
    fn filter_handles_multi_word_query() {
        let entries = vec![
            entry(1, "", "alpha beta gamma", &[]),
            entry(2, "", "alpha gamma", &[]),
        ];
        let sources = build_candidates(&entries, None, &dd_mm_yyyy());
        let result = filter_candidates(&sources, "alpha beta");
        assert!(!result.is_empty());
        assert_eq!(result[0].id, 1);
    }

    #[test]
    fn format_token_shape() {
        assert_eq!(format_mention_token(246), "@id:246");
    }
}
