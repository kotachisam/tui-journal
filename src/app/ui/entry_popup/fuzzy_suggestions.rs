use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Clear, List, ListItem, ListState},
};

const MAX_SUGGESTIONS: usize = 8;

/// Reusable state for any field-level fuzzy autocomplete overlay (tags,
/// category, future: title-history, slug-suggestions, etc.). Stores the
/// scored matches and the currently-highlighted selection.
pub struct SuggestionState {
    matches: Vec<(i64, String)>,
    selected: usize,
}

impl SuggestionState {
    /// Builds a state from a query and a candidate set. Returns `None` when
    /// there are no matches (caller should treat that as "hide overlay").
    ///
    /// `exclude_exact` excludes a candidate that exactly matches the query
    /// (case-insensitive, trimmed) — useful for single-value fields like
    /// "category" where the user has already typed the canonical value, so
    /// suggesting it back is just noise.
    pub fn build(query: &str, candidates: &[String], exclude_exact: bool) -> Option<Self> {
        let matches = compute_matches(query, candidates, exclude_exact);
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

    pub fn selected_value(&self) -> Option<&str> {
        self.matches.get(self.selected).map(|(_, v)| v.as_str())
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

/// Pure scoring + filtering. Public so consumers can call it directly without
/// constructing a `SuggestionState` (e.g., when they want to inspect matches
/// without owning the navigation state).
pub fn compute_matches(
    query: &str,
    candidates: &[String],
    exclude_exact: bool,
) -> Vec<(i64, String)> {
    let q = query.trim();
    if q.is_empty() {
        return Vec::new();
    }
    let q_lc = q.to_lowercase();
    let matcher = SkimMatcherV2::default();

    let mut scored: Vec<(i64, String)> = candidates
        .iter()
        .filter_map(|c| {
            let c_lc = c.to_lowercase();
            if exclude_exact && c_lc == q_lc {
                return None;
            }
            // Match against lowercased forms on both sides so the user can
            // type "PO" or "po" or "Po" and get the same hits — Skim's
            // smart-case treats uppercase as case-sensitive otherwise.
            matcher
                .fuzzy_match(&c_lc, &q_lc)
                .map(|score| (score, c.clone()))
        })
        .collect();
    scored.sort_by_key(|m| std::cmp::Reverse(m.0));
    scored.truncate(MAX_SUGGESTIONS);
    scored
}

/// Renders a bordered list overlay anchored to `anchor_area`. Drawn below
/// when there's room; falls back to above; clips when the terminal is tight.
pub fn render_overlay(frame: &mut Frame, anchor_area: Rect, state: &SuggestionState, title: &str) {
    let matches = state.matches();
    if matches.is_empty() {
        return;
    }

    let frame_area = frame.area();
    let desired_height = (matches.len() as u16) + 2; // +2 for borders
    let below_y = anchor_area.y + anchor_area.height;
    let space_below = frame_area.height.saturating_sub(below_y);

    let (overlay_y, overlay_height) = if space_below >= desired_height {
        (below_y, desired_height)
    } else if anchor_area.y >= desired_height {
        (anchor_area.y - desired_height, desired_height)
    } else {
        // Tight fit — clip below, but keep at least 3 rows so the
        // overlay is still useful.
        (below_y, space_below.max(3).min(desired_height))
    };

    let overlay_width = anchor_area.width.min(60);
    let overlay_area = Rect {
        x: anchor_area.x,
        y: overlay_y,
        width: overlay_width,
        height: overlay_height,
    };

    let items: Vec<ListItem> = matches
        .iter()
        .map(|(_, value)| ListItem::new(value.as_str()))
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(Color::LightBlue).fg(Color::Black));

    let mut list_state = ListState::default();
    list_state.select(Some(state.selected_index()));

    frame.render_widget(Clear, overlay_area);
    frame.render_stateful_widget(list, overlay_area, &mut list_state);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cats() -> Vec<String> {
        vec!["journal".into(), "post".into(), "thread".into()]
    }

    #[test]
    fn empty_query_returns_no_matches() {
        assert!(compute_matches("", &cats(), false).is_empty());
    }

    #[test]
    fn includes_exact_when_not_excluded() {
        let m = compute_matches("post", &cats(), false);
        assert!(m.iter().any(|(_, v)| v == "post"));
    }

    #[test]
    fn excludes_exact_when_flag_set() {
        let m = compute_matches("post", &cats(), true);
        assert!(m.iter().all(|(_, v)| v != "post"));
    }

    #[test]
    fn case_insensitive_query() {
        let m = compute_matches("PO", &cats(), false);
        assert!(m.iter().any(|(_, v)| v == "post"));
    }

    #[test]
    fn caps_at_eight() {
        let many: Vec<String> = (0..50).map(|i| format!("item-{i}")).collect();
        let m = compute_matches("item", &many, false);
        assert!(m.len() <= MAX_SUGGESTIONS);
    }

    #[test]
    fn build_returns_none_when_empty() {
        assert!(SuggestionState::build("zzznoresult", &cats(), false).is_none());
    }

    #[test]
    fn navigation_clamps_at_boundaries() {
        let mut state = SuggestionState::build("o", &cats(), false).expect("matches");
        let count = state.matches().len();
        for _ in 0..(count + 5) {
            state.move_down();
        }
        assert_eq!(state.selected_index(), count - 1);
        for _ in 0..10 {
            state.move_up();
        }
        assert_eq!(state.selected_index(), 0);
    }
}
