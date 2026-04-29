//! Category-specific helpers for the entry popup's category-field autocomplete.
//!
//! Single-value field semantics: the active query is the entire trimmed
//! content (no comma parsing), and applying a selected suggestion replaces
//! the field outright. The fuzzy matching, navigation state, and overlay
//! render all live in `super::fuzzy_suggestions`.

use super::fuzzy_suggestions::SuggestionState;

/// Returns the active query for the category field — the whole trimmed
/// content. `None` when empty so the caller knows to hide the overlay.
pub fn active_query(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Builds the autocomplete state for the category field. Excludes any
/// candidate whose lowercase matches the query (the user has already typed
/// the canonical value; suggesting it back is noise).
pub fn build_state(line: &str, candidates: &[String]) -> Option<SuggestionState> {
    let query = active_query(line)?;
    SuggestionState::build(query, candidates, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cats() -> Vec<String> {
        vec![
            "journal".into(),
            "post".into(),
            "thread".into(),
            "quip".into(),
        ]
    }

    #[test]
    fn empty_line_yields_no_query() {
        assert_eq!(active_query(""), None);
        assert_eq!(active_query("   "), None);
    }

    #[test]
    fn whitespace_trimmed() {
        assert_eq!(active_query("  post  "), Some("post"));
    }

    #[test]
    fn build_state_returns_none_when_field_empty() {
        assert!(build_state("", &cats()).is_none());
    }

    #[test]
    fn build_state_excludes_exact_match() {
        let state = build_state("post", &cats());
        if let Some(state) = state {
            assert!(state.matches().iter().all(|(_, v)| v != "post"));
        }
        // either None (no fuzzy hits beyond exact) or Some that excludes "post"
    }

    #[test]
    fn build_state_returns_partial_matches() {
        let state = build_state("po", &cats()).expect("po should match post");
        assert!(state.matches().iter().any(|(_, v)| v == "post"));
    }
}
