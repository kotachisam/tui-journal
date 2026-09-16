use std::sync::OnceLock;

use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;

use crate::app::ui::editor::width::str_cols;

pub const MAX_EMOJI_SUGGESTIONS: usize = 8;

const ZWJ: char = '\u{200d}';

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmojiCandidate {
    pub emoji: &'static str,
    pub shortcode: &'static str,
    pub name: &'static str,
    pub match_indices: Vec<usize>,
}

fn table() -> &'static [EmojiCandidate] {
    static TABLE: OnceLock<Vec<EmojiCandidate>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut out: Vec<EmojiCandidate> = emojis::iter()
            .filter(|e| !e.as_str().contains(ZWJ))
            .filter(|e| str_cols(e.as_str()) == 2)
            .filter_map(|e| {
                e.shortcode().map(|shortcode| EmojiCandidate {
                    emoji: e.as_str(),
                    shortcode,
                    name: e.name(),
                    match_indices: Vec::new(),
                })
            })
            .collect();
        out.sort_by_key(|c| (c.shortcode.len(), c.shortcode));
        out
    })
}

pub fn exact_match(shortcode: &str) -> Option<&'static EmojiCandidate> {
    table().iter().find(|c| c.shortcode == shortcode)
}

pub fn filter_candidates(query: &str) -> Vec<EmojiCandidate> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let matcher = SkimMatcherV2::default().smart_case();
    let mut scored: Vec<(i64, &EmojiCandidate, Vec<usize>)> = table()
        .iter()
        .filter_map(|c| {
            matcher
                .fuzzy_indices(c.shortcode, trimmed)
                .map(|(score, indices)| (score, c, indices))
        })
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.shortcode.cmp(b.1.shortcode)));

    scored
        .into_iter()
        .take(MAX_EMOJI_SUGGESTIONS)
        .map(|(_, c, match_indices)| EmojiCandidate {
            match_indices,
            ..c.clone()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_not_empty() {
        assert!(table().len() > 500);
    }

    #[test]
    fn every_candidate_is_exactly_two_columns_wide() {
        for c in table() {
            assert_eq!(str_cols(c.emoji), 2, "{} ({})", c.shortcode, c.emoji);
        }
    }

    #[test]
    fn zwj_sequences_are_excluded() {
        assert!(!table().iter().any(|c| c.emoji.contains(ZWJ)));
    }

    #[test]
    fn empty_query_yields_nothing() {
        assert!(filter_candidates("").is_empty());
        assert!(filter_candidates("   ").is_empty());
    }

    #[test]
    fn exact_shortcode_ranks_first() {
        let results = filter_candidates("tada");
        assert_eq!(results.first().map(|c| c.shortcode), Some("tada"));
        assert_eq!(results.first().map(|c| c.emoji), Some("🎉"));
    }

    #[test]
    fn results_are_capped() {
        assert!(filter_candidates("a").len() <= MAX_EMOJI_SUGGESTIONS);
    }

    #[test]
    fn fuzzy_query_matches_subsequence() {
        let results = filter_candidates("rckt");
        assert!(results.iter().any(|c| c.shortcode == "rocket"));
    }

    #[test]
    fn match_indices_point_into_the_shortcode() {
        let results = filter_candidates("tada");
        let first = results.first().expect("a result");
        for &i in &first.match_indices {
            assert!(i < first.shortcode.chars().count());
        }
    }

    #[test]
    fn exact_match_finds_known_shortcode() {
        assert_eq!(exact_match("rocket").map(|c| c.emoji), Some("🚀"));
        assert!(exact_match("definitely_not_an_emoji").is_none());
    }

    #[test]
    fn nonsense_query_yields_nothing() {
        assert!(filter_candidates("qqzzxxjjkk").is_empty());
    }
}
