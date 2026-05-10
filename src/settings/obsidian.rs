use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use tj_publisher::obsidian::ObsidianConfig;

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct ObsidianSettings {
    /// Absolute path to the Obsidian vault root. If unset, sync is disabled.
    #[serde(default)]
    pub vault_dir: Option<PathBuf>,
    /// Filename pattern (same syntax as `tjournal export --filename-format`).
    /// Defaults to `{date:%Y-%m-%d}-{id}` which is collision-free.
    #[serde(default)]
    pub filename_format: Option<String>,
    /// When true, a successful `tjournal notion push`/`pull` automatically
    /// triggers an obsidian sync as well. The notion command's exit code is
    /// not affected by obsidian failures (warn + continue).
    #[serde(default = "default_enable_on_notion_sync")]
    pub enable_on_notion_sync: bool,
    /// Map of entry category → vault-relative folder. Entries with a category
    /// not in this map are skipped with a warning.
    #[serde(default)]
    pub category_dirs: HashMap<String, String>,
}

fn default_enable_on_notion_sync() -> bool {
    true
}

pub const DEFAULT_FILENAME_FORMAT: &str = "{date:%Y-%m-%d}-{id}";

impl ObsidianSettings {
    /// Returns true when the vault path is configured. Other settings have
    /// sensible defaults; only `vault_dir` is required to enable sync.
    pub fn is_configured(&self) -> bool {
        self.vault_dir.is_some()
    }

    pub fn filename_format_or_default(&self) -> &str {
        self.filename_format
            .as_deref()
            .unwrap_or(DEFAULT_FILENAME_FORMAT)
    }

    /// Build the publisher's pure-config view from these settings. Returns
    /// `None` when not configured (caller should skip sync).
    pub fn to_publisher_config(&self) -> Option<ObsidianConfig> {
        let vault_dir = self.vault_dir.clone()?;
        Some(ObsidianConfig {
            vault_dir,
            filename_format: self.filename_format_or_default().to_string(),
            category_dirs: self.category_dirs.clone(),
        })
    }

    /// Validate that no category dir contains a path-traversal segment or an
    /// absolute path. Called at settings load.
    pub fn validate(&self) -> Result<()> {
        if let Some(vault) = &self.vault_dir
            && !vault.is_absolute()
        {
            bail!(
                "obsidian.vault_dir must be an absolute path; got '{}'",
                vault.display()
            );
        }
        for (category, dir) in &self.category_dirs {
            validate_relative_dir(category, dir)?;
        }
        Ok(())
    }
}

fn validate_relative_dir(category: &str, dir: &str) -> Result<()> {
    let p = Path::new(dir);
    if p.is_absolute() {
        bail!("obsidian.category_dirs[{category}] must be relative; got '{dir}'");
    }
    for component in p.components() {
        use std::path::Component;
        match component {
            Component::ParentDir => {
                bail!("obsidian.category_dirs[{category}] must not contain '..'; got '{dir}'");
            }
            Component::Prefix(_) | Component::RootDir => {
                bail!("obsidian.category_dirs[{category}] must be relative; got '{dir}'");
            }
            Component::CurDir | Component::Normal(_) => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_with_dirs(dirs: &[(&str, &str)]) -> ObsidianSettings {
        ObsidianSettings {
            vault_dir: Some(PathBuf::from("/tmp/vault")),
            category_dirs: dirs
                .iter()
                .map(|(k, v)| ((*k).into(), (*v).into()))
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn relative_dirs_pass() {
        let s = settings_with_dirs(&[("journal", "DAILY Journal"), ("post", "Blog")]);
        assert!(s.validate().is_ok());
    }

    #[test]
    fn parent_dir_rejected() {
        let s = settings_with_dirs(&[("journal", "../escape")]);
        assert!(s.validate().is_err());
    }

    #[test]
    fn absolute_category_dir_rejected() {
        let s = settings_with_dirs(&[("journal", "/etc")]);
        assert!(s.validate().is_err());
    }

    #[test]
    fn non_absolute_vault_dir_rejected() {
        let s = ObsidianSettings {
            vault_dir: Some(PathBuf::from("relative/vault")),
            ..Default::default()
        };
        assert!(s.validate().is_err());
    }

    #[test]
    fn unset_vault_means_unconfigured() {
        let s = ObsidianSettings::default();
        assert!(!s.is_configured());
    }

    #[test]
    fn to_publisher_config_round_trip() {
        let s = settings_with_dirs(&[("journal", "DAILY Journal")]);
        let cfg = s.to_publisher_config().unwrap();
        assert_eq!(cfg.vault_dir, std::path::PathBuf::from("/tmp/vault"));
        assert_eq!(cfg.category_dirs.get("journal"), Some(&"DAILY Journal".to_string()));
        assert!(cfg.filename_format.contains("{date"));
    }

    #[test]
    fn to_publisher_config_returns_none_when_unset() {
        let s = ObsidianSettings::default();
        assert!(s.to_publisher_config().is_none());
    }
}
