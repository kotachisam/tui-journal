//! Thin adapter over `tj_publisher::obsidian`. Maps tjournal's `Entry` into
//! the publisher's `PublishableEntry` (loading prior sync state from the
//! `entries.obsidian_*` columns), drives the engine, then applies the
//! resulting `EntryAction`s back to the local DataProvider.
//!
//! All sync logic (planning, hashing, conflict refusal, delete propagation,
//! file I/O) lives in the publisher; this module handles only the
//! tjournal-specific shape mapping.

use anyhow::{Context, Result};
use backend::{DataProvider, Entry};
use tj_publisher::markdown::write_frontmatter as md_write_frontmatter;
use tj_publisher::obsidian as engine;

pub use engine::{EntryAction, EntrySyncState, SyncOutcome};

use crate::app::persistence::frontmatter_from;
use crate::settings::obsidian::ObsidianSettings;

pub async fn push_to_obsidian<D: DataProvider>(
    provider: &D,
    settings: &ObsidianSettings,
    force: bool,
) -> Result<SyncOutcome> {
    let config = settings
        .to_publisher_config()
        .context("obsidian.vault_dir is not configured")?;

    let entries = provider.load_all_entries().await?;
    let publishable: Vec<engine::PublishableEntry> =
        entries.iter().map(entry_to_publishable).collect();

    let outcome = engine::push(&publishable, &config, force).await?;
    apply_outcome(provider, &outcome).await?;
    Ok(outcome)
}

fn entry_to_publishable(entry: &Entry) -> engine::PublishableEntry {
    let mut body = String::new();
    let fields = frontmatter_from(entry);
    md_write_frontmatter(&fields, &mut body);
    body.push_str(&entry.content);
    if !entry.content.ends_with('\n') {
        body.push('\n');
    }

    let last_sync = match (
        entry.obsidian_synced_at,
        entry.obsidian_content_hash.as_ref(),
        entry.obsidian_filename.as_ref(),
        entry.obsidian_relative_dir.as_ref(),
    ) {
        (Some(synced_at), Some(hash), Some(filename), Some(dir)) => Some(EntrySyncState {
            synced_at,
            content_hash: hash.clone(),
            filename: filename.clone(),
            relative_dir: dir.clone(),
        }),
        _ => None,
    };

    engine::PublishableEntry {
        id: entry.id as u64,
        category: entry.category.clone(),
        date: entry.date,
        title: entry.title.clone(),
        body,
        deleted_at: entry.deleted_at,
        last_sync,
    }
}

async fn apply_outcome<D: DataProvider>(provider: &D, outcome: &SyncOutcome) -> Result<()> {
    for action in &outcome.actions {
        match action {
            EntryAction::Wrote {
                entry_id,
                new_state,
            } => {
                provider
                    .set_obsidian_sync_state(
                        *entry_id as u32,
                        new_state.synced_at,
                        &new_state.content_hash,
                        &new_state.filename,
                        &new_state.relative_dir,
                    )
                    .await?;
            }
            EntryAction::Deleted { entry_id } => {
                provider.clear_obsidian_sync_state(*entry_id as u32).await?;
            }
            EntryAction::Skipped { .. }
            | EntryAction::Conflict { .. }
            | EntryAction::UnmappedCategory { .. }
            | EntryAction::Errored { .. } => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn entry_to_publishable_loads_last_sync_when_all_columns_present() {
        let mut e = Entry::new(
            42,
            chrono::Utc.with_ymd_and_hms(2026, 1, 2, 0, 0, 0).unwrap(),
            "title".into(),
            "body".into(),
            vec![],
            None,
        );
        e.obsidian_synced_at = Some(chrono::Utc::now());
        e.obsidian_content_hash = Some("abc".into());
        e.obsidian_filename = Some("file.md".into());
        e.obsidian_relative_dir = Some("DAILY".into());
        e.category = "journal".into();

        let p = entry_to_publishable(&e);
        assert_eq!(p.id, 42);
        assert_eq!(p.category, "journal");
        assert!(p.last_sync.is_some());
        assert!(p.body.contains("title:"));
        assert!(p.body.ends_with("body\n"));
    }

    #[test]
    fn entry_to_publishable_omits_last_sync_when_any_column_missing() {
        let mut e = Entry::new(1, chrono::Utc::now(), "t".into(), "b".into(), vec![], None);
        e.obsidian_synced_at = Some(chrono::Utc::now());
        // hash + filename + dir all missing → last_sync = None
        let p = entry_to_publishable(&e);
        assert!(p.last_sync.is_none());
    }
}
