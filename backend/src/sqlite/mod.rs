use std::{path::PathBuf, str::FromStr};

use self::sqlite_helper::EntryIntermediate;

use super::*;
use anyhow::anyhow;
use path_absolutize::Absolutize;
use sqlx::{
    Row, Sqlite, SqlitePool,
    migrate::MigrateDatabase,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};

mod sqlite_helper;

pub struct SqliteDataProvide {
    pool: SqlitePool,
}

impl SqliteDataProvide {
    pub async fn from_file(file_path: PathBuf) -> anyhow::Result<Self> {
        let file_full_path = file_path.absolutize()?;
        if !file_path.exists() {
            if let Some(parent) = file_path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }

        let db_url = format!("sqlite://{}", file_full_path.to_string_lossy());

        SqliteDataProvide::create(&db_url).await
    }

    pub async fn create(db_url: &str) -> anyhow::Result<Self> {
        if !Sqlite::database_exists(db_url).await? {
            log::trace!("Creating Database with the URL '{db_url}'");
            Sqlite::create_database(db_url)
                .await
                .map_err(|err| anyhow!("Creating database failed. Error info: {err}"))?;
        }

        // We are using the database as a normal file for one user.
        // Journal mode will causes problems with the synchronisation in our case and it must be
        // turned off
        let options = SqliteConnectOptions::from_str(db_url)?
            .journal_mode(SqliteJournalMode::Off)
            .synchronous(SqliteSynchronous::Off);

        let pool = SqlitePoolOptions::new().connect_with(options).await?;

        sqlx::migrate!("backend/src/sqlite/migrations")
            .run(&pool)
            .await
            .map_err(|err| match err {
                sqlx::migrate::MigrateError::VersionMissing(id) => anyhow!("Database version mismatches. Error Info: migration {id} was previously applied but is missing in the resolved migrations"),
                err => anyhow!("Error while applying migrations on database: Error info {err}"),
            })?;

        Ok(Self { pool })
    }
}

impl DataProvider for SqliteDataProvide {
    async fn load_all_entries(&self) -> anyhow::Result<Vec<Entry>> {
        let entries: Vec<EntryIntermediate> = sqlx::query_as(
            r"SELECT entries.id, entries.title, entries.date, entries.content, entries.priority,
                entries.sync_provider, entries.external_id, entries.last_synced_at, entries.deleted_at,
                entries.updated_at, entries.source_last_edited_at,
                GROUP_CONCAT(tags.tag) AS tags
            FROM entries
            LEFT JOIN tags ON entries.id = tags.entry_id
            GROUP BY entries.id
            ORDER BY date DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|err| {
            log::error!("Loading entries failed. Error Info {err}");
            anyhow!(err)
        })?;

        let entries: Vec<Entry> = entries.into_iter().map(Entry::from).collect();

        Ok(entries)
    }

    async fn add_entry(&self, mut entry: EntryDraft) -> Result<Entry, ModifyEntryError> {
        if entry.updated_at.is_none() {
            entry.updated_at = Some(chrono::Utc::now());
        }

        let row = sqlx::query(
            r"INSERT INTO entries (
                title, date, content, priority,
                sync_provider, external_id, last_synced_at, deleted_at,
                updated_at, source_last_edited_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING id",
        )
        .bind(&entry.title)
        .bind(entry.date)
        .bind(&entry.content)
        .bind(entry.priority)
        .bind(&entry.sync_provider)
        .bind(&entry.external_id)
        .bind(entry.last_synced_at)
        .bind(entry.deleted_at)
        .bind(entry.updated_at)
        .bind(entry.source_last_edited_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|err| {
            log::error!("Add entry failed. Error info: {err}");
            anyhow!(err)
        })?;

        let id = row.get::<u32, _>(0);

        for tag in entry.tags.iter() {
            sqlx::query(
                r"INSERT INTO tags (entry_id, tag)
                VALUES($1, $2)",
            )
            .bind(id)
            .bind(tag)
            .execute(&self.pool)
            .await
            .map_err(|err| {
                log::error!("Add entry tags failed. Error info:{err}");
                anyhow!(err)
            })?;
        }

        Ok(Entry::from_draft(id, entry))
    }

    async fn remove_entry(&self, entry_id: u32) -> anyhow::Result<()> {
        sqlx::query(r"DELETE FROM entries WHERE id=$1")
            .bind(entry_id)
            .execute(&self.pool)
            .await
            .map_err(|err| {
                log::error!("Delete entry failed. Error info: {err}");
                anyhow!(err)
            })?;

        Ok(())
    }

    async fn update_entry(&self, mut entry: Entry) -> Result<Entry, ModifyEntryError> {
        if entry.updated_at.is_none() {
            entry.updated_at = Some(chrono::Utc::now());
        }

        // Snapshot the entry's pre-update state into revisions before
        // applying the new values. Silently a no-op if the entry doesn't
        // exist (shouldn't happen under normal flow but would be benign).
        sqlx::query(
            r"INSERT INTO entry_revisions
                (entry_id, title, date, content, priority, tags, saved_at)
            SELECT
                e.id,
                e.title,
                e.date,
                e.content,
                e.priority,
                (SELECT GROUP_CONCAT(tag, ',') FROM tags WHERE entry_id = e.id),
                COALESCE(e.updated_at, e.date)
            FROM entries e
            WHERE e.id = $1",
        )
        .bind(entry.id)
        .execute(&self.pool)
        .await
        .map_err(|err| {
            log::error!("Snapshot entry revision failed. Error info: {err}");
            anyhow!(err)
        })?;

        sqlx::query(
            r"UPDATE entries
            SET title = $1,
                date = $2,
                content = $3,
                priority = $4,
                sync_provider = $5,
                external_id = $6,
                last_synced_at = $7,
                deleted_at = $8,
                updated_at = $9,
                source_last_edited_at = $10
            WHERE id = $11",
        )
        .bind(&entry.title)
        .bind(entry.date)
        .bind(&entry.content)
        .bind(entry.priority)
        .bind(&entry.sync_provider)
        .bind(&entry.external_id)
        .bind(entry.last_synced_at)
        .bind(entry.deleted_at)
        .bind(entry.updated_at)
        .bind(entry.source_last_edited_at)
        .bind(entry.id)
        .execute(&self.pool)
        .await
        .map_err(|err| {
            log::error!("Update entry failed. Error info {err}");
            anyhow!(err)
        })?;

        let existing_tags: Vec<String> = sqlx::query_scalar(
            r"SELECT tag FROM tags 
            WHERE entry_id = $1",
        )
        .bind(entry.id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| {
            log::error!("Update entry tags failed. Error info {err}");
            anyhow!(err)
        })?;

        // Tags to remove
        for tag_to_remove in existing_tags.iter().filter(|tag| !entry.tags.contains(tag)) {
            sqlx::query(r"DELETE FROM tags Where entry_id = $1 AND tag = $2")
                .bind(entry.id)
                .bind(tag_to_remove)
                .execute(&self.pool)
                .await
                .map_err(|err| {
                    log::error!("Update entry tags failed. Error info {err}");
                    anyhow!(err)
                })?;
        }

        // Tags to insert
        for tag_to_insert in entry.tags.iter().filter(|tag| !existing_tags.contains(tag)) {
            sqlx::query(
                r"INSERT INTO tags (entry_id, tag)
                VALUES ($1, $2)",
            )
            .bind(entry.id)
            .bind(tag_to_insert)
            .execute(&self.pool)
            .await
            .map_err(|err| {
                log::error!("Update entry tags failed. Error info {err}");
                anyhow!(err)
            })?;
        }

        Ok(entry)
    }

    async fn get_export_object(&self, entries_ids: &[u32]) -> anyhow::Result<EntriesDTO> {
        let ids_text = entries_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<String>>()
            .join(", ");

        let sql = format!(
            r"SELECT entries.id, entries.title, entries.date, entries.content, entries.priority,
                entries.sync_provider, entries.external_id, entries.last_synced_at, entries.deleted_at,
                entries.updated_at, entries.source_last_edited_at,
                GROUP_CONCAT(tags.tag) AS tags
            FROM entries
            LEFT JOIN tags ON entries.id = tags.entry_id
            WHERE entries.id IN ({ids_text})
            GROUP BY entries.id
            ORDER BY date DESC"
        );

        let entries: Vec<EntryIntermediate> = sqlx::query_as(sql.as_str())
            .fetch_all(&self.pool)
            .await
            .map_err(|err| {
                log::error!("Loading entries failed. Error Info {err}");
                anyhow!(err)
            })?;

        let entry_drafts = entries
            .into_iter()
            .map(Entry::from)
            .map(EntryDraft::from_entry)
            .collect();

        Ok(EntriesDTO::new(entry_drafts))
    }

    async fn assign_priority_to_entries(&self, priority: u32) -> anyhow::Result<()> {
        let sql = format!(
            r"UPDATE entries
            SET priority = '{priority}'
            WHERE priority IS NULL;"
        );

        sqlx::query(sql.as_str())
            .execute(&self.pool)
            .await
            .map_err(|err| {
                log::error!("Assign priority to entries failed. Error info {err}");

                anyhow!(err)
            })?;

        Ok(())
    }

    async fn get_revisions_for_entry(&self, entry_id: u32) -> anyhow::Result<Vec<EntryRevision>> {
        let rows: Vec<sqlite_helper::RevisionRow> = sqlx::query_as(
            r"SELECT id, entry_id, title, date, content, priority, tags, saved_at
            FROM entry_revisions
            WHERE entry_id = $1
            ORDER BY saved_at DESC, id DESC",
        )
        .bind(entry_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| {
            log::error!("Loading revisions failed. Error Info {err}");
            anyhow!(err)
        })?;

        Ok(rows.into_iter().map(EntryRevision::from).collect())
    }

    async fn log_activity(
        &self,
        action_type: &str,
        entry_id: Option<u32>,
        details: Option<&str>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r"INSERT INTO activity_log (timestamp, action_type, entry_id, details)
            VALUES ($1, $2, $3, $4)",
        )
        .bind(chrono::Utc::now())
        .bind(action_type)
        .bind(entry_id)
        .bind(details)
        .execute(&self.pool)
        .await
        .map_err(|err| {
            log::error!("Writing activity log row failed. Error info: {err}");
            anyhow!(err)
        })?;

        Ok(())
    }

    async fn get_activity_log(&self) -> anyhow::Result<Vec<ActivityLogEntry>> {
        let rows: Vec<sqlite_helper::ActivityLogRow> = sqlx::query_as(
            r"SELECT id, timestamp, action_type, entry_id, details
            FROM activity_log
            ORDER BY timestamp DESC, id DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|err| {
            log::error!("Loading activity log failed. Error Info {err}");
            anyhow!(err)
        })?;

        Ok(rows.into_iter().map(ActivityLogEntry::from).collect())
    }
}
