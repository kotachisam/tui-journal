use chrono::{DateTime, Utc};

use serde::{Deserialize, Serialize};

#[cfg(feature = "json")]
mod json;
#[cfg(feature = "json")]
pub use json::JsonDataProvide;

#[cfg(feature = "sqlite")]
mod sqlite;
#[cfg(feature = "sqlite")]
pub use sqlite::SqliteDataProvide;

pub const TRANSFER_DATA_VERSION: u16 = 100;

pub mod activity_actions {
    pub const ENTRY_CREATED: &str = "entry_created";
    pub const ENTRY_UPDATED: &str = "entry_updated";
    pub const ENTRY_DELETED: &str = "entry_deleted";
    pub const ENTRY_RESTORED: &str = "entry_restored";
}

#[derive(Debug, thiserror::Error)]
pub enum ModifyEntryError {
    #[error("{0}")]
    ValidationError(String),
    #[error("{0}")]
    DataError(#[from] anyhow::Error),
}

// The warning can be suppressed since this will be used with the code base of this app only
#[allow(async_fn_in_trait)]
pub trait DataProvider {
    async fn load_all_entries(&self) -> anyhow::Result<Vec<Entry>>;
    async fn add_entry(&self, entry: EntryDraft) -> Result<Entry, ModifyEntryError>;
    /// Restores an entry with its existing id. Implementations must not overwrite another entry.
    async fn restore_entry(&self, entry: Entry) -> Result<Entry, ModifyEntryError>;
    async fn remove_entry(&self, entry_id: u32) -> anyhow::Result<()>;
    async fn update_entry(&self, entry: Entry) -> Result<Entry, ModifyEntryError>;
    async fn get_export_object(&self, entries_ids: &[u32]) -> anyhow::Result<EntriesDTO>;
    async fn import_entries(&self, entries_dto: EntriesDTO) -> anyhow::Result<()> {
        debug_assert_eq!(
            TRANSFER_DATA_VERSION, entries_dto.version,
            "Version mismatches check if there is a need to do a converting to the data"
        );

        for entry_draft in entries_dto.entries {
            self.add_entry(entry_draft).await?;
        }

        Ok(())
    }
    /// Assigns priority to all entries that don't have a priority assigned to
    async fn assign_priority_to_entries(&self, priority: u32) -> anyhow::Result<()>;

    /// Returns prior snapshots of the given entry, newest first. Backends
    /// that don't support revisioning return an empty vec.
    async fn get_revisions_for_entry(&self, _entry_id: u32) -> anyhow::Result<Vec<EntryRevision>> {
        Ok(Vec::new())
    }

    /// Records a user-visible action for the activity log. Best-effort;
    /// callers should not propagate failures — logging must never break
    /// the operation it's describing. Backends that don't support this
    /// silently drop the event.
    async fn log_activity(
        &self,
        _action_type: &str,
        _entry_id: Option<u32>,
        _details: Option<&str>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    /// Returns activity log entries newest-first. Backends that don't
    /// support activity logging return an empty vec.
    async fn get_activity_log(&self) -> anyhow::Result<Vec<ActivityLogEntry>> {
        Ok(Vec::new())
    }

    /// Records that an entry was successfully written to the Obsidian vault.
    /// Stores the content hash (for skip-unchanged), the filename, and the
    /// vault-relative directory the file was placed in so a later filename
    /// or category change can unlink the old file.
    async fn set_obsidian_sync_state(
        &self,
        entry_id: u32,
        synced_at: DateTime<Utc>,
        content_hash: &str,
        filename: &str,
        relative_dir: &str,
    ) -> anyhow::Result<()>;

    /// Clears the Obsidian sync state for an entry — used after the file is
    /// unlinked (e.g. entry deleted in tjournal, file removed from vault).
    async fn clear_obsidian_sync_state(&self, entry_id: u32) -> anyhow::Result<()>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityLogEntry {
    pub id: u32,
    pub timestamp: DateTime<Utc>,
    pub action_type: String,
    pub entry_id: Option<u32>,
    pub details: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryRevision {
    pub id: u32,
    pub entry_id: u32,
    pub title: String,
    pub date: DateTime<Utc>,
    pub content: String,
    pub priority: Option<u32>,
    pub tags: Vec<String>,
    pub saved_at: DateTime<Utc>,
}

/// The default category for entries that don't have one set (e.g., loaded
/// from a pre-category JSON file or an unmigrated DB row).
pub const DEFAULT_CATEGORY: &str = "journal";

fn default_category() -> String {
    DEFAULT_CATEGORY.to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub id: u32,
    pub date: DateTime<Utc>,
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub priority: Option<u32>,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default)]
    pub sync_provider: Option<String>,
    #[serde(default)]
    pub external_id: Option<String>,
    #[serde(default)]
    pub last_synced_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub deleted_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub source_last_edited_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub obsidian_synced_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub obsidian_content_hash: Option<String>,
    #[serde(default)]
    pub obsidian_filename: Option<String>,
    #[serde(default)]
    pub obsidian_relative_dir: Option<String>,
}

impl Entry {
    #[allow(dead_code)]
    pub fn new(
        id: u32,
        date: DateTime<Utc>,
        title: String,
        content: String,
        tags: Vec<String>,
        priority: Option<u32>,
    ) -> Self {
        Self {
            id,
            date,
            title,
            content,
            tags,
            priority,
            category: default_category(),
            sync_provider: None,
            external_id: None,
            last_synced_at: None,
            deleted_at: None,
            updated_at: None,
            source_last_edited_at: None,
            obsidian_synced_at: None,
            obsidian_content_hash: None,
            obsidian_filename: None,
            obsidian_relative_dir: None,
        }
    }

    pub fn from_draft(id: u32, draft: EntryDraft) -> Self {
        Self {
            id,
            date: draft.date,
            title: draft.title,
            content: draft.content,
            tags: draft.tags,
            priority: draft.priority,
            category: draft.category,
            sync_provider: draft.sync_provider,
            external_id: draft.external_id,
            last_synced_at: draft.last_synced_at,
            deleted_at: draft.deleted_at,
            updated_at: draft.updated_at,
            source_last_edited_at: draft.source_last_edited_at,
            obsidian_synced_at: None,
            obsidian_content_hash: None,
            obsidian_filename: None,
            obsidian_relative_dir: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryDraft {
    pub date: DateTime<Utc>,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub priority: Option<u32>,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default)]
    pub sync_provider: Option<String>,
    #[serde(default)]
    pub external_id: Option<String>,
    #[serde(default)]
    pub last_synced_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub deleted_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub source_last_edited_at: Option<DateTime<Utc>>,
}

impl EntryDraft {
    pub fn new(
        date: DateTime<Utc>,
        title: String,
        tags: Vec<String>,
        priority: Option<u32>,
    ) -> Self {
        let content = String::new();
        Self {
            date,
            title,
            content,
            tags,
            priority,
            category: default_category(),
            sync_provider: None,
            external_id: None,
            last_synced_at: None,
            deleted_at: None,
            updated_at: None,
            source_last_edited_at: None,
        }
    }

    #[must_use]
    pub fn with_content(mut self, content: String) -> Self {
        self.content = content;
        self
    }

    #[must_use]
    pub fn with_category(mut self, category: String) -> Self {
        self.category = category;
        self
    }

    pub fn from_entry(entry: Entry) -> Self {
        Self {
            date: entry.date,
            title: entry.title,
            content: entry.content,
            tags: entry.tags,
            priority: entry.priority,
            category: entry.category,
            sync_provider: entry.sync_provider,
            external_id: entry.external_id,
            last_synced_at: entry.last_synced_at,
            deleted_at: entry.deleted_at,
            updated_at: entry.updated_at,
            source_last_edited_at: entry.source_last_edited_at,
        }
    }
}

/// Entries data transfer object
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntriesDTO {
    pub version: u16,
    pub entries: Vec<EntryDraft>,
}

impl EntriesDTO {
    pub fn new(entries: Vec<EntryDraft>) -> Self {
        Self {
            version: TRANSFER_DATA_VERSION,
            entries,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use chrono::TimeZone;

    use super::*;

    fn sample_draft() -> EntryDraft {
        EntryDraft {
            date: Utc.with_ymd_and_hms(2024, 1, 2, 3, 4, 5).unwrap(),
            title: String::from("Draft"),
            content: String::from("Body"),
            tags: vec![String::from("one"), String::from("two")],
            priority: Some(3),
            category: default_category(),
            sync_provider: None,
            external_id: None,
            last_synced_at: None,
            deleted_at: None,
            updated_at: None,
            source_last_edited_at: None,
        }
    }

    struct ImportStubProvider {
        added_entries: Mutex<Vec<EntryDraft>>,
        fail_on_call: Option<usize>,
    }

    impl ImportStubProvider {
        fn new(fail_on_call: Option<usize>) -> Self {
            Self {
                added_entries: Mutex::new(Vec::new()),
                fail_on_call,
            }
        }
    }

    impl DataProvider for ImportStubProvider {
        async fn load_all_entries(&self) -> anyhow::Result<Vec<Entry>> {
            unreachable!("not used in these tests");
        }

        async fn add_entry(&self, entry: EntryDraft) -> Result<Entry, ModifyEntryError> {
            let mut added_entries = self.added_entries.lock().unwrap();
            let call_idx = added_entries.len();
            added_entries.push(entry.clone());

            if self.fail_on_call == Some(call_idx) {
                return Err(ModifyEntryError::ValidationError(format!(
                    "fail on {call_idx}"
                )));
            }

            Ok(Entry::from_draft(call_idx as u32, entry))
        }

        async fn restore_entry(&self, _entry: Entry) -> Result<Entry, ModifyEntryError> {
            unreachable!("not used in these tests");
        }

        async fn remove_entry(&self, _entry_id: u32) -> anyhow::Result<()> {
            unreachable!("not used in these tests");
        }

        async fn update_entry(&self, _entry: Entry) -> Result<Entry, ModifyEntryError> {
            unreachable!("not used in these tests");
        }

        async fn get_export_object(&self, _entries_ids: &[u32]) -> anyhow::Result<EntriesDTO> {
            unreachable!("not used in these tests");
        }

        async fn assign_priority_to_entries(&self, _priority: u32) -> anyhow::Result<()> {
            unreachable!("not used in these tests");
        }

        async fn set_obsidian_sync_state(
            &self,
            _entry_id: u32,
            _synced_at: DateTime<Utc>,
            _content_hash: &str,
            _filename: &str,
            _relative_dir: &str,
        ) -> anyhow::Result<()> {
            unreachable!("not used in these tests");
        }

        async fn clear_obsidian_sync_state(&self, _entry_id: u32) -> anyhow::Result<()> {
            unreachable!("not used in these tests");
        }
    }

    #[test]
    fn draft_to_entry() {
        let draft = sample_draft();

        let entry = Entry::from_draft(7, draft.clone());

        assert_eq!(entry.id, 7);
        assert_eq!(entry.date, draft.date);
        assert_eq!(entry.title, draft.title);
        assert_eq!(entry.content, draft.content);
        assert_eq!(entry.tags, draft.tags);
        assert_eq!(entry.priority, draft.priority);
    }

    #[test]
    fn with_content_replaces_only_body() {
        let draft = sample_draft();

        let updated = draft.clone().with_content(String::from("Updated"));

        assert_eq!(updated.content, "Updated");
        assert_eq!(updated.date, draft.date);
        assert_eq!(updated.title, draft.title);
        assert_eq!(updated.tags, draft.tags);
        assert_eq!(updated.priority, draft.priority);
    }

    #[test]
    fn from_entry_drops_id_only() {
        let entry = Entry::new(
            11,
            Utc.with_ymd_and_hms(2023, 11, 12, 13, 14, 15).unwrap(),
            String::from("Title"),
            String::from("Content"),
            vec![String::from("tag")],
            Some(2),
        );

        let draft = EntryDraft::from_entry(entry.clone());

        assert_eq!(draft.date, entry.date);
        assert_eq!(draft.title, entry.title);
        assert_eq!(draft.content, entry.content);
        assert_eq!(draft.tags, entry.tags);
        assert_eq!(draft.priority, entry.priority);
    }

    #[test]
    fn dto_sets_version() {
        let dto = EntriesDTO::new(vec![sample_draft()]);

        assert_eq!(dto.version, TRANSFER_DATA_VERSION);
        assert_eq!(dto.entries, vec![sample_draft()]);
    }

    #[tokio::test]
    async fn import_entries_keeps_order() {
        let provider = ImportStubProvider::new(None);
        let entries = vec![
            sample_draft(),
            EntryDraft::new(
                Utc.with_ymd_and_hms(2025, 6, 7, 8, 9, 10).unwrap(),
                String::from("Second"),
                vec![String::from("x")],
                None,
            ),
        ];

        provider
            .import_entries(EntriesDTO::new(entries.clone()))
            .await
            .unwrap();

        let added_entries = provider.added_entries.lock().unwrap().clone();
        assert_eq!(added_entries, entries);
    }

    #[tokio::test]
    async fn import_entries_stops_on_error() {
        let provider = ImportStubProvider::new(Some(1));
        let entries = vec![
            sample_draft(),
            EntryDraft::new(
                Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
                String::from("Second"),
                vec![],
                None,
            ),
            EntryDraft::new(
                Utc.with_ymd_and_hms(2025, 1, 2, 0, 0, 0).unwrap(),
                String::from("Third"),
                vec![],
                None,
            ),
        ];

        let err = provider
            .import_entries(EntriesDTO::new(entries.clone()))
            .await
            .unwrap_err();

        assert_eq!(err.to_string(), "fail on 1");

        // The stub records the draft before failing, so the third entry proves import stopped.
        let added_entries = provider.added_entries.lock().unwrap().clone();
        assert_eq!(added_entries, entries[..2].to_vec());
    }
}
