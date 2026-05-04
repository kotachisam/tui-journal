use crate::app::ui::inline_completer::InlineCompleterState;

use super::candidates::MentionCandidate;

pub type MentionState = InlineCompleterState<MentionCandidate>;

#[cfg(test)]
mod tests {
    use super::*;

    fn make_candidates(n: usize) -> Vec<MentionCandidate> {
        (0..n)
            .map(|i| MentionCandidate {
                id: (i + 1) as u32,
                snippet: format!("snippet {i}"),
                match_indices: Vec::new(),
                date_display: format!("2026-05-{:02}", i + 1),
            })
            .collect()
    }

    #[test]
    fn new_starts_at_zero_with_empty_query_and_candidates() {
        let state = MentionState::new(3, 7);
        assert_eq!(state.anchor_line, 3);
        assert_eq!(state.anchor_col, 7);
        assert_eq!(state.selected_idx, 0);
        assert!(state.query.is_empty());
        assert!(state.candidates.is_empty());
    }

    #[test]
    fn move_up_at_zero_stays_at_zero() {
        let mut state = MentionState::new(0, 0);
        state.candidates = make_candidates(3);
        state.move_up();
        assert_eq!(state.selected_idx, 0);
    }

    #[test]
    fn move_down_with_no_candidates_stays_at_zero() {
        let mut state = MentionState::new(0, 0);
        state.move_down();
        assert_eq!(state.selected_idx, 0);
    }

    #[test]
    fn move_down_advances_within_bounds() {
        let mut state = MentionState::new(0, 0);
        state.candidates = make_candidates(3);
        state.move_down();
        assert_eq!(state.selected_idx, 1);
        state.move_down();
        assert_eq!(state.selected_idx, 2);
    }

    #[test]
    fn move_down_clamps_at_last_index() {
        let mut state = MentionState::new(0, 0);
        state.candidates = make_candidates(2);
        state.move_down();
        state.move_down();
        state.move_down();
        assert_eq!(state.selected_idx, 1);
    }

    #[test]
    fn move_up_decrements_within_bounds() {
        let mut state = MentionState::new(0, 0);
        state.candidates = make_candidates(3);
        state.selected_idx = 2;
        state.move_up();
        assert_eq!(state.selected_idx, 1);
        state.move_up();
        assert_eq!(state.selected_idx, 0);
    }

    #[test]
    fn selected_returns_none_when_empty() {
        let state = MentionState::new(0, 0);
        assert!(state.selected().is_none());
    }

    #[test]
    fn selected_returns_candidate_at_idx() {
        let mut state = MentionState::new(0, 0);
        state.candidates = make_candidates(3);
        state.selected_idx = 1;
        assert_eq!(state.selected().map(|c| c.id), Some(2));
    }

    #[test]
    fn selected_returns_none_when_idx_out_of_bounds() {
        let mut state = MentionState::new(0, 0);
        state.candidates = make_candidates(2);
        state.selected_idx = 5;
        assert!(state.selected().is_none());
    }
}
