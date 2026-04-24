use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, anyhow};

use crate::settings::settings_default_dir_path;

const TEMPLATES_SUBDIR: &str = "templates";
const FRONTMATTER_DELIM: &str = "---";

#[derive(Debug, Clone)]
pub struct Template {
    pub name: String,
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub priority: Option<u32>,
    pub content: String,
}

pub fn templates_dir() -> anyhow::Result<PathBuf> {
    Ok(settings_default_dir_path()?.join(TEMPLATES_SUBDIR))
}

pub fn list_templates() -> anyhow::Result<Vec<Template>> {
    let dir = templates_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut templates: Vec<Template> = fs::read_dir(&dir)
        .with_context(|| format!("Failed to read templates dir: {}", dir.display()))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        })
        .filter_map(|entry| load_template(&entry.path()).ok())
        .collect();

    templates.sort_by_key(|a| a.name.to_lowercase());
    Ok(templates)
}

pub fn load_template(path: &Path) -> anyhow::Result<Template> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("Failed to read template: {}", path.display()))?;

    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_owned())
        .ok_or_else(|| anyhow!("Template file has no readable name"))?;

    let (title, tags, priority, content) = split_frontmatter(&raw);

    Ok(Template {
        name,
        title,
        tags,
        priority,
        content,
    })
}

pub fn create_default_templates() -> anyhow::Result<PathBuf> {
    let dir = templates_dir()?;
    fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create templates dir: {}", dir.display()))?;

    let daily = dir.join("daily-reflection.md");
    if !daily.exists() {
        fs::write(&daily, DEFAULT_DAILY_TEMPLATE)?;
    }

    let gratitude = dir.join("gratitude.md");
    if !gratitude.exists() {
        fs::write(&gratitude, DEFAULT_GRATITUDE_TEMPLATE)?;
    }

    let readme = dir.join("README.md");
    if !readme.exists() {
        fs::write(&readme, TEMPLATES_README)?;
    }

    Ok(dir)
}

/// Parses YAML-style frontmatter delimited by `---` lines. Only `title`,
/// `tags` (comma-separated or YAML flow sequence), and `priority` are
/// recognised; unknown keys are silently ignored so templates can carry
/// future fields without breaking older builds. Content is everything after
/// the closing delimiter. A file without frontmatter is treated as pure
/// content.
fn split_frontmatter(raw: &str) -> (Option<String>, Vec<String>, Option<u32>, String) {
    let mut lines = raw.lines();

    let first = lines.next();
    if first.map(str::trim) != Some(FRONTMATTER_DELIM) {
        return (None, Vec::new(), None, raw.to_owned());
    }

    let mut frontmatter_lines: Vec<&str> = Vec::new();
    let mut closed = false;
    let mut content_lines: Vec<&str> = Vec::new();

    for line in lines {
        if !closed {
            if line.trim() == FRONTMATTER_DELIM {
                closed = true;
            } else {
                frontmatter_lines.push(line);
            }
        } else {
            content_lines.push(line);
        }
    }

    if !closed {
        return (None, Vec::new(), None, raw.to_owned());
    }

    let mut title = None;
    let mut tags = Vec::new();
    let mut priority = None;

    for line in frontmatter_lines {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_lowercase();
        let value = value.trim();

        match key.as_str() {
            "title" => title = Some(strip_quotes(value).to_owned()),
            "tags" => tags = parse_tag_list(value),
            "priority" => priority = value.parse::<u32>().ok(),
            _ => {}
        }
    }

    let content = content_lines.join("\n").trim_start_matches('\n').to_owned();
    (title, tags, priority, content)
}

fn strip_quotes(s: &str) -> &str {
    let trimmed = s.trim();
    if trimmed.len() >= 2 {
        let first = trimmed.chars().next().unwrap();
        let last = trimmed.chars().last().unwrap();
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            return &trimmed[1..trimmed.len() - 1];
        }
    }
    trimmed
}

fn parse_tag_list(value: &str) -> Vec<String> {
    let trimmed = value.trim();
    let inner = if trimmed.starts_with('[') && trimmed.ends_with(']') {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };
    inner
        .split(',')
        .map(|t| strip_quotes(t.trim()).to_owned())
        .filter(|t| !t.is_empty())
        .collect()
}

const DEFAULT_DAILY_TEMPLATE: &str = r#"---
title: Daily reflection
tags: [daily]
---
## What happened today

-

## How am I feeling

-

## What's on my mind
"#;

const DEFAULT_GRATITUDE_TEMPLATE: &str = r#"---
title: Gratitude
tags: [gratitude]
---
Three things I am grateful for today:

1.
2.
3.
"#;

const TEMPLATES_README: &str = r#"# tui-journal templates

Each `.md` file in this directory becomes a selectable template in the
Shift+N picker. The filename (without `.md`) is what the picker shows.

## Format

A template is plain markdown. Optional YAML frontmatter at the top can
pre-fill fields in the new-entry popup:

```
---
title: Morning pages
tags: [reflection, daily]
priority: 2
---
Body content that seeds the entry's content field goes here.
```

Supported frontmatter keys:

- `title` — string. Pre-fills the Title field.
- `tags`  — comma-separated list or YAML flow sequence
            (e.g. `[one, two]`). Pre-fills the Tags field.
- `priority` — positive integer. Pre-fills the Priority field.

Frontmatter is optional. Files without it are treated as body-only
templates; the Title/Tags/Priority fields default to empty.

## Adding a new template

1. Copy any existing template (`daily-reflection.md` is a good start).
2. Rename the copy to something memorable — that name shows up in the
   picker.
3. Edit the frontmatter (or delete it if you don't want pre-fills) and
   the body to taste.
4. Shift+N inside `tjournal` will pick up the new file next time.

## Notes

- Values can be edited from inside the new-entry popup after selection,
  so templates are starting points, not strict forms.
- The date is always today's date when the template is selected — it's
  not part of the template spec.
- Unknown frontmatter keys are ignored, so you can add your own notes
  inside the frontmatter without breaking anything (but it's cleaner
  to keep them out).
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_frontmatter() {
        let raw = "---\ntitle: Morning Pages\ntags: [reflection, daily]\npriority: 2\n---\nBody\n";
        let (title, tags, priority, content) = split_frontmatter(raw);
        assert_eq!(title.as_deref(), Some("Morning Pages"));
        assert_eq!(tags, vec!["reflection", "daily"]);
        assert_eq!(priority, Some(2));
        assert_eq!(content, "Body");
    }

    #[test]
    fn no_frontmatter_treats_all_as_content() {
        let raw = "Just content\nmore content";
        let (title, tags, priority, content) = split_frontmatter(raw);
        assert!(title.is_none());
        assert!(tags.is_empty());
        assert!(priority.is_none());
        assert_eq!(content, raw);
    }

    #[test]
    fn unclosed_frontmatter_treats_all_as_content() {
        let raw = "---\ntitle: no closer\ncontent here";
        let (title, _, _, content) = split_frontmatter(raw);
        assert!(title.is_none());
        assert_eq!(content, raw);
    }

    #[test]
    fn comma_separated_tags_without_brackets() {
        let raw = "---\ntags: one, two, three\n---\n";
        let (_, tags, _, _) = split_frontmatter(raw);
        assert_eq!(tags, vec!["one", "two", "three"]);
    }

    #[test]
    fn quoted_title_gets_stripped() {
        let raw = "---\ntitle: \"Quoted Title\"\n---\n";
        let (title, _, _, _) = split_frontmatter(raw);
        assert_eq!(title.as_deref(), Some("Quoted Title"));
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let raw = "---\ntitle: ok\ncustom_field: ignored\n---\nbody";
        let (title, _, _, content) = split_frontmatter(raw);
        assert_eq!(title.as_deref(), Some("ok"));
        assert_eq!(content, "body");
    }
}
