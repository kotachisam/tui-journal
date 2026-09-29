use std::collections::HashMap;
use std::sync::OnceLock;

use crate::app::ui::editor::width::str_cols;

pub const MAX_EMOJI_SUGGESTIONS: usize = 8;

const ZWJ: char = '\u{200d}';
const MIN_TAG_STEM_CHARS: usize = 3;
const GEMOJI_TAGS: &str = include_str!("gemoji_tags.tsv");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmojiCandidate {
    pub emoji: &'static str,
    pub shortcode: &'static str,
    pub name: &'static str,
    pub tag: Option<&'static str>,
    pub match_indices: Vec<usize>,
}

struct EmojiEntry {
    emoji: &'static str,
    shortcodes: Vec<&'static str>,
    name: &'static str,
    tags: Vec<&'static str>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    Exact,
    Prefix,
    WordPrefix,
    TagExact,
    TagPrefix,
    TagStem,
}

struct Hit {
    tier: Tier,
    shortcode: &'static str,
    tag: Option<&'static str>,
    match_start: usize,
}

fn tag_index() -> HashMap<&'static str, Vec<&'static str>> {
    GEMOJI_TAGS
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(shortcode, tags)| (shortcode, tags.split(' ').collect()))
        .collect()
}

fn table() -> &'static [EmojiEntry] {
    static TABLE: OnceLock<Vec<EmojiEntry>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let tags = tag_index();
        let mut out: Vec<EmojiEntry> = emojis::iter()
            .filter(|e| !e.as_str().contains(ZWJ))
            .filter(|e| str_cols(e.as_str()) == 2)
            .filter(|e| e.shortcode().is_some())
            .map(|e| {
                let shortcodes: Vec<&'static str> = e.shortcodes().collect();
                let tags = shortcodes
                    .iter()
                    .find_map(|s| tags.get(s))
                    .cloned()
                    .unwrap_or_default();
                EmojiEntry {
                    emoji: e.as_str(),
                    shortcodes,
                    name: e.name(),
                    tags,
                }
            })
            .collect();
        out.sort_by_key(|e| (e.shortcodes[0].len(), e.shortcodes[0]));
        out
    })
}

fn word_prefix_start(shortcode: &str, query: &str) -> Option<usize> {
    shortcode
        .match_indices('_')
        .map(|(i, _)| i + 1)
        .find(|&start| shortcode[start..].starts_with(query))
}

fn shortcode_hit(shortcode: &'static str, query: &str) -> Option<Hit> {
    let (tier, match_start) = if shortcode == query {
        (Tier::Exact, 0)
    } else if shortcode.starts_with(query) {
        (Tier::Prefix, 0)
    } else {
        (Tier::WordPrefix, word_prefix_start(shortcode, query)?)
    };
    Some(Hit {
        tier,
        shortcode,
        tag: None,
        match_start,
    })
}

fn tag_hit(entry: &EmojiEntry, tag: &'static str, query: &str) -> Option<Hit> {
    let tier = if tag == query {
        Tier::TagExact
    } else if tag.starts_with(query) {
        Tier::TagPrefix
    } else if tag.len() >= MIN_TAG_STEM_CHARS && query.starts_with(tag) {
        Tier::TagStem
    } else {
        return None;
    };
    Some(Hit {
        tier,
        shortcode: entry.shortcodes[0],
        tag: Some(tag),
        match_start: 0,
    })
}

fn best_hit(entry: &EmojiEntry, query: &str) -> Option<Hit> {
    let shortcode_hits = entry
        .shortcodes
        .iter()
        .filter_map(|s| shortcode_hit(s, query));
    let tag_hits = entry.tags.iter().filter_map(|t| tag_hit(entry, t, query));
    shortcode_hits.chain(tag_hits).min_by_key(|h| h.tier)
}

impl Hit {
    fn into_candidate(self, entry: &EmojiEntry, query_len: usize) -> EmojiCandidate {
        let match_indices = match self.tag {
            Some(_) => Vec::new(),
            None => (self.match_start..self.match_start + query_len).collect(),
        };
        EmojiCandidate {
            emoji: entry.emoji,
            shortcode: self.shortcode,
            name: entry.name,
            tag: self.tag,
            match_indices,
        }
    }
}

pub fn exact_match(shortcode: &str) -> Option<EmojiCandidate> {
    table().iter().find_map(|entry| {
        entry
            .shortcodes
            .iter()
            .find(|&&s| s == shortcode)
            .map(|&s| EmojiCandidate {
                emoji: entry.emoji,
                shortcode: s,
                name: entry.name,
                tag: None,
                match_indices: Vec::new(),
            })
    })
}

pub fn filter_candidates(query: &str) -> Vec<EmojiCandidate> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return Vec::new();
    }

    let mut hits: Vec<(Hit, &EmojiEntry)> = table()
        .iter()
        .filter_map(|entry| best_hit(entry, &query).map(|hit| (hit, entry)))
        .collect();
    hits.sort_by_key(|(hit, _)| hit.tier);

    hits.into_iter()
        .take(MAX_EMOJI_SUGGESTIONS)
        .map(|(hit, entry)| hit.into_candidate(entry, query.len()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shortcodes_for(query: &str) -> Vec<&'static str> {
        filter_candidates(query)
            .iter()
            .map(|c| c.shortcode)
            .collect()
    }

    #[test]
    fn table_is_not_empty() {
        assert!(table().len() > 500);
    }

    #[test]
    fn every_candidate_is_exactly_two_columns_wide() {
        for e in table() {
            assert_eq!(str_cols(e.emoji), 2, "{} ({})", e.shortcodes[0], e.emoji);
        }
    }

    #[test]
    fn zwj_sequences_are_excluded() {
        assert!(!table().iter().any(|e| e.emoji.contains(ZWJ)));
    }

    #[test]
    fn vendored_tags_resolve_to_table_entries() {
        let tagged = table().iter().filter(|e| !e.tags.is_empty()).count();
        assert_eq!(tagged, 419);
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
    fn prefix_ranks_before_word_prefix() {
        let results = shortcodes_for("dog");
        let dog = results.iter().position(|&s| s == "dog").unwrap();
        let guide = results.iter().position(|&s| s == "guide_dog").unwrap();
        assert!(dog < guide);
    }

    #[test]
    fn word_prefix_matches_inside_a_shortcode() {
        assert!(shortcodes_for("pushing").contains(&"leftwards_pushing_hand"));
    }

    #[test]
    fn scattered_letters_do_not_match() {
        assert!(shortcodes_for("rckt").is_empty());
        assert!(!shortcodes_for("haha").contains(&"handshake"));
        assert!(!shortcodes_for("sad").contains(&"sandwich"));
    }

    #[test]
    fn mid_word_substrings_do_not_match() {
        assert!(!shortcodes_for("ock").contains(&"rocket"));
    }

    #[test]
    fn query_is_case_insensitive() {
        assert_eq!(shortcodes_for("TADA").first(), Some(&"tada"));
    }

    #[test]
    fn aliases_are_searchable() {
        let results = filter_candidates("thumbsup");
        assert_eq!(results.first().map(|c| c.emoji), Some("👍"));
        assert_eq!(results.first().map(|c| c.shortcode), Some("thumbsup"));
    }

    #[test]
    fn tag_finds_emoji_whose_shortcode_does_not_contain_the_query() {
        let results = filter_candidates("haha");
        let emoji: Vec<&str> = results.iter().map(|c| c.emoji).collect();
        assert_eq!(emoji, vec!["😃", "😆"]);
        assert!(results.iter().all(|c| c.tag == Some("haha")));
    }

    #[test]
    fn elongated_query_matches_its_tag_stem() {
        for query in ["hahah", "hahaha", "lolol"] {
            assert!(!filter_candidates(query).is_empty(), "{query}");
        }
        assert_eq!(
            filter_candidates("lolol").first().map(|c| c.shortcode),
            Some("rofl")
        );
    }

    #[test]
    fn shortcode_hits_rank_above_tag_hits() {
        let results = filter_candidates("laugh");
        assert_eq!(results.first().map(|c| c.shortcode), Some("laughing"));
        assert!(
            results
                .iter()
                .any(|c| c.shortcode == "rofl" && c.tag.is_some())
        );
    }

    #[test]
    fn match_indices_cover_the_matched_run() {
        let results = filter_candidates("pushing");
        let hit = results
            .iter()
            .find(|c| c.shortcode == "leftwards_pushing_hand")
            .unwrap();
        assert_eq!(hit.match_indices, (10..17).collect::<Vec<_>>());
    }

    #[test]
    fn tag_hits_carry_no_shortcode_highlight() {
        let results = filter_candidates("haha");
        assert!(results.first().unwrap().match_indices.is_empty());
    }

    #[test]
    fn exact_match_finds_known_shortcode() {
        assert_eq!(exact_match("rocket").map(|c| c.emoji), Some("🚀"));
        assert!(exact_match("definitely_not_an_emoji").is_none());
    }

    #[test]
    fn exact_match_accepts_aliases() {
        assert_eq!(exact_match("satisfied").map(|c| c.emoji), Some("😆"));
    }

    #[test]
    fn nonsense_query_yields_nothing() {
        assert!(filter_candidates("qqzzxxjjkk").is_empty());
    }
}
