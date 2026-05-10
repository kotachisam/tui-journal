use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;

use backend::Entry;

use crate::settings::DateFormat;

pub(super) const MAX_MENTION_SUGGESTIONS: usize = 8;
pub(super) const SNIPPET_WINDOW_CHARS: usize = 70;
const SNIPPET_LEAD_CONTEXT: usize = 12;
const BODY_SCORE_WEIGHT: i64 = 2;

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

fn extract_snippet(body_flat: &str, body_first_line: &str, query: &str) -> (String, Vec<usize>) {
    let chars: Vec<char> = body_flat.chars().collect();
    if chars.is_empty() || query.trim().is_empty() {
        return (
            truncate_chars(body_first_line, SNIPPET_WINDOW_CHARS),
            Vec::new(),
        );
    }

    let matcher = SkimMatcherV2::default().smart_case();
    let Some((_, indices)) = matcher.fuzzy_indices(body_flat, query.trim()) else {
        return (
            truncate_chars(body_first_line, SNIPPET_WINDOW_CHARS),
            Vec::new(),
        );
    };
    let Some(&first_match) = indices.first() else {
        return (
            truncate_chars(body_first_line, SNIPPET_WINDOW_CHARS),
            Vec::new(),
        );
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
    fn flattens_whitespace_to_single_spaces() {
        assert_eq!(flatten_whitespace("foo\n\n  bar\tbaz "), "foo bar baz");
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
        assert!(
            snippet.contains("naval"),
            "snippet missing match: {snippet}"
        );
        assert!(
            snippet.starts_with('…'),
            "expected leading ellipsis: {snippet}"
        );
        let char_count = snippet.chars().count();
        assert!(
            char_count <= SNIPPET_WINDOW_CHARS + 2,
            "too long: {char_count}"
        );
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
            assert!(
                i > 0,
                "matched char index {i} should not include the ellipsis"
            );
        }
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
}
