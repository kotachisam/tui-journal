use anyhow::{Context, bail};
use backend::DataProvider;

use crate::settings::notion::{NotionSettings, read_database_id_from_env, read_token};

use super::{client::NotionClient, mapper::page_to_draft};

#[derive(Debug, Clone, Copy, Default)]
pub struct BootstrapOutcome {
    pub inserted: usize,
    pub skipped: usize,
}

pub async fn bootstrap_from_notion<D: DataProvider>(
    provider: &D,
    settings: &NotionSettings,
    force: bool,
) -> anyhow::Result<BootstrapOutcome> {
    let database_id = settings
        .database_id
        .clone()
        .or_else(read_database_id_from_env)
        .context(
            "Notion database ID not set. Provide --database-id, set NOTION_DATABASE_ID, or add notion.database_id to settings.",
        )?;
    let token = read_token()?;

    let existing = provider.load_all_entries().await?;
    if !existing.is_empty() && !force {
        bail!(
            "Local backend already has {} entries. Re-run with --force to proceed.",
            existing.len()
        );
    }

    let client = NotionClient::new(token, database_id);
    let data_source_id = client.resolve_data_source_id().await?;
    let pages = client.fetch_all_pages(&data_source_id).await?;

    let mut outcome = BootstrapOutcome::default();
    for page in pages {
        let markdown = match client.page_as_markdown(&page.id).await {
            Ok(md) => md,
            Err(err) => {
                log::warn!("Skipping page {} (markdown fetch failed): {err}", page.id);
                outcome.skipped += 1;
                continue;
            }
        };

        let draft = page_to_draft(&page, markdown, &settings.mappings);

        if let Err(err) = provider.add_entry(draft).await {
            log::error!("Failed to insert page {}: {err}", page.id);
            outcome.skipped += 1;
            continue;
        }

        outcome.inserted += 1;
    }

    Ok(outcome)
}
