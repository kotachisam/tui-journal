use anyhow::{Context, anyhow, bail};
use backend::{DataProvider, EntriesDTO, Entry};
use std::collections::HashMap;
use std::fmt::Write as _;
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

const DEFAULT_FILENAME_FORMAT: &str = "{id}-{slug}";

fn render_filename(fmt: &str, entry: &Entry) -> anyhow::Result<String> {
    let mut out = String::with_capacity(fmt.len() + 16);
    let mut chars = fmt.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '{' {
            out.push(ch);
            continue;
        }
        let mut token = String::new();
        let mut closed = false;
        for tc in chars.by_ref() {
            if tc == '}' {
                closed = true;
                break;
            }
            token.push(tc);
        }
        if !closed {
            bail!("Unclosed '{{' in filename format");
        }
        let (name, spec) = match token.split_once(':') {
            Some((n, s)) => (n, Some(s)),
            None => (token.as_str(), None),
        };
        match name {
            "id" => {
                if spec.is_some() {
                    bail!("Token {{id}} does not accept a format spec");
                }
                let _ = write!(out, "{}", entry.id);
            }
            "slug" | "title" => {
                if spec.is_some() {
                    bail!("Token {{{name}}} does not accept a format spec");
                }
                out.push_str(&slug_for_filename(&entry.title));
            }
            "date" => {
                let spec = spec.unwrap_or("%Y-%m-%d");
                let mut tmp = String::new();
                if write!(&mut tmp, "{}", entry.date.format(spec)).is_err() {
                    bail!("Invalid date format spec: '{spec}'");
                }
                out.push_str(&tmp);
            }
            other => bail!("Unknown token '{{{other}}}' in filename format"),
        }
    }
    if !out.ends_with(".md") {
        out.push_str(".md");
    }
    validate_filename_safety(&out)?;
    Ok(out)
}

fn validate_filename_safety(name: &str) -> anyhow::Result<()> {
    if name.is_empty() || name == ".md" {
        bail!("Filename format produced empty filename");
    }
    if name.contains('/') || name.contains('\\') {
        bail!("Filename format produced path separator: '{name}'");
    }
    if name.starts_with('.') {
        bail!("Filename format produced hidden file: '{name}'");
    }
    if name == "." || name == ".." {
        bail!("Filename format produced reserved name: '{name}'");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_entry(id: u32, title: &str, date: &str) -> Entry {
        let date = chrono::NaiveDateTime::parse_from_str(
            &format!("{date} 00:00:00"),
            "%Y-%m-%d %H:%M:%S",
        )
        .unwrap()
        .and_utc();
        Entry::new(id, date, title.to_string(), String::new(), vec![], None)
    }

    #[test]
    fn default_format_matches_legacy_pattern() {
        let e = mk_entry(42, "Hello World", "2026-01-02");
        assert_eq!(
            render_filename(DEFAULT_FILENAME_FORMAT, &e).unwrap(),
            "42-hello-world.md"
        );
    }

    #[test]
    fn empty_title_uses_untitled_slug() {
        let e = mk_entry(7, "", "2026-01-02");
        assert_eq!(
            render_filename(DEFAULT_FILENAME_FORMAT, &e).unwrap(),
            "7-untitled.md"
        );
    }

    #[test]
    fn date_format_yyyy_mm_dd() {
        let e = mk_entry(1, "", "2026-04-19");
        assert_eq!(
            render_filename("{date:%Y-%m-%d}", &e).unwrap(),
            "2026-04-19.md"
        );
    }

    #[test]
    fn mixed_tokens() {
        let e = mk_entry(99, "Big Day", "2026-04-19");
        assert_eq!(
            render_filename("{date:%Y-%m-%d}-{slug}", &e).unwrap(),
            "2026-04-19-big-day.md"
        );
    }

    #[test]
    fn date_default_spec_when_omitted() {
        let e = mk_entry(1, "", "2026-04-19");
        assert_eq!(render_filename("{date}", &e).unwrap(), "2026-04-19.md");
    }

    #[test]
    fn explicit_md_extension_not_doubled() {
        let e = mk_entry(1, "", "2026-04-19");
        assert_eq!(
            render_filename("{date:%Y-%m-%d}.md", &e).unwrap(),
            "2026-04-19.md"
        );
    }

    #[test]
    fn unknown_token_errors() {
        let e = mk_entry(1, "x", "2026-04-19");
        assert!(render_filename("{nope}", &e).is_err());
    }

    #[test]
    fn unclosed_brace_errors() {
        let e = mk_entry(1, "x", "2026-04-19");
        assert!(render_filename("{id-no-close", &e).is_err());
    }

    #[test]
    fn rejects_path_separator_via_date_spec() {
        let e = mk_entry(1, "", "2026-04-19");
        assert!(render_filename("{date:%Y/%m/%d}", &e).is_err());
    }

    #[test]
    fn rejects_hidden_file() {
        let e = mk_entry(1, "", "2026-04-19");
        assert!(render_filename(".hidden-{id}", &e).is_err());
    }

    #[test]
    fn invalid_strftime_spec_errors() {
        let e = mk_entry(1, "", "2026-04-19");
        assert!(render_filename("{date:%Q}", &e).is_err());
    }

    #[test]
    fn id_does_not_accept_spec() {
        let e = mk_entry(1, "x", "2026-04-19");
        assert!(render_filename("{id:foo}", &e).is_err());
    }
}
