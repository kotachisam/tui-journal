use chrono::{DateTime, Utc};
use sqlx::FromRow;

use crate::{Entry, EntryRevision};

/// Helper class to retrieve entries' data from database since FromRow can't handle arrays
#[derive(FromRow)]
pub(crate) struct EntryIntermediate {
    pub id: u32,
    pub date: DateTime<Utc>,
    pub title: String,
    pub content: String,
    pub priority: Option<u32>,
    /// Tags as a string with commas as separator for the tags
    pub tags: Option<String>,
    pub sync_provider: Option<String>,
    pub external_id: Option<String>,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub source_last_edited_at: Option<DateTime<Utc>>,
}

impl From<EntryIntermediate> for Entry {
    fn from(value: EntryIntermediate) -> Self {
        Entry {
            id: value.id,
            date: value.date,
            title: value.title,
            content: value.content,
            priority: value.priority,
            tags: value
                .tags
                .map(|tags| tags.split_terminator(',').map(String::from).collect())
                .unwrap_or_default(),
            sync_provider: value.sync_provider,
            external_id: value.external_id,
            last_synced_at: value.last_synced_at,
            deleted_at: value.deleted_at,
            updated_at: value.updated_at,
            source_last_edited_at: value.source_last_edited_at,
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
                .map(|tags| tags.split_terminator(',').map(String::from).collect())
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
            tags: tags.map(String::from),
            sync_provider: None,
            external_id: None,
            last_synced_at: None,
            deleted_at: None,
            updated_at: None,
            source_last_edited_at: None,
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
}
