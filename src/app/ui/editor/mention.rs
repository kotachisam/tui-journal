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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedMention {
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
        let prefix_match = chars[i..i + prefix_len].iter().zip(PREFIX.chars()).all(|(a, b)| *a == b);
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

pub(super) fn parse_anchor_suffix_buffer(
    chars: &[char],
    start: usize,
) -> (usize, Option<String>) {
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocMention {
    pub line_idx: usize,
    pub char_range: std::ops::Range<usize>,
    pub id: u32,
    pub anchor: Option<String>,
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

pub fn render_mention_label(entry: &Entry, date_format: &DateFormat) -> String {
    let title = entry.title.trim();
    let date_str = date_format.display(&entry.date);
    if title.is_empty() {
        date_str
    } else {
        format!("{title} ({date_str})")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedMention {
    pub line_idx: usize,
    pub source_char_range: std::ops::Range<usize>,
    pub label_char_range: std::ops::Range<usize>,
    pub id: u32,
    pub label: String,
    pub missing: bool,
    pub anchor: Option<String>,
}

pub fn substitute_mentions(
    content: &str,
    entries: &[Entry],
    date_format: &DateFormat,
) -> (String, Vec<RenderedMention>) {
    let doc_mentions = parse_mentions_in_doc(content);
    if doc_mentions.is_empty() {
        return (content.to_string(), Vec::new());
    }

    let mut by_line: std::collections::BTreeMap<usize, Vec<&DocMention>> =
        std::collections::BTreeMap::new();
    for m in &doc_mentions {
        by_line.entry(m.line_idx).or_default().push(m);
    }

    let mut rendered_lines: Vec<String> = Vec::new();
    let mut rendered_mentions: Vec<RenderedMention> = Vec::new();

    for (line_idx, line) in content.lines().enumerate() {
        let Some(mentions) = by_line.get(&line_idx) else {
            rendered_lines.push(line.to_string());
            continue;
        };
        let chars: Vec<char> = line.chars().collect();
        let mut new_line = String::new();
        let mut cursor: usize = 0;
        let mut new_char_pos: usize = 0;
        for m in mentions {
            let pre: String = chars[cursor..m.char_range.start].iter().collect();
            new_line.push_str(&pre);
            new_char_pos += m.char_range.start - cursor;

            let entry = entries
                .iter()
                .find(|e| e.id == m.id && e.deleted_at.is_none());
            let (label, missing) = match entry {
                Some(e) => (render_mention_label(e, date_format), false),
                None => {
                    let raw: String = chars[m.char_range.clone()].iter().collect();
                    (raw, true)
                }
            };
            let label_char_count = label.chars().count();
            let label_char_start = new_char_pos;
            new_line.push_str(&label);
            new_char_pos += label_char_count;

            rendered_mentions.push(RenderedMention {
                line_idx,
                source_char_range: m.char_range.clone(),
                label_char_range: label_char_start..label_char_start + label_char_count,
                id: m.id,
                label,
                missing,
                anchor: m.anchor.clone(),
            });
            cursor = m.char_range.end;
        }
        let tail: String = chars[cursor..].iter().collect();
        new_line.push_str(&tail);
        rendered_lines.push(new_line);
    }

    let mut rendered_content = rendered_lines.join("\n");
    if content.ends_with('\n') {
        rendered_content.push('\n');
    }
    (rendered_content, rendered_mentions)
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
    content.lines().enumerate().find_map(|(idx, line)| {
        line.to_lowercase()
            .contains(&needle)
            .then_some(idx as u16)
    })
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
        let result =
            parse_mentions_in_line("@id:1(\"naval\") and @id:5(\"books\")");
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
    fn find_anchor_line_basic_hit() {
        assert_eq!(find_anchor_line("foo\nbar naval baz\nqux", "naval"), Some(1));
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
    fn render_label_with_title() {
        let e = entry(1, "My Title", "body", &[]);
        assert_eq!(render_mention_label(&e, &dd_mm_yyyy()), "My Title (15-05-2026)");
    }

    #[test]
    fn render_label_blank_title_falls_back_to_date_only() {
        let e = entry(1, "", "body", &[]);
        assert_eq!(render_mention_label(&e, &dd_mm_yyyy()), "15-05-2026");
    }

    #[test]
    fn render_label_whitespace_title_treated_as_blank() {
        let e = entry(1, "   ", "body", &[]);
        assert_eq!(render_mention_label(&e, &dd_mm_yyyy()), "15-05-2026");
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

    #[test]
    fn substitute_no_mentions_returns_content_unchanged() {
        let content = "no tokens here\nsecond line";
        let (out, mentions) = substitute_mentions(content, &[], &dd_mm_yyyy());
        assert_eq!(out, content);
        assert!(mentions.is_empty());
    }

    #[test]
    fn substitute_present_entry_inserts_label() {
        let entries = vec![entry(159, "My Day", "body", &[])];
        let content = "see @id:159 today";
        let (out, mentions) = substitute_mentions(content, &entries, &dd_mm_yyyy());
        assert_eq!(out, "see My Day (15-05-2026) today");
        assert_eq!(mentions.len(), 1);
        assert!(!mentions[0].missing);
        assert_eq!(mentions[0].label, "My Day (15-05-2026)");
        assert_eq!(mentions[0].source_char_range, 4..11);
        assert_eq!(mentions[0].label_char_range, 4..23);
    }

    #[test]
    fn substitute_missing_entry_keeps_raw_token_with_missing_flag() {
        let content = "see @id:99999 here";
        let (out, mentions) = substitute_mentions(content, &[], &dd_mm_yyyy());
        assert_eq!(out, "see @id:99999 here");
        assert_eq!(mentions.len(), 1);
        assert!(mentions[0].missing);
        assert_eq!(mentions[0].label, "@id:99999");
    }

    #[test]
    fn substitute_blank_title_uses_date_only() {
        let entries = vec![entry(7, "", "body", &[])];
        let content = "ref @id:7";
        let (out, mentions) = substitute_mentions(content, &entries, &dd_mm_yyyy());
        assert_eq!(out, "ref 15-05-2026");
        assert_eq!(mentions[0].label, "15-05-2026");
    }

    #[test]
    fn substitute_handles_multiple_tokens_on_same_line() {
        let entries = vec![
            entry(1, "A", "x", &[]),
            entry(2, "B", "y", &[]),
        ];
        let content = "@id:1 then @id:2";
        let (out, mentions) = substitute_mentions(content, &entries, &dd_mm_yyyy());
        assert_eq!(out, "A (15-05-2026) then B (15-05-2026)");
        assert_eq!(mentions.len(), 2);
        assert_eq!(mentions[0].id, 1);
        assert_eq!(mentions[1].id, 2);
        let chars: Vec<char> = out.chars().collect();
        let label0: String = chars[mentions[0].label_char_range.clone()].iter().collect();
        let label1: String = chars[mentions[1].label_char_range.clone()].iter().collect();
        assert_eq!(label0, "A (15-05-2026)");
        assert_eq!(label1, "B (15-05-2026)");
    }

    #[test]
    fn substitute_skips_tokens_inside_code_fence() {
        let entries = vec![entry(1, "Title", "body", &[])];
        let content = "ref @id:1\n```\n@id:1 inside\n```\nend";
        let (out, mentions) = substitute_mentions(content, &entries, &dd_mm_yyyy());
        assert_eq!(out, "ref Title (15-05-2026)\n```\n@id:1 inside\n```\nend");
        assert_eq!(mentions.len(), 1);
    }

    #[test]
    fn substitute_treats_soft_deleted_entry_as_missing() {
        let mut deleted = entry(5, "Gone", "body", &[]);
        deleted.deleted_at = Some(Utc.with_ymd_and_hms(2026, 5, 14, 0, 0, 0).unwrap());
        let entries = vec![deleted];
        let content = "see @id:5";
        let (_, mentions) = substitute_mentions(content, &entries, &dd_mm_yyyy());
        assert_eq!(mentions.len(), 1);
        assert!(mentions[0].missing);
    }
}
