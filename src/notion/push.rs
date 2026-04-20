use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, bail};
use backend::{DataProvider, Entry};
use chrono::{DateTime, Utc};
use notionrs_types::prelude::PageResponse;
use time::OffsetDateTime;
use tokio::sync::mpsc::UnboundedSender;

use crate::settings::notion::{NotionSettings, SyncMode, read_database_id_from_env, read_token};

use super::{
    bootstrap::{SyncProgress, SyncStage},
    client::NotionClient,
    mapper::{NOTION_PROVIDER, entry_to_properties},
};

const RATE_LIMIT_DELAY: Duration = Duration::from_millis(340);

#[derive(Debug, Default, Clone, Copy)]
pub struct PushOutcome {
    pub created: usize,
    pub updated: usize,
    pub archived: usize,
    pub skipped_unchanged: usize,
    pub skipped_conflict: usize,
    pub errored: usize,
}

pub async fn push_to_notion<D: DataProvider>(
    provider: &D,
    settings: &NotionSettings,
    progress: Option<UnboundedSender<SyncProgress>>,
) -> anyhow::Result<PushOutcome> {
    match settings.sync_mode {
        SyncMode::Push | SyncMode::TwoWay => {}
        SyncMode::LocalOnly | SyncMode::Pull => bail!(
            "Push is disabled. Set notion.sync_mode = \"push\" or \"two_way\" in settings first."
        ),
    }

    let database_id = settings
        .database_id
        .clone()
        .or_else(read_database_id_from_env)
        .context(
            "Notion database ID not set. Provide --database-id, set NOTION_DATABASE_ID, or add notion.database_id to settings.",
        )?;
    let token = read_token()?;

    send_progress(&progress, SyncStage::ResolvingDataSource, 0, 0);
    let client = NotionClient::new(token, database_id);
    let data_source_id = client.resolve_data_source_id().await?;

    send_progress(&progress, SyncStage::QueryingPages, 0, 0);
    let remote_pages = client.fetch_all_pages(&data_source_id).await?;
    let remote_by_id: HashMap<&str, &PageResponse> = remote_pages
        .iter()
        .map(|page| (page.id.as_str(), page))
        .collect();

    let local = provider.load_all_entries().await?;
    let candidates: Vec<&Entry> = local
        .iter()
        .filter(|entry| match entry.sync_provider.as_deref() {
            Some(provider) => provider == NOTION_PROVIDER,
            None => true,
        })
        .collect();

    let total = candidates.len();
    let mut outcome = PushOutcome::default();

    for (index, entry) in candidates.into_iter().enumerate() {
        let position = index + 1;
        send_progress(&progress, SyncStage::WritingToDatabase, position, total);

        let action = decide_push(entry, &remote_by_id);
        let result = match &action {
            PushAction::Skip => {
                outcome.skipped_unchanged += 1;
                continue;
            }
            PushAction::SkipConflict => {
                log::warn!(
                    "Skipping push for entry {} ({}): remote changed since last sync, use pull to reconcile",
                    entry.id,
                    entry.external_id.as_deref().unwrap_or("<no id>")
                );
                outcome.skipped_conflict += 1;
                continue;
            }
            PushAction::Archive(page_id) => archive(provider, &client, entry, page_id).await,
            PushAction::Create => create(provider, &client, entry, settings, &data_source_id).await,
            PushAction::Update(page_id) => {
                update(provider, &client, entry, settings, page_id).await
            }
        };

        match (&action, result) {
            (_, Err(err)) => {
                log::warn!("push failed for entry {}: {err}", entry.id);
                outcome.errored += 1;
            }
            (PushAction::Create, Ok(())) => outcome.created += 1,
            (PushAction::Update(_), Ok(())) => outcome.updated += 1,
            (PushAction::Archive(_), Ok(())) => outcome.archived += 1,
            _ => {}
        }

        tokio::time::sleep(RATE_LIMIT_DELAY).await;
    }

    Ok(outcome)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PushAction {
    Skip,
    SkipConflict,
    Create,
    Update(String),
    Archive(String),
}

fn decide_push(entry: &Entry, remote: &HashMap<&str, &PageResponse>) -> PushAction {
    if entry.deleted_at.is_some() {
        if let Some(external_id) = entry.external_id.as_deref() {
            if remote.contains_key(external_id) {
                return PushAction::Archive(external_id.to_owned());
            }
        }
        return PushAction::Skip;
    }

    let Some(external_id) = entry.external_id.as_deref() else {
        return PushAction::Create;
    };

    let local_changed = match (entry.updated_at, entry.last_synced_at) {
        (Some(updated), Some(synced)) => updated > synced,
        (Some(_), None) => true,
        _ => false,
    };

    if !local_changed {
        return PushAction::Skip;
    }

    let remote_page = remote.get(external_id);
    let remote_changed = match remote_page {
        Some(page) => {
            let remote_edited = offset_to_chrono(page.last_edited_time);
            entry
                .source_last_edited_at
                .is_none_or(|stored| remote_edited > stored)
        }
        None => false,
    };

    if !remote_changed {
        return PushAction::Update(external_id.to_owned());
    }

    match (entry.updated_at, remote_page) {
        (Some(updated), Some(page))
            if updated > offset_to_chrono(page.last_edited_time) =>
        {
            PushAction::Update(external_id.to_owned())
        }
        _ => PushAction::SkipConflict,
    }
}

async fn create<D: DataProvider>(
    provider: &D,
    client: &NotionClient,
    entry: &Entry,
    settings: &NotionSettings,
    data_source_id: &str,
) -> anyhow::Result<()> {
    let properties = entry_to_properties(entry, &settings.mappings);
    let response = client
        .create_page(data_source_id, properties, entry.content.clone())
        .await?;

    let now = Utc::now();
    let mut updated = entry.clone();
    updated.sync_provider = Some(NOTION_PROVIDER.to_owned());
    updated.external_id = Some(response.id.clone());
    updated.last_synced_at = Some(now);
    updated.source_last_edited_at = Some(offset_to_chrono(response.last_edited_time));
    updated.updated_at = Some(now);
    provider
        .update_entry(updated)
        .await
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    Ok(())
}

async fn update<D: DataProvider>(
    provider: &D,
    client: &NotionClient,
    entry: &Entry,
    settings: &NotionSettings,
    page_id: &str,
) -> anyhow::Result<()> {
    let properties = entry_to_properties(entry, &settings.mappings);
    let response = client.update_page_properties(page_id, properties).await?;
    tokio::time::sleep(RATE_LIMIT_DELAY).await;
    client
        .replace_page_markdown(page_id, entry.content.clone())
        .await?;

    let now = Utc::now();
    let mut refreshed = entry.clone();
    refreshed.last_synced_at = Some(now);
    refreshed.source_last_edited_at = Some(offset_to_chrono(response.last_edited_time));
    refreshed.updated_at = Some(now);
    provider
        .update_entry(refreshed)
        .await
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    Ok(())
}

async fn archive<D: DataProvider>(
    provider: &D,
    client: &NotionClient,
    entry: &Entry,
    page_id: &str,
) -> anyhow::Result<()> {
    client.archive_page(page_id).await?;

    let now = Utc::now();
    let mut refreshed = entry.clone();
    refreshed.last_synced_at = Some(now);
    refreshed.updated_at = Some(now);
    provider
        .update_entry(refreshed)
        .await
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    Ok(())
}

fn offset_to_chrono(odt: OffsetDateTime) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(odt.unix_timestamp(), odt.nanosecond())
        .unwrap_or_else(Utc::now)
}

fn send_progress(
    sender: &Option<UnboundedSender<SyncProgress>>,
    stage: SyncStage,
    current: usize,
    total: usize,
) {
    if let Some(tx) = sender {
        let _ = tx.send(SyncProgress {
            stage,
            current,
            total,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn make_entry(
        external_id: Option<&str>,
        last_synced_at: Option<DateTime<Utc>>,
        updated_at: Option<DateTime<Utc>>,
        source_last_edited_at: Option<DateTime<Utc>>,
        deleted_at: Option<DateTime<Utc>>,
    ) -> Entry {
        Entry {
            id: 1,
            date: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            title: "T".into(),
            content: "C".into(),
            tags: vec![],
            priority: None,
            sync_provider: Some(NOTION_PROVIDER.to_owned()),
            external_id: external_id.map(str::to_owned),
            last_synced_at,
            deleted_at,
            updated_at,
            source_last_edited_at,
        }
    }

    fn remote_for(page_id: &str, last_edited: OffsetDateTime) -> PageResponse {
        let json = format!(
            r#"{{
                "object": "page",
                "id": "{page_id}",
                "created_time": "2025-01-01T00:00:00.000Z",
                "last_edited_time": "{last}",
                "created_by": {{"object": "user", "id": "u"}},
                "last_edited_by": {{"object": "user", "id": "u"}},
                "cover": null, "icon": null,
                "parent": {{"type": "workspace", "workspace": true}},
                "archived": false, "properties": {{}},
                "url": "https://n.so/p",
                "public_url": null,
                "developer_survey": null, "request_id": null,
                "in_trash": false, "is_locked": false, "is_archived": false
            }}"#,
            last = last_edited
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap()
        );
        serde_json::from_str(&json).unwrap()
    }

    fn chrono_to_offset(dt: DateTime<Utc>) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(dt.timestamp()).unwrap()
    }

    #[test]
    fn new_entry_with_no_external_id_is_created() {
        let entry = make_entry(None, None, Some(Utc::now()), None, None);
        let remote = HashMap::new();
        assert_eq!(decide_push(&entry, &remote), PushAction::Create);
    }

    #[test]
    fn unsynced_local_entry_is_created() {
        let mut entry = make_entry(None, None, Some(Utc::now()), None, None);
        entry.sync_provider = None;
        let remote = HashMap::new();
        assert_eq!(decide_push(&entry, &remote), PushAction::Create);
    }

    #[test]
    fn unchanged_synced_entry_is_skipped() {
        let synced = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap();
        let entry = make_entry(
            Some("page-1"),
            Some(synced),
            Some(synced),
            Some(synced),
            None,
        );
        let remote_page = remote_for("page-1", chrono_to_offset(synced));
        let remote: HashMap<&str, &PageResponse> = std::iter::once(("page-1", &remote_page)).collect();
        assert_eq!(decide_push(&entry, &remote), PushAction::Skip);
    }

    #[test]
    fn locally_modified_without_remote_change_updates() {
        let synced = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap();
        let local_edit = Utc.with_ymd_and_hms(2025, 7, 1, 0, 0, 0).unwrap();
        let entry = make_entry(
            Some("page-1"),
            Some(synced),
            Some(local_edit),
            Some(synced),
            None,
        );
        let remote_page = remote_for("page-1", chrono_to_offset(synced));
        let remote: HashMap<&str, &PageResponse> = std::iter::once(("page-1", &remote_page)).collect();
        assert_eq!(
            decide_push(&entry, &remote),
            PushAction::Update("page-1".to_owned())
        );
    }

    #[test]
    fn conflict_with_local_newer_wins_update() {
        let synced = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap();
        let remote_edit = Utc.with_ymd_and_hms(2025, 7, 1, 0, 0, 0).unwrap();
        let local_edit = Utc.with_ymd_and_hms(2025, 8, 1, 0, 0, 0).unwrap();
        let entry = make_entry(
            Some("page-1"),
            Some(synced),
            Some(local_edit),
            Some(synced),
            None,
        );
        let remote_page = remote_for("page-1", chrono_to_offset(remote_edit));
        let remote: HashMap<&str, &PageResponse> = std::iter::once(("page-1", &remote_page)).collect();
        assert_eq!(
            decide_push(&entry, &remote),
            PushAction::Update("page-1".to_owned())
        );
    }

    #[test]
    fn conflict_with_remote_newer_skips() {
        let synced = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap();
        let local_edit = Utc.with_ymd_and_hms(2025, 7, 1, 0, 0, 0).unwrap();
        let remote_edit = Utc.with_ymd_and_hms(2025, 8, 1, 0, 0, 0).unwrap();
        let entry = make_entry(
            Some("page-1"),
            Some(synced),
            Some(local_edit),
            Some(synced),
            None,
        );
        let remote_page = remote_for("page-1", chrono_to_offset(remote_edit));
        let remote: HashMap<&str, &PageResponse> = std::iter::once(("page-1", &remote_page)).collect();
        assert_eq!(decide_push(&entry, &remote), PushAction::SkipConflict);
    }

    #[test]
    fn deleted_entry_with_remote_counterpart_is_archived() {
        let entry = make_entry(
            Some("page-1"),
            Some(Utc::now()),
            None,
            None,
            Some(Utc::now()),
        );
        let remote_page = remote_for("page-1", chrono_to_offset(Utc::now()));
        let remote: HashMap<&str, &PageResponse> = std::iter::once(("page-1", &remote_page)).collect();
        assert_eq!(
            decide_push(&entry, &remote),
            PushAction::Archive("page-1".to_owned())
        );
    }

    #[test]
    fn deleted_entry_without_remote_counterpart_is_skipped() {
        let entry = make_entry(
            Some("page-1"),
            Some(Utc::now()),
            None,
            None,
            Some(Utc::now()),
        );
        let remote: HashMap<&str, &PageResponse> = HashMap::new();
        assert_eq!(decide_push(&entry, &remote), PushAction::Skip);
    }
}
