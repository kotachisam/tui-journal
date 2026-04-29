use anyhow::Context;
use backend::{DataProvider, Entry, EntryDraft};
use tj_publisher::notion as lib;
use tokio::sync::mpsc::UnboundedSender;

use crate::settings::notion::{NotionSettings, SyncMode, read_database_id_from_env, read_token};

pub use lib::{BootstrapOutcome, PullOutcome, PushOutcome, SyncProgress, SyncStage};

pub async fn bootstrap_from_notion<D: DataProvider>(
    provider: &D,
    settings: &NotionSettings,
    force: bool,
    progress: Option<UnboundedSender<SyncProgress>>,
) -> anyhow::Result<BootstrapOutcome> {
    let existing = provider.load_all_entries().await?;
    if !existing.is_empty() && !force {
        anyhow::bail!(
            "Local backend already has {} entries. Re-run with --force to proceed.",
            existing.len()
        );
    }

    let config = build_config(settings)?;
    let (mut outcome, drafts) = lib::bootstrap(&config, progress).await?;

    for draft in drafts {
        if let Err(err) = provider.add_entry(into_entry_draft(draft)).await {
            log::error!("Failed to insert page: {err}");
            outcome.inserted = outcome.inserted.saturating_sub(1);
            outcome.skipped += 1;
        }
    }

    Ok(outcome)
}

pub async fn pull_from_notion<D: DataProvider>(
    provider: &D,
    settings: &NotionSettings,
    progress: Option<UnboundedSender<SyncProgress>>,
) -> anyhow::Result<PullOutcome> {
    let existing = provider.load_all_entries().await?;
    let syncable: Vec<lib::SyncableEntry> = existing.iter().map(syncable_from_entry).collect();
    let config = build_config(settings)?;
    let (mut outcome, changes) = lib::pull(&syncable, &config, progress).await?;

    for change in changes {
        match change {
            lib::PullChange::Insert(draft) => {
                if let Err(err) = provider.add_entry(into_entry_draft(draft)).await {
                    log::warn!("pull insert failed: {err}");
                    outcome.errored += 1;
                    outcome.inserted = outcome.inserted.saturating_sub(1);
                }
            }
            lib::PullChange::Update(id, draft) => {
                if let Some(existing_entry) = existing.iter().find(|e| e.id == id) {
                    let merged = apply_pull_to_entry(existing_entry, &draft);
                    if let Err(err) = provider.update_entry(merged).await {
                        log::warn!("pull update failed for entry {id}: {err}");
                        outcome.errored += 1;
                        outcome.updated = outcome.updated.saturating_sub(1);
                    }
                }
            }
        }
    }

    Ok(outcome)
}

pub async fn push_to_notion<D: DataProvider>(
    provider: &D,
    settings: &NotionSettings,
    progress: Option<UnboundedSender<SyncProgress>>,
) -> anyhow::Result<PushOutcome> {
    match settings.sync_mode {
        SyncMode::Push | SyncMode::TwoWay => {}
        SyncMode::LocalOnly | SyncMode::Pull => anyhow::bail!(
            "Push is disabled. Set notion.sync_mode = \"push\" or \"two_way\" in settings first."
        ),
    }

    let existing = provider.load_all_entries().await?;
    let syncable: Vec<lib::SyncableEntry> = existing.iter().map(syncable_from_entry).collect();
    let config = build_config(settings)?;
    let (mut outcome, updates) = lib::push(&syncable, &config, progress).await?;

    for update in updates {
        if let Some(entry) = existing.iter().find(|e| e.id == update.id) {
            let merged = apply_update_to_entry(entry, &update);
            if let Err(err) = provider.update_entry(merged).await {
                log::warn!("push apply-update failed for entry {}: {err}", entry.id);
                outcome.errored += 1;
            }
        }
    }

    Ok(outcome)
}

fn build_config(settings: &NotionSettings) -> anyhow::Result<lib::NotionConfig> {
    let database_id = settings
        .database_id
        .clone()
        .or_else(read_database_id_from_env)
        .context(
            "Notion database ID not set. Provide --database-id, set NOTION_DATABASE_ID, or add notion.database_id to settings.",
        )?;
    let token = read_token()?;
    Ok(lib::NotionConfig {
        token,
        database_id,
        mappings: lib::PropertyMappings {
            title_property: settings.mappings.title_property.clone(),
            date_property: settings.mappings.date_property.clone(),
            tags_property: settings.mappings.tags_property.clone(),
        },
    })
}

fn syncable_from_entry(entry: &Entry) -> lib::SyncableEntry {
    lib::SyncableEntry {
        id: entry.id,
        date: entry.date,
        title: entry.title.clone(),
        content: entry.content.clone(),
        tags: entry.tags.clone(),
        priority: entry.priority,
        sync_provider: entry.sync_provider.clone(),
        external_id: entry.external_id.clone(),
        last_synced_at: entry.last_synced_at,
        deleted_at: entry.deleted_at,
        updated_at: entry.updated_at,
        source_last_edited_at: entry.source_last_edited_at,
    }
}

fn into_entry_draft(draft: lib::SyncableDraft) -> EntryDraft {
    let mut d = EntryDraft::new(draft.date, draft.title, draft.tags, draft.priority);
    d.content = draft.content;
    d.sync_provider = draft.sync_provider;
    d.external_id = draft.external_id;
    d.last_synced_at = draft.last_synced_at;
    d.deleted_at = draft.deleted_at;
    d.updated_at = draft.updated_at;
    d.source_last_edited_at = draft.source_last_edited_at;
    d
}

fn apply_pull_to_entry(existing: &Entry, draft: &lib::SyncableDraft) -> Entry {
    let mut merged = existing.clone();
    merged.title = draft.title.clone();
    merged.date = draft.date;
    merged.tags = draft.tags.clone();
    merged.content = draft.content.clone();
    merged.sync_provider = draft.sync_provider.clone();
    merged.external_id = draft.external_id.clone();
    merged.last_synced_at = draft.last_synced_at;
    merged.source_last_edited_at = draft.source_last_edited_at;
    merged.updated_at = draft.updated_at;
    merged
}

fn apply_update_to_entry(existing: &Entry, update: &lib::EntryUpdate) -> Entry {
    let mut merged = existing.clone();
    merged.sync_provider = update.sync_provider.clone();
    merged.external_id = update.external_id.clone();
    merged.last_synced_at = update.last_synced_at;
    merged.source_last_edited_at = update.source_last_edited_at;
    merged.updated_at = update.updated_at;
    merged
}
