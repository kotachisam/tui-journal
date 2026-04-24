use std::collections::HashMap;

use anyhow::Context;
use backend::{DataProvider, Entry};
use chrono::{DateTime, Utc};
use notionrs_types::prelude::PageResponse;
use time::OffsetDateTime;
use tokio::sync::mpsc::UnboundedSender;

use crate::settings::notion::{NotionSettings, read_database_id_from_env, read_token};

use super::{
    bootstrap::{SyncProgress, SyncStage},
    client::NotionClient,
    mapper::{NOTION_PROVIDER, page_to_draft},
};

#[derive(Debug, Default, Clone, Copy)]
pub struct PullOutcome {
    pub inserted: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub local_wins: usize,
    pub errored: usize,
}

pub async fn pull_from_notion<D: DataProvider>(
    provider: &D,
    settings: &NotionSettings,
    progress: Option<UnboundedSender<SyncProgress>>,
) -> anyhow::Result<PullOutcome> {
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
    let pages = client.fetch_all_pages(&data_source_id).await?;

    let existing = provider.load_all_entries().await?;
    let by_external_id = index_by_external_id(&existing);

    let mut outcome = PullOutcome::default();
    let mut plan: Vec<PullPlanItem> = Vec::new();
    for page in pages {
        match by_external_id.get(page.id.as_str()) {
            None => plan.push(PullPlanItem::Insert(page)),
            Some(existing_entry) => match decide_update(existing_entry, &page) {
                UpdateDecision::Skip => outcome.unchanged += 1,
                UpdateDecision::LocalWins => outcome.local_wins += 1,
                UpdateDecision::ApplyRemote => {
                    plan.push(PullPlanItem::ApplyRemote(page, existing_entry));
                }
            },
        }
    }

    let total = plan.len();
    for (index, item) in plan.into_iter().enumerate() {
        let position = index + 1;
        send_progress(&progress, SyncStage::FetchingPageContent, position, total);

        match item {
            PullPlanItem::Insert(page) => {
                match insert_new(provider, settings, &client, &page).await {
                    Ok(()) => outcome.inserted += 1,
                    Err(err) => {
                        log::warn!("pull insert failed for {}: {err}", page.id);
                        outcome.errored += 1;
                    }
                }
            }
            PullPlanItem::ApplyRemote(page, existing_entry) => {
                match apply_remote(provider, settings, &client, &page, existing_entry).await {
                    Ok(()) => outcome.updated += 1,
                    Err(err) => {
                        log::warn!("pull update failed for {}: {err}", page.id);
                        outcome.errored += 1;
                    }
                }
            }
        }
    }

    Ok(outcome)
}

enum PullPlanItem<'a> {
    Insert(PageResponse),
    ApplyRemote(PageResponse, &'a Entry),
}

fn index_by_external_id(entries: &[Entry]) -> HashMap<&str, &Entry> {
    entries
        .iter()
        .filter_map(|entry| {
            let provider_match = entry
                .sync_provider
                .as_deref()
                .is_some_and(|provider| provider == NOTION_PROVIDER);
            let id = entry.external_id.as_deref()?;
            provider_match.then_some((id, entry))
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpdateDecision {
    Skip,
    ApplyRemote,
    LocalWins,
}

fn decide_update(existing: &Entry, page: &PageResponse) -> UpdateDecision {
    let remote_edited = offset_to_chrono(page.last_edited_time);
    let remote_changed = existing
        .source_last_edited_at
        .is_none_or(|stored| remote_edited > stored);

    if !remote_changed {
        return UpdateDecision::Skip;
    }

    let local_changed = match (existing.updated_at, existing.last_synced_at) {
        (Some(updated), Some(synced)) => updated > synced,
        (Some(_), None) => true,
        _ => false,
    };

    if !local_changed {
        return UpdateDecision::ApplyRemote;
    }

    if remote_edited > existing.updated_at.unwrap_or(DateTime::<Utc>::MIN_UTC) {
        UpdateDecision::ApplyRemote
    } else {
        UpdateDecision::LocalWins
    }
}

async fn insert_new<D: DataProvider>(
    provider: &D,
    settings: &NotionSettings,
    client: &NotionClient,
    page: &PageResponse,
) -> anyhow::Result<()> {
    let markdown = client.page_as_markdown(&page.id).await?;
    let draft = page_to_draft(page, markdown, &settings.mappings);
    provider
        .add_entry(draft)
        .await
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    Ok(())
}

async fn apply_remote<D: DataProvider>(
    provider: &D,
    settings: &NotionSettings,
    client: &NotionClient,
    page: &PageResponse,
    existing: &Entry,
) -> anyhow::Result<()> {
    let markdown = client.page_as_markdown(&page.id).await?;
    let draft = page_to_draft(page, markdown, &settings.mappings);

    let mut merged = existing.clone();
    merged.title = draft.title;
    merged.date = draft.date;
    merged.tags = draft.tags;
    merged.content = draft.content;
    merged.sync_provider = draft.sync_provider;
    merged.external_id = draft.external_id;
    merged.last_synced_at = draft.last_synced_at;
    merged.source_last_edited_at = draft.source_last_edited_at;
    merged.updated_at = draft.last_synced_at;

    provider
        .update_entry(merged)
        .await
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    Ok(())
}

fn offset_to_chrono(odt: OffsetDateTime) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(odt.unix_timestamp(), odt.nanosecond()).unwrap_or_else(Utc::now)
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
        external_id: &str,
        last_synced_at: Option<DateTime<Utc>>,
        updated_at: Option<DateTime<Utc>>,
        source_last_edited_at: Option<DateTime<Utc>>,
    ) -> Entry {
        Entry {
            id: 1,
            date: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            title: "T".into(),
            content: "C".into(),
            tags: vec![],
            priority: None,
            sync_provider: Some(NOTION_PROVIDER.to_owned()),
            external_id: Some(external_id.to_owned()),
            last_synced_at,
            deleted_at: None,
            updated_at,
            source_last_edited_at,
        }
    }

    fn make_page_with_last_edited(last_edited: OffsetDateTime) -> PageResponse {
        let json = format!(
            r#"{{
                "object": "page",
                "id": "page-1",
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

    #[test]
    fn skips_when_remote_unchanged() {
        let synced = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap();
        let existing = make_entry("page-1", Some(synced), Some(synced), Some(synced));
        let page = make_page_with_last_edited(chrono_to_offset(synced));

        assert_eq!(decide_update(&existing, &page), UpdateDecision::Skip);
    }

    #[test]
    fn applies_remote_when_only_remote_changed() {
        let synced = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap();
        let remote_edit = Utc.with_ymd_and_hms(2025, 7, 1, 0, 0, 0).unwrap();
        let existing = make_entry("page-1", Some(synced), Some(synced), Some(synced));
        let page = make_page_with_last_edited(chrono_to_offset(remote_edit));

        assert_eq!(decide_update(&existing, &page), UpdateDecision::ApplyRemote);
    }

    #[test]
    fn local_wins_when_local_newer() {
        let synced = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap();
        let remote_edit = Utc.with_ymd_and_hms(2025, 7, 1, 0, 0, 0).unwrap();
        let local_edit = Utc.with_ymd_and_hms(2025, 8, 1, 0, 0, 0).unwrap();
        let existing = make_entry("page-1", Some(synced), Some(local_edit), Some(synced));
        let page = make_page_with_last_edited(chrono_to_offset(remote_edit));

        assert_eq!(decide_update(&existing, &page), UpdateDecision::LocalWins);
    }

    #[test]
    fn remote_wins_when_remote_newer_than_local_edit() {
        let synced = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap();
        let local_edit = Utc.with_ymd_and_hms(2025, 7, 1, 0, 0, 0).unwrap();
        let remote_edit = Utc.with_ymd_and_hms(2025, 8, 1, 0, 0, 0).unwrap();
        let existing = make_entry("page-1", Some(synced), Some(local_edit), Some(synced));
        let page = make_page_with_last_edited(chrono_to_offset(remote_edit));

        assert_eq!(decide_update(&existing, &page), UpdateDecision::ApplyRemote);
    }

    #[test]
    fn stale_entry_without_source_last_edited_is_pulled() {
        let existing = make_entry("page-1", None, None, None);
        let remote = Utc.with_ymd_and_hms(2025, 7, 1, 0, 0, 0).unwrap();
        let page = make_page_with_last_edited(chrono_to_offset(remote));

        assert_eq!(decide_update(&existing, &page), UpdateDecision::ApplyRemote);
    }

    fn chrono_to_offset(dt: DateTime<Utc>) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(dt.timestamp()).unwrap()
    }
}
