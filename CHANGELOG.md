# Changelog

Fork of [AmmarAbouZor/tui-journal](https://github.com/AmmarAbouZor/tui-journal).
Tracks features and fixes that diverge from upstream. Upstream-bound changes
(with open or merged PRs) are noted inline.

Format loosely based on [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

### Added

- **Bidirectional Notion sync** (fork-only). Three CLI subcommands:
  - `tjournal notion bootstrap [--force] [--database-id X]` — one-shot
    initial import of every page from a Notion database.
  - `tjournal notion pull [--database-id X]` — incremental sync. Skips
    pages whose `last_edited_time` hasn't changed since the last sync;
    last-write-wins conflict resolution when both sides changed.
  - `tjournal notion push [--database-id X]` — sends locally modified
    entries to Notion. Creates pages for unsynced entries, updates
    modified synced ones, archives entries with `deleted_at`. Skips
    conflicts with a warning.
- **`notion.sync_mode` config gate** (fork-only). `local_only` (default)
  / `pull` / `push` / `two_way`. Push and exit-time sync refuse to run
  unless set to `push` or `two_way`.
- **Configurable Notion property mappings** (fork-only). Override
  `notion.mappings.title_property`, `.date_property`, `.tags_property` in
  config to adapt to any Notion database schema.
- **Exit-time sync prompt** (fork-only). When quitting with unsynced
  changes and push is enabled, a prompt offers to push before exit.
- **Environment auto-load** (fork-only). `.dev.vars` and `.env` in the
  working directory are loaded at startup via `dotenvy`. Non-
  overwriting; exported shell variables take precedence.
- **Journal templates** (fork-only). Press `Shift+N` to pick a template
  for a new entry. Templates live as `.md` files in the config
  directory under `templates/`, optionally with YAML frontmatter
  specifying `title`, `tags`, and `priority`. If the dir is missing,
  a prompt offers to create it with starter templates and a README.
- **Content-indexed fuzzy find** (fork-only). The fuzzy-find popup now
  searches across title + first 120 chars of content per entry, so
  blank-title entries become findable.
- **`tag_visibility` setting** (fork-only). Set to `"hide"` in config
  to omit tags from the entries-list render. Tags remain editable via
  the entry details dialog.
- **Contextual Ctrl-T hint** (fork-only). The entry popup's Tags field
  label now reads `Tags - comma-separated | <Ctrl-T>: browse existing`
  when focused, surfacing the existing shortcut.
- **Editor escape-to-list** (upstream PR
  [#616](https://github.com/AmmarAbouZor/tui-journal/pull/616), open).
  Pressing `Esc` in Normal mode returns focus to the entries list.
- **Markdown rendering in preview pane** (merged from Julian's PR
  [#609](https://github.com/AmmarAbouZor/tui-journal/pull/609) + local
  fixes). Press `p` on the entries list to render the current entry's
  content as styled markdown instead of the raw editor.

### Changed

- `add_entry` and `update_entry` on the SQLite backend now persist all
  sync-metadata columns and preserve caller-provided `updated_at` when
  set, so sync-initiated writes don't trip their own "locally modified"
  detection.
- TUI edit paths (`update_entry_attributes`, `update_entry_content`)
  stamp `updated_at = now` directly on the in-memory entry so the
  unsynced count at exit reflects the real state.
- `resolve_exit` reloads entries from the data provider before
  computing the unsynced count, covering drift from CLI-triggered
  syncs in prior sessions.

### Fixed

- Markdown preview mode no longer strands the editor in a render-while-
  editing state when `i` / `Enter` is pressed from preview.
- Preview mode can be toggled in multi-select mode (`p` now bound
  there too) and exited with global `Esc`.
- Exit-time sync prompt receives accurate counts after in-memory drift
  from prior sync operations.
- Empty-title entries in Notion-imported data no longer produce a
  blank title line in the entries list (the title row is skipped
  when trimmed-empty).

### Schema

Two migrations added columns used by the sync machinery. They are
backward compatible via `serde(default)` and a back-fill:

- `20260419000000_app_sync_fields.sql` — `sync_provider`, `external_id`,
  `last_synced_at`, `deleted_at`.
- `20260420000000_entry_updated_at.sql` — `updated_at`,
  `source_last_edited_at`. Back-fills `updated_at` from
  `COALESCE(last_synced_at, date)` for existing rows.

### Documentation

- `docs/NOTION_SYNC.md` — complete setup and operational guide for
  the Notion integration.
- `.dev.vars.example` — env-var template with inline comments.

### Upstream PR status

- [#616](https://github.com/AmmarAbouZor/tui-journal/pull/616)
  editor escape-to-list — **open**, reopened by maintainer after an
  initial close.
- [#617](https://github.com/AmmarAbouZor/tui-journal/pull/617)
  empty-title support — **closed**. Needed an issue-first discussion
  per the upstream AI-contribution policy; alternative designs
  (auto-populate from date, dedicated "untitled" marker) deferred
  pending that conversation.
- [#619](https://github.com/AmmarAbouZor/tui-journal/pull/619)
  `tag_visibility` setting — **closed** by contributor after the same
  policy realisation.
