use anyhow::Result;
use backend::DataProvider;

use super::{App, UICommand, UIComponents, editor::MentionFollow};

pub(super) const BACKSTACK_CAP: usize = 50;

#[derive(Default)]
pub(super) struct Backstack {
    stack: Vec<u32>,
}

impl Backstack {
    pub(super) fn push(&mut self, id: u32, cap: usize) {
        if self.stack.last() == Some(&id) {
            return;
        }
        self.stack.push(id);
        if self.stack.len() > cap {
            self.stack.remove(0);
        }
    }

    pub(super) fn pop_valid<F: Fn(u32) -> bool>(&mut self, exists: F) -> Option<u32> {
        while let Some(id) = self.stack.pop() {
            if exists(id) {
                return Some(id);
            }
        }
        None
    }
}

impl UIComponents<'_> {
    pub(super) fn push_backstack(&mut self, entry_id: Option<u32>) {
        let Some(id) = entry_id else { return };
        self.backstack.push(id, BACKSTACK_CAP);
    }

    pub(super) fn pop_backstack<D: DataProvider>(&mut self, app: &mut App<D>) {
        let valid_ids: std::collections::HashSet<u32> = app
            .entries
            .iter()
            .filter(|e| e.deleted_at.is_none())
            .map(|e| e.id)
            .collect();
        if let Some(id) = self.backstack.pop_valid(|id| valid_ids.contains(&id)) {
            self.set_current_entry(Some(id), app);
        }
    }

    pub(super) async fn follow_mention<D: DataProvider>(
        &mut self,
        target: MentionFollow,
        app: &mut App<D>,
    ) -> Result<()> {
        let exists = app
            .entries
            .iter()
            .any(|e| e.id == target.id && e.deleted_at.is_none());
        if !exists {
            self.show_toast(format!("Entry @id:{} not found", target.id));
            return Ok(());
        }
        if self.has_unsaved() {
            self.pending_mention_target = Some(target);
            self.show_unsaved_msg_box(Some(UICommand::FollowMention));
        } else {
            let id = target.id;
            self.push_backstack(app.current_entry_id);
            self.set_current_entry(Some(id), app);
            self.apply_mention_anchor(target.anchor.as_deref(), app);
        }
        Ok(())
    }

    pub(super) fn apply_mention_anchor<D: DataProvider>(
        &mut self,
        anchor: Option<&str>,
        app: &App<D>,
    ) {
        let Some(anchor) = anchor.filter(|s| !s.is_empty()) else {
            return;
        };
        let Some(entry) = app.get_current_entry() else {
            return;
        };
        match super::editor::mention::find_anchor_line(&entry.content, anchor) {
            Some(line) => self.editor.set_preview_scroll(line),
            None => self.show_toast(format!(
                "Anchor \"{anchor}\" not found in target — content may have changed"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Backstack;

    #[test]
    fn push_then_pop_returns_pushed_id() {
        let mut b = Backstack::default();
        b.push(7, 50);
        assert_eq!(b.pop_valid(|_| true), Some(7));
    }

    #[test]
    fn pop_on_empty_returns_none() {
        let mut b = Backstack::default();
        assert_eq!(b.pop_valid(|_| true), None);
    }

    #[test]
    fn consecutive_same_id_dedupes() {
        let mut b = Backstack::default();
        b.push(7, 50);
        b.push(7, 50);
        assert_eq!(b.pop_valid(|_| true), Some(7));
        assert_eq!(b.pop_valid(|_| true), None);
    }

    #[test]
    fn cap_evicts_oldest() {
        let mut b = Backstack::default();
        for id in 1..=4u32 {
            b.push(id, 3);
        }
        assert_eq!(b.pop_valid(|_| true), Some(4));
        assert_eq!(b.pop_valid(|_| true), Some(3));
        assert_eq!(b.pop_valid(|_| true), Some(2));
        assert_eq!(b.pop_valid(|_| true), None);
    }

    #[test]
    fn pop_skips_deleted_continues_to_valid() {
        let mut b = Backstack::default();
        b.push(1, 50);
        b.push(2, 50);
        b.push(3, 50);
        let valid_ids = [1u32];
        assert_eq!(b.pop_valid(|id| valid_ids.contains(&id)), Some(1));
        assert_eq!(b.pop_valid(|_| true), None);
    }

    #[test]
    fn pop_empties_when_all_deleted() {
        let mut b = Backstack::default();
        b.push(1, 50);
        b.push(2, 50);
        assert_eq!(b.pop_valid(|_| false), None);
        assert_eq!(b.pop_valid(|_| true), None);
    }

    #[test]
    fn push_after_dedupe_still_evicts_at_cap() {
        let mut b = Backstack::default();
        b.push(1, 3);
        b.push(2, 3);
        b.push(2, 3);
        b.push(3, 3);
        b.push(4, 3);
        assert_eq!(b.pop_valid(|_| true), Some(4));
        assert_eq!(b.pop_valid(|_| true), Some(3));
        assert_eq!(b.pop_valid(|_| true), Some(2));
        assert_eq!(b.pop_valid(|_| true), None);
    }
}
