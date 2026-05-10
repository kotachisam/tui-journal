use anyhow::{Context, anyhow, bail};
use backend::{DataProvider, EntriesDTO, Entry};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::{fs::File, path::PathBuf};
use tj_publisher::markdown::{
    DEFAULT_FILENAME_FORMAT, FrontmatterFields, RenderableEntry, render_filename as md_render,
    write_frontmatter as md_write_frontmatter,
};

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

        let is_markdown = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));

        let body = if is_markdown {
            let mut buf = String::new();
            write_frontmatter(entry, &mut buf);
            buf.push_str(&entry.content);
            if !entry.content.ends_with('\n') {
                buf.push('\n');
            }
            buf
        } else {
            entry.content.to_owned()
        };

        tokio::fs::write(path, body).await?;

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
        filename_format: Option<String>,
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

        let format_str = filename_format.as_deref().unwrap_or(DEFAULT_FILENAME_FORMAT);

        let mut planned: Vec<(String, &Entry)> = Vec::with_capacity(matches.len());
        let mut by_name: HashMap<String, Vec<u32>> = HashMap::new();
        for &entry in matches.iter() {
            let name = render_filename(format_str, entry)
                .with_context(|| format!("Rendering filename for entry id {}", entry.id))?;
            by_name.entry(name.clone()).or_default().push(entry.id);
            planned.push((name, entry));
        }

        let mut collisions: Vec<(String, Vec<u32>)> = by_name
            .into_iter()
            .filter(|(_, ids)| ids.len() > 1)
            .collect();
        if !collisions.is_empty() {
            collisions.sort_by(|a, b| a.0.cmp(&b.0));
            let mut msg = String::from(
                "Filename format produces duplicates (add {id} to disambiguate):\n",
            );
            for (name, mut ids) in collisions {
                ids.sort();
                let _ = writeln!(msg, "  '{name}' from entries {ids:?}");
            }
            bail!("{msg}");
        }

        let mut written = 0;
        for (file_name, entry) in planned {
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

        self.view_category = state.last_view_category.clone();
        self.state = state;
    }

    pub fn persist_state(&self) -> anyhow::Result<()> {
        self.state.save(&self.settings)?;

        Ok(())
    }
}

pub(crate) fn write_frontmatter(entry: &Entry, buf: &mut String) {
    md_write_frontmatter(&frontmatter_from(entry), buf);
}

pub(crate) fn render_filename(fmt: &str, entry: &Entry) -> anyhow::Result<String> {
    md_render(fmt, &renderable_from(entry))
}

pub(crate) fn renderable_from(entry: &Entry) -> RenderableEntry {
    RenderableEntry {
        id: entry.id as u64,
        date: entry.date,
        title: entry.title.clone(),
    }
}

pub(crate) fn frontmatter_from(entry: &Entry) -> FrontmatterFields {
    FrontmatterFields {
        id: entry.id as u64,
        title: entry.title.clone(),
        date: entry.date,
        priority: entry.priority,
        tags: entry.tags.clone(),
    }
}

