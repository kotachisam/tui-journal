use std::collections::BTreeSet;

use backend::Entry;

const KNOWN_CATEGORIES: &[&str] = &["journal", "post"];

/// The display order of categories: hard-coded known categories first, then
/// any user-introduced categories sorted alphabetically. Used to drive the
/// tab bar at the top of the entries list and the `[`/`]` cycle.
pub fn ordered_categories(entries: &[Entry]) -> Vec<String> {
    let known: Vec<String> = KNOWN_CATEGORIES.iter().map(|s| (*s).to_owned()).collect();
    let user: BTreeSet<String> = entries
        .iter()
        .map(|e| e.category.clone())
        .filter(|c| !KNOWN_CATEGORIES.contains(&c.as_str()))
        .collect();

    let mut out = known;
    out.extend(user);
    out
}

/// Returns the next category in cycle order, wrapping. `step` is +1 for
/// "next" (e.g., `]`) and -1 for "prev" (e.g., `[`). Returns `current`
/// unchanged if there are no other categories or `current` isn't in the
/// list (shouldn't happen under normal flow but kept defensive).
pub fn cycle_category(categories: &[String], current: &str, step: i32) -> String {
    if categories.is_empty() {
        return current.to_owned();
    }
    let pos = categories.iter().position(|c| c == current).unwrap_or(0);
    let len = categories.len() as i32;
    let next = ((pos as i32 + step).rem_euclid(len)) as usize;
    categories[next].clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn entry_with_category(id: u32, category: &str) -> Entry {
        Entry {
            id,
            date: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            title: String::new(),
            content: String::new(),
            tags: vec![],
            priority: None,
            category: category.to_owned(),
            sync_provider: None,
            external_id: None,
            last_synced_at: None,
            deleted_at: None,
            updated_at: None,
            source_last_edited_at: None,
        }
    }

    #[test]
    fn ordered_includes_both_known_categories_with_no_entries() {
        let categories = ordered_categories(&[]);
        assert_eq!(categories, vec!["journal", "post"]);
    }

    #[test]
    fn ordered_keeps_known_first_then_user_sorted() {
        let entries = vec![
            entry_with_category(1, "thread"),
            entry_with_category(2, "quip"),
            entry_with_category(3, "journal"),
            entry_with_category(4, "post"),
        ];
        let categories = ordered_categories(&entries);
        assert_eq!(categories, vec!["journal", "post", "quip", "thread"]);
    }

    #[test]
    fn cycle_next_wraps_to_first() {
        let cats: Vec<String> = vec!["journal", "post"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(cycle_category(&cats, "post", 1), "journal");
    }

    #[test]
    fn cycle_prev_wraps_to_last() {
        let cats: Vec<String> = vec!["journal", "post"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(cycle_category(&cats, "journal", -1), "post");
    }

    #[test]
    fn cycle_with_unknown_current_starts_from_zero() {
        let cats: Vec<String> = vec!["journal", "post"]
            .into_iter()
            .map(String::from)
            .collect();
        // Unknown "current" → fallback to position 0; +1 → "post"
        assert_eq!(cycle_category(&cats, "ghost", 1), "post");
    }
}
