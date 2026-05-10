use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result, bail};
use backend::{DataProvider, Entry};
use chrono::{DateTime, Utc};

use crate::app::persistence::{render_filename, write_frontmatter};
use crate::settings::obsidian::ObsidianSettings;

#[derive(Debug, Default, Clone)]
pub struct SyncOutcome {
    pub written: usize,
    pub skipped_unchanged: usize,
    pub deleted: usize,
    pub conflicts: Vec<Conflict>,
    pub skipped_unmapped: Vec<UnmappedCategory>,
    pub errored: Vec<EntryError>,
}

#[derive(Debug, Clone)]
pub struct Conflict {
    pub entry_id: u32,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct UnmappedCategory {
    pub entry_id: u32,
    pub category: String,
}

#[derive(Debug, Clone)]
pub struct EntryError {
    pub entry_id: u32,
    pub message: String,
}

pub async fn push_to_obsidian<D: DataProvider>(
    provider: &D,
    settings: &ObsidianSettings,
    force: bool,
) -> Result<SyncOutcome> {
    let vault_dir = settings
        .vault_dir
        .as_ref()
        .context("obsidian.vault_dir is not configured")?;
    if !vault_dir.exists() {
        bail!(
            "obsidian.vault_dir does not exist: {}",
            vault_dir.display()
        );
    }

    let format_str = settings.filename_format_or_default().to_string();
    let entries = provider.load_all_entries().await?;

    let live: Vec<&Entry> = entries
        .iter()
        .filter(|e| e.deleted_at.is_none())
        .collect();
    let dead: Vec<&Entry> = entries
        .iter()
        .filter(|e| e.deleted_at.is_some() && e.obsidian_filename.is_some())
        .collect();

    let mut outcome = SyncOutcome::default();

    let mut planned: Vec<PlannedWrite> = Vec::with_capacity(live.len());
    for entry in &live {
        match plan_write(entry, settings, &format_str) {
            PlanResult::Plan(plan) => planned.push(plan),
            PlanResult::Unmapped(u) => outcome.skipped_unmapped.push(u),
            PlanResult::Error(e) => outcome.errored.push(e),
        }
    }

    detect_filename_collisions(&planned, &mut outcome.errored);
    if !outcome.errored.is_empty() {
        return Ok(outcome);
    }

    for plan in planned {
        match execute_plan(provider, vault_dir, plan, force).await {
            Ok(WriteResult::Wrote) => outcome.written += 1,
            Ok(WriteResult::Skipped) => outcome.skipped_unchanged += 1,
            Ok(WriteResult::Conflict { entry_id, path }) => {
                outcome.conflicts.push(Conflict { entry_id, path })
            }
            Err(err) => outcome.errored.push(EntryError {
                entry_id: err.entry_id,
                message: err.message,
            }),
        }
    }

    for entry in dead {
        match propagate_delete(provider, vault_dir, entry).await {
            Ok(DeleteResult::Unlinked) | Ok(DeleteResult::AlreadyGone) => outcome.deleted += 1,
            Err(err) => outcome.errored.push(EntryError {
                entry_id: err.entry_id,
                message: err.message,
            }),
        }
    }

    Ok(outcome)
}

struct PlannedWrite<'a> {
    entry: &'a Entry,
    relative_dir: String,
    filename: String,
    body: String,
    content_hash: String,
}

enum PlanResult<'a> {
    Plan(PlannedWrite<'a>),
    Unmapped(UnmappedCategory),
    Error(EntryError),
}

fn plan_write<'a>(
    entry: &'a Entry,
    settings: &ObsidianSettings,
    format_str: &str,
) -> PlanResult<'a> {
    let Some(relative_dir) = settings.dir_for_category(&entry.category) else {
        return PlanResult::Unmapped(UnmappedCategory {
            entry_id: entry.id,
            category: entry.category.clone(),
        });
    };
    let filename = match render_filename(format_str, entry) {
        Ok(name) => name,
        Err(err) => {
            return PlanResult::Error(EntryError {
                entry_id: entry.id,
                message: format!("filename: {err}"),
            });
        }
    };
    let mut body = String::new();
    write_frontmatter(entry, &mut body);
    body.push_str(&entry.content);
    if !entry.content.ends_with('\n') {
        body.push('\n');
    }
    let content_hash = hash_body(&body);
    PlanResult::Plan(PlannedWrite {
        entry,
        relative_dir: relative_dir.to_string(),
        filename,
        body,
        content_hash,
    })
}

fn detect_filename_collisions(plans: &[PlannedWrite<'_>], errored: &mut Vec<EntryError>) {
    let mut by_path: HashMap<PathBuf, Vec<u32>> = HashMap::new();
    for plan in plans {
        let key = PathBuf::from(&plan.relative_dir).join(&plan.filename);
        by_path.entry(key).or_default().push(plan.entry.id);
    }
    let mut colliders: HashSet<u32> = HashSet::new();
    for (path, ids) in by_path.into_iter().filter(|(_, ids)| ids.len() > 1) {
        let mut ids = ids;
        ids.sort();
        let msg = format!(
            "filename collision at '{}' from entries {:?}; add {{id}} to obsidian.filename_format",
            path.display(),
            ids
        );
        for id in &ids {
            colliders.insert(*id);
            errored.push(EntryError {
                entry_id: *id,
                message: msg.clone(),
            });
        }
    }
    if !colliders.is_empty() {
        // Dedup messages: keep one error per entry id
        let mut seen: HashSet<u32> = HashSet::new();
        errored.retain(|e| seen.insert(e.entry_id));
    }
}

enum WriteResult {
    Wrote,
    Skipped,
    Conflict { entry_id: u32, path: PathBuf },
}

struct WriteFailure {
    entry_id: u32,
    message: String,
}

async fn execute_plan<D: DataProvider>(
    provider: &D,
    vault_dir: &Path,
    plan: PlannedWrite<'_>,
    force: bool,
) -> std::result::Result<WriteResult, WriteFailure> {
    let target_dir = vault_dir.join(&plan.relative_dir);
    let target_path = target_dir.join(&plan.filename);

    if let (Some(prev_filename), Some(prev_dir)) = (
        plan.entry.obsidian_filename.as_ref(),
        plan.entry.obsidian_relative_dir.as_ref(),
    ) {
        let prev_path = vault_dir.join(prev_dir).join(prev_filename);
        if prev_path != target_path && prev_path.exists() {
            tokio::fs::remove_file(&prev_path).await.map_err(|err| WriteFailure {
                entry_id: plan.entry.id,
                message: format!(
                    "failed to unlink stale path {}: {}",
                    prev_path.display(),
                    err
                ),
            })?;
        }
    }

    if Some(plan.content_hash.as_str()) == plan.entry.obsidian_content_hash.as_deref()
        && target_path.exists()
    {
        return Ok(WriteResult::Skipped);
    }

    if !force
        && target_path.exists()
        && let Some(synced_at) = plan.entry.obsidian_synced_at
        && file_modified_after(&target_path, synced_at).unwrap_or(false)
    {
        return Ok(WriteResult::Conflict {
            entry_id: plan.entry.id,
            path: target_path,
        });
    }

    tokio::fs::create_dir_all(&target_dir).await.map_err(|err| WriteFailure {
        entry_id: plan.entry.id,
        message: format!("failed to create dir {}: {}", target_dir.display(), err),
    })?;
    tokio::fs::write(&target_path, plan.body.as_bytes()).await.map_err(|err| WriteFailure {
        entry_id: plan.entry.id,
        message: format!("failed to write {}: {}", target_path.display(), err),
    })?;

    let synced_at = Utc::now();
    provider
        .set_obsidian_sync_state(
            plan.entry.id,
            synced_at,
            &plan.content_hash,
            &plan.filename,
            &plan.relative_dir,
        )
        .await
        .map_err(|err| WriteFailure {
            entry_id: plan.entry.id,
            message: format!("failed to update sync state: {err}"),
        })?;

    Ok(WriteResult::Wrote)
}

enum DeleteResult {
    Unlinked,
    AlreadyGone,
}

async fn propagate_delete<D: DataProvider>(
    provider: &D,
    vault_dir: &Path,
    entry: &Entry,
) -> std::result::Result<DeleteResult, WriteFailure> {
    let (Some(filename), Some(relative_dir)) = (
        entry.obsidian_filename.as_ref(),
        entry.obsidian_relative_dir.as_ref(),
    ) else {
        return Ok(DeleteResult::AlreadyGone);
    };
    let path = vault_dir.join(relative_dir).join(filename);
    let result = if path.exists() {
        tokio::fs::remove_file(&path).await.map_err(|err| WriteFailure {
            entry_id: entry.id,
            message: format!("failed to unlink {}: {}", path.display(), err),
        })?;
        DeleteResult::Unlinked
    } else {
        DeleteResult::AlreadyGone
    };
    provider
        .clear_obsidian_sync_state(entry.id)
        .await
        .map_err(|err| WriteFailure {
            entry_id: entry.id,
            message: format!("failed to clear sync state: {err}"),
        })?;
    Ok(result)
}

fn hash_body(body: &str) -> String {
    let mut h = DefaultHasher::new();
    body.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn file_modified_after(path: &Path, threshold: DateTime<Utc>) -> Option<bool> {
    let mtime = std::fs::metadata(path).ok()?.modified().ok()?;
    let threshold_st: SystemTime = threshold.into();
    Some(mtime > threshold_st)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable_for_same_content() {
        assert_eq!(hash_body("hello world"), hash_body("hello world"));
    }

    #[test]
    fn hash_differs_for_different_content() {
        assert_ne!(hash_body("hello"), hash_body("hello!"));
    }

    fn entry_with(id: u32, category: &str, content: &str) -> Entry {
        let mut e = Entry::new(
            id,
            chrono::Utc::now(),
            String::new(),
            content.to_string(),
            vec![],
            None,
        );
        e.category = category.to_string();
        e
    }

    fn settings_with_dirs() -> ObsidianSettings {
        ObsidianSettings {
            vault_dir: Some(PathBuf::from("/tmp/vault")),
            filename_format: Some("{date:%Y-%m-%d}-{id}".to_string()),
            enable_on_notion_sync: true,
            category_dirs: [("journal", "DAILY Journal"), ("post", "Blog")]
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        }
    }

    #[test]
    fn plan_write_skips_unmapped_category() {
        let s = settings_with_dirs();
        let e = entry_with(1, "unknown_category", "x");
        match plan_write(&e, &s, "{date:%Y-%m-%d}-{id}") {
            PlanResult::Unmapped(u) => assert_eq!(u.category, "unknown_category"),
            _ => panic!("expected Unmapped"),
        }
    }

    #[test]
    fn plan_write_succeeds_for_mapped_category() {
        let s = settings_with_dirs();
        let e = entry_with(1, "journal", "x");
        match plan_write(&e, &s, "{date:%Y-%m-%d}-{id}") {
            PlanResult::Plan(p) => {
                assert_eq!(p.relative_dir, "DAILY Journal");
                assert!(p.filename.ends_with("-1.md"));
                assert!(p.content_hash.len() == 16);
            }
            _ => panic!("expected Plan"),
        }
    }

    #[test]
    fn collision_detection_flags_duplicates() {
        let mut errored = vec![];
        let e1 = entry_with(1, "journal", "");
        let e2 = entry_with(2, "journal", "");
        // Force same filename for both via static format
        let plans = vec![
            PlannedWrite {
                entry: &e1,
                relative_dir: "DAILY Journal".to_string(),
                filename: "same.md".to_string(),
                body: String::new(),
                content_hash: String::new(),
            },
            PlannedWrite {
                entry: &e2,
                relative_dir: "DAILY Journal".to_string(),
                filename: "same.md".to_string(),
                body: String::new(),
                content_hash: String::new(),
            },
        ];
        detect_filename_collisions(&plans, &mut errored);
        assert_eq!(errored.len(), 2);
        assert!(errored.iter().any(|e| e.entry_id == 1));
        assert!(errored.iter().any(|e| e.entry_id == 2));
    }

    #[test]
    fn collision_detection_passes_unique_paths() {
        let mut errored = vec![];
        let e1 = entry_with(1, "journal", "");
        let e2 = entry_with(2, "journal", "");
        let plans = vec![
            PlannedWrite {
                entry: &e1,
                relative_dir: "DAILY Journal".to_string(),
                filename: "a.md".to_string(),
                body: String::new(),
                content_hash: String::new(),
            },
            PlannedWrite {
                entry: &e2,
                relative_dir: "DAILY Journal".to_string(),
                filename: "b.md".to_string(),
                body: String::new(),
                content_hash: String::new(),
            },
        ];
        detect_filename_collisions(&plans, &mut errored);
        assert!(errored.is_empty());
    }
}
