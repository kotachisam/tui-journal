use anyhow::{Context, anyhow, bail};
use backend::{DataProvider, EntriesDTO, Entry};
use std::{fs::File, path::PathBuf};

use super::App;
use super::state::AppState;
use super::ui::UIComponents;

impl<D> App<D>
where
    D: DataProvider,
{
    /// Count entries that would be pushed on a sync — either locally modified
    /// since last sync, or local-only (will be created in Notion on push).
    pub fn unsynced_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| match entry.sync_provider.as_deref() {
                None => true,
                Some(_) => match (entry.updated_at, entry.last_synced_at) {
                    (Some(updated), Some(synced)) => updated > synced,
                    (Some(_), None) => true,
                    _ => false,
                },
            })
            .count()
    }

    pub async fn get_activity_log(&self) -> anyhow::Result<Vec<backend::ActivityLogEntry>> {
        self.data_provide.get_activity_log().await
    }

    pub(super) async fn export_entry_content(
        &self,
        entry_id: u32,
        path: PathBuf,
    ) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let entry = self.get_entry(entry_id).expect("Entry should exist");

        tokio::fs::write(path, entry.content.to_owned()).await?;

        Ok(())
    }

    pub(super) async fn export_entries(&self, path: PathBuf) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let selected_ids: Vec<u32> = self.selected_entries.iter().cloned().collect();

        let entries_dto = self.data_provide.get_export_object(&selected_ids).await?;

        let file = File::create(path)?;
        serde_json::to_writer_pretty(&file, &entries_dto)?;

        Ok(())
    }

    pub async fn export_to_directory(
        &self,
        dir: PathBuf,
        tag: Option<String>,
    ) -> anyhow::Result<usize> {
        tokio::fs::create_dir_all(&dir)
            .await
            .with_context(|| format!("Creating export directory {}", dir.display()))?;

        let entries = self.data_provide.load_all_entries().await?;
        let matches: Vec<&Entry> = entries
            .iter()
            .filter(|entry| entry.deleted_at.is_none())
            .filter(|entry| match tag.as_deref() {
                Some(t) => entry.tags.iter().any(|entry_tag| entry_tag == t),
                None => true,
            })
            .collect();

        let mut written = 0;
        for entry in matches {
            let file_name = format!("{}-{}.md", entry.id, slug_for_filename(&entry.title));
            let mut path = dir.clone();
            path.push(&file_name);

            let mut body = String::new();
            write_frontmatter(entry, &mut body);
            body.push_str(&entry.content);
            if !entry.content.ends_with('\n') {
                body.push('\n');
            }

            tokio::fs::write(&path, body.as_bytes())
                .await
                .with_context(|| format!("Writing {}", path.display()))?;
            written += 1;
        }

        Ok(written)
    }

    pub(super) async fn import_entries(&self, file_path: PathBuf) -> anyhow::Result<()> {
        if !file_path.exists() {
            bail!("Import file doesn't exist: path {}", file_path.display())
        }

        let file = File::open(file_path)
            .map_err(|err| anyhow!("Error while opening import file: Error: {err}"))?;

        let entries_dto: EntriesDTO = serde_json::from_reader(&file)
            .map_err(|err| anyhow!("Error while parsing import file. Error: {err}"))?;

        self.data_provide
            .import_entries(entries_dto)
            .await
            .map_err(|err| anyhow!("Error while importing the entry. Error: {err}"))?;

        Ok(())
    }

    /// Assigns priority to all entries that don't have a priority assigned to
    pub(super) async fn assign_priority_to_entries(&self, priority: u32) -> anyhow::Result<()> {
        self.data_provide
            .assign_priority_to_entries(priority)
            .await?;

        Ok(())
    }

    pub fn load_state(&mut self, ui_components: &mut UIComponents) {
        let state = match AppState::load(&self.settings) {
            Ok(state) => state,
            Err(err) => {
                ui_components.show_err_msg(format!(
                    "Loading state failed. Falling back to default state\n\rError Info: {err}"
                ));
                AppState::default()
            }
        };

        self.state = state;
    }

    pub fn persist_state(&self) -> anyhow::Result<()> {
        self.state.save(&self.settings)?;

        Ok(())
    }
}

fn slug_for_filename(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut last_dash = false;
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "untitled".to_string()
    } else {
        trimmed
    }
}

fn write_frontmatter(entry: &Entry, buf: &mut String) {
    buf.push_str("---\n");
    buf.push_str(&format!("id: {}\n", entry.id));
    buf.push_str(&format!("title: {}\n", yaml_scalar(&entry.title)));
    buf.push_str(&format!("date: {}\n", entry.date.to_rfc3339()));
    if let Some(p) = entry.priority {
        buf.push_str(&format!("priority: {p}\n"));
    }
    if !entry.tags.is_empty() {
        buf.push_str("tags:\n");
        for tag in &entry.tags {
            buf.push_str(&format!("  - {}\n", yaml_scalar(tag)));
        }
    }
    buf.push_str("---\n\n");
}

fn yaml_scalar(s: &str) -> String {
    let safe = !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '-' || c == '_');
    if safe {
        s.to_string()
    } else {
        let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{escaped}\"")
    }
}
