use chrono::{DateTime, Utc};
use sqlx::FromRow;

use crate::{ActivityLogEntry, Entry, EntryRevision};

/// Helper class to retrieve entries' data from database since FromRow can't handle arrays
#[derive(FromRow)]
pub(crate) struct EntryIntermediate {
    pub id: u32,
    pub date: DateTime<Utc>,
    pub title: String,
    pub content: String,
    pub priority: Option<u32>,
    pub category: String,
    /// Tags as a string with commas as separator for the tags
    pub tags: Option<String>,
    pub sync_provider: Option<String>,
    pub external_id: Option<String>,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub source_last_edited_at: Option<DateTime<Utc>>,
    pub obsidian_synced_at: Option<DateTime<Utc>>,
    pub obsidian_content_hash: Option<String>,
    pub obsidian_filename: Option<String>,
    pub obsidian_relative_dir: Option<String>,
}

impl From<EntryIntermediate> for Entry {
    fn from(value: EntryIntermediate) -> Self {
        Entry {
            id: value.id,
            date: value.date,
            title: value.title,
            content: value.content,
            priority: value.priority,
            category: value.category,
            tags: value
                .tags
                .map(|tags| {
                    tags.split(',')
                        .map(|t| t.trim().to_owned())
                        .filter(|t| !t.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            sync_provider: value.sync_provider,
            external_id: value.external_id,
            last_synced_at: value.last_synced_at,
            deleted_at: value.deleted_at,
            updated_at: value.updated_at,
            source_last_edited_at: value.source_last_edited_at,
            obsidian_synced_at: value.obsidian_synced_at,
            obsidian_content_hash: value.obsidian_content_hash,
            obsidian_filename: value.obsidian_filename,
            obsidian_relative_dir: value.obsidian_relative_dir,
        }
    }
}

#[derive(FromRow)]
pub(crate) struct RevisionRow {
    pub id: u32,
    pub entry_id: u32,
    pub title: String,
    pub date: DateTime<Utc>,
    pub content: String,
    pub priority: Option<u32>,
    pub tags: Option<String>,
    pub saved_at: DateTime<Utc>,
}

#[derive(FromRow)]
pub(crate) struct ActivityLogRow {
    pub id: u32,
    pub timestamp: DateTime<Utc>,
    pub action_type: String,
    pub entry_id: Option<u32>,
    pub details: Option<String>,
}

impl From<ActivityLogRow> for ActivityLogEntry {
    fn from(value: ActivityLogRow) -> Self {
        ActivityLogEntry {
            id: value.id,
            timestamp: value.timestamp,
            action_type: value.action_type,
            entry_id: value.entry_id,
            details: value.details,
        }
    }
}

impl From<RevisionRow> for EntryRevision {
    fn from(value: RevisionRow) -> Self {
        EntryRevision {
            id: value.id,
            entry_id: value.entry_id,
            title: value.title,
            date: value.date,
            content: value.content,
            priority: value.priority,
            tags: value
                .tags
                .map(|tags| {
                    tags.split(',')
                        .map(|t| t.trim().to_owned())
                        .filter(|t| !t.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            saved_at: value.saved_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn sample_intermediate(tags: Option<&str>) -> EntryIntermediate {
        EntryIntermediate {
            id: 4,
            date: Utc.with_ymd_and_hms(2024, 3, 4, 5, 6, 7).unwrap(),
            title: String::from("Title"),
            content: String::from("Content"),
            priority: Some(2),
            category: String::from("journal"),
            tags: tags.map(String::from),
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

    #[test]
    fn none_tags_become_empty() {
        let entry: Entry = sample_intermediate(None).into();

        assert!(entry.tags.is_empty());
    }

    #[test]
    fn comma_tags_preserve_order() {
        let entry: Entry = sample_intermediate(Some("rust,tests,sqlite")).into();

        assert_eq!(entry.tags, vec!["rust", "tests", "sqlite"]);
    }

    #[test]
    fn empty_tags_stay_empty() {
        let entry: Entry = sample_intermediate(Some("")).into();

        assert!(entry.tags.is_empty());
    }

    #[test]
    fn empty_segments_are_filtered() {
        let entry: Entry = sample_intermediate(Some("rust,,tests,")).into();

        assert_eq!(entry.tags, vec!["rust", "tests"]);
    }

    #[test]
    fn whitespace_only_segments_are_filtered() {
        let entry: Entry = sample_intermediate(Some("rust, , tests")).into();

        assert_eq!(entry.tags, vec!["rust", "tests"]);
    }
}
