use anyhow::Context;
use backend::{DataProvider, Entry, EntryDraft, EntryRevision, activity_actions};
use chrono::{DateTime, Utc};

use super::App;
use super::history::HistoryStack;

/// Represents what part of [`Entry`] will be changed.
pub(super) enum EntryEditPart {
    /// The attributes (Name, Date...) of the entry will be changed
    Attributes,
    /// The content of the entry will be changed.
    Content,
}

impl<D> App<D>
where
    D: DataProvider,
{
    /// Get entries that meet the filter criteria (if any) AND match the
    /// currently-active category tab.
    pub fn get_active_entries(&self) -> impl DoubleEndedIterator<Item = &Entry> {
        self.entries
            .iter()
            .filter(|entry| !self.filtered_out_entries.contains(&entry.id))
            .filter(|entry| entry.category == self.view_category)
    }

    pub fn get_entry(&self, entry_id: u32) -> Option<&Entry> {
        self.get_active_entries().find(|e| e.id == entry_id)
    }

    /// Gives a mutable reference to the entry with given id if exist, registering it in
    /// the history according to the given [`EntryEditPart`] and [`HistoryStack`]
    pub(super) fn get_entry_mut(
        &mut self,
        entry_id: u32,
        edit_target: EntryEditPart,
        history_target: HistoryStack,
    ) -> Option<&mut Entry> {
        let entry_opt = self.entries.iter_mut().find(|e| e.id == entry_id);

        if let Some(entry) = entry_opt.as_ref() {
            match edit_target {
                EntryEditPart::Attributes => self
                    .history
                    .register_change_attributes(history_target, entry),
                EntryEditPart::Content => {
                    self.history.register_change_content(history_target, entry)
                }
            };
        }

        entry_opt
    }

    /// Gets' the selected entry currently.
    pub fn get_current_entry(&self) -> Option<&Entry> {
        self.current_entry_id
            .and_then(|id| self.get_active_entries().find(|entry| entry.id == id))
    }

    pub async fn load_entries(&mut self) -> anyhow::Result<()> {
        log::trace!("Loading entries");

        self.entries = self.data_provide.load_all_entries().await?;

        self.sort_entries();

        self.update_filtered_out_entries();

        self.update_colored_tags();

        Ok(())
    }

    pub async fn get_revisions(&self, entry_id: u32) -> anyhow::Result<Vec<EntryRevision>> {
        self.data_provide.get_revisions_for_entry(entry_id).await
    }

    pub(super) async fn log_activity_best_effort(
        &self,
        action: &'static str,
        entry_id: Option<u32>,
        details: Option<&str>,
    ) {
        if let Err(err) = self
            .data_provide
            .log_activity(action, entry_id, details)
            .await
        {
            log::warn!("Activity log write failed for '{action}': {err}");
        }
    }

    /// Overwrites the entry's user-editable fields with the revision's
    /// contents. Preserves id and sync metadata. The existing update_entry
    /// snapshot-before-write hook captures the pre-restore state as a new
    /// revision, so restores are themselves undoable via the same
    /// mechanism.
    pub async fn restore_from_revision(
        &mut self,
        entry_id: u32,
        revision: &EntryRevision,
    ) -> anyhow::Result<()> {
        let mut entry = self
            .entries
            .iter()
            .find(|e| e.id == entry_id)
            .cloned()
            .context("Entry not found in memory when restoring revision")?;

        entry.title = revision.title.clone();
        entry.date = revision.date;
        entry.content = revision.content.clone();
        entry.tags = revision.tags.clone();
        entry.priority = revision.priority;
        entry.updated_at = Some(Utc::now());

        self.data_provide.update_entry(entry.clone()).await?;

        self.log_activity_best_effort(
            activity_actions::ENTRY_RESTORED,
            Some(entry_id),
            Some(&entry.title),
        )
        .await;

        if let Some(in_mem) = self.entries.iter_mut().find(|e| e.id == entry_id) {
            *in_mem = entry;
        }

        self.sort_entries();
        self.update_filter();
        self.update_filtered_out_entries();
        self.update_colored_tags();

        Ok(())
    }

    pub async fn add_entry(
        &mut self,
        title: String,
        date: DateTime<Utc>,
        tags: Vec<String>,
        priority: Option<u32>,
        category: String,
    ) -> anyhow::Result<u32> {
        self.add_entry_intern(
            title,
            date,
            tags,
            priority,
            category,
            None,
            HistoryStack::Undo,
        )
        .await
    }

    pub async fn add_entry_with_content(
        &mut self,
        title: String,
        date: DateTime<Utc>,
        tags: Vec<String>,
        priority: Option<u32>,
        category: String,
        content: String,
    ) -> anyhow::Result<u32> {
        self.add_entry_intern(
            title,
            date,
            tags,
            priority,
            category,
            Some(content),
            HistoryStack::Undo,
        )
        .await
    }

    /// Creates an [`Entry`] from the given arguments, registering the change to the provided
    /// [`HistoryStack`].
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn add_entry_intern(
        &mut self,
        title: String,
        date: DateTime<Utc>,
        tags: Vec<String>,
        priority: Option<u32>,
        category: String,
        content: Option<String>,
        history_target: HistoryStack,
    ) -> anyhow::Result<u32> {
        log::trace!("Adding entry");

        let mut draft = EntryDraft::new(date, title, tags, priority).with_category(category);
        if let Some(content) = content {
            draft = draft.with_content(content);
        }

        let entry = self.data_provide.add_entry(draft).await?;
        let entry_id = entry.id;

        self.log_activity_best_effort(
            activity_actions::ENTRY_CREATED,
            Some(entry_id),
            Some(&entry.title),
        )
        .await;

        self.history.register_add(history_target, &entry);

        self.entries.push(entry);

        self.sort_entries();
        self.update_filtered_out_entries();
        self.update_colored_tags();

        Ok(entry_id)
    }

    /// Restores an [`Entry`] preserving its original id, registering the change
    /// to the provided [`HistoryStack`]. Used by undo to bring back an entry
    /// that was deleted (vs. `add_entry_intern` which assigns a fresh id).
    pub(super) async fn restore_entry_intern(
        &mut self,
        entry: Entry,
        history_target: HistoryStack,
    ) -> anyhow::Result<u32> {
        log::trace!("Restoring entry");

        let entry = self.data_provide.restore_entry(entry).await?;
        let entry_id = entry.id;

        self.log_activity_best_effort(
            activity_actions::ENTRY_RESTORED,
            Some(entry_id),
            Some(&entry.title),
        )
        .await;

        self.history.register_add(history_target, &entry);

        self.entries.push(entry);

        self.sort_entries();
        self.update_filtered_out_entries();
        self.update_colored_tags();

        Ok(entry_id)
    }

    /// Updates the attributes of the currently selected [`Entry`]
    pub async fn update_current_entry_attributes(
        &mut self,
        title: String,
        date: DateTime<Utc>,
        tags: Vec<String>,
        priority: Option<u32>,
        category: String,
    ) -> anyhow::Result<()> {
        let current_entry_id = self
            .current_entry_id
            .expect("Current entry id must have value when updating entry attributes");
        self.update_entry_attributes(
            current_entry_id,
            title,
            date,
            tags,
            priority,
            category,
            HistoryStack::Undo,
        )
        .await
    }

    /// Updates the attributes of the given [`Entry`], registering its state before the change on
    /// the given [`HistoryStack`]
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn update_entry_attributes(
        &mut self,
        entry_id: u32,
        title: String,
        date: DateTime<Utc>,
        tags: Vec<String>,
        priority: Option<u32>,
        category: String,
        history_target: HistoryStack,
    ) -> anyhow::Result<()> {
        log::trace!("Updating entry");

        assert!(self.current_entry_id.is_some());

        let mut candidate = self
            .entries
            .iter()
            .find(|entry| entry.id == entry_id)
            .cloned()
            .expect("Entry not found for id when updating attributes");

        candidate.title = title;
        candidate.date = date;
        candidate.tags = tags;
        candidate.priority = priority;
        candidate.category = category;
        candidate.updated_at = Some(Utc::now());

        let log_title = candidate.title.clone();

        let persisted_entry = self.data_provide.update_entry(candidate).await?;

        let entry = self
            .get_entry_mut(entry_id, EntryEditPart::Attributes, history_target)
            .expect("Updated entry must remain in the entries list");
        *entry = persisted_entry;

        self.log_activity_best_effort(
            activity_actions::ENTRY_UPDATED,
            Some(entry_id),
            Some(&log_title),
        )
        .await;

        self.sort_entries();

        self.update_filter();
        self.update_filtered_out_entries();
        self.update_colored_tags();

        Ok(())
    }

    /// Updates the content of the currently selected [`Entry`]
    pub async fn update_current_entry_content(
        &mut self,
        entry_content: String,
    ) -> anyhow::Result<()> {
        let current_entry_id = self
            .current_entry_id
            .expect("Current entry id must have value when updating entry content");
        self.update_entry_content(current_entry_id, entry_content, HistoryStack::Undo)
            .await
    }

    /// Update the content of the given [`Entry`], registering its previous content to the given
    /// [`HistoryStack`]
    pub async fn update_entry_content(
        &mut self,
        entry_id: u32,
        entry_content: String,
        history_target: HistoryStack,
    ) -> anyhow::Result<()> {
        log::trace!("Updating entry content");

        let entry = self
            .get_entry_mut(entry_id, EntryEditPart::Content, history_target)
            .expect("Entry not found for id when updating content");

        entry.content = entry_content;
        entry.updated_at = Some(Utc::now());

        let clone = entry.clone();
        let log_title = clone.title.clone();

        self.data_provide.update_entry(clone).await?;

        self.log_activity_best_effort(
            activity_actions::ENTRY_UPDATED,
            Some(entry_id),
            Some(&log_title),
        )
        .await;

        self.update_filtered_out_entries();

        Ok(())
    }

    pub async fn delete_entry(&mut self, entry_id: u32) -> anyhow::Result<()> {
        self.delete_entry_intern(entry_id, HistoryStack::Undo).await
    }

    /// Removes the given entry, registering it to the given [`HistoryStack`]
    pub async fn delete_entry_intern(
        &mut self,
        entry_id: u32,
        history_target: HistoryStack,
    ) -> anyhow::Result<()> {
        log::trace!("Deleting entry with id: {entry_id}");

        self.data_provide.remove_entry(entry_id).await?;
        let removed_entry = self
            .entries
            .iter()
            .position(|entry| entry.id == entry_id)
            .map(|index| self.entries.remove(index))
            .expect("entry must be in the entries list");

        self.log_activity_best_effort(
            activity_actions::ENTRY_DELETED,
            Some(entry_id),
            Some(&removed_entry.title),
        )
        .await;

        self.history.register_remove(history_target, removed_entry);

        self.update_filter();
        self.update_filtered_out_entries();
        self.update_colored_tags();

        Ok(())
    }
}
