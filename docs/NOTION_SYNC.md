# Notion Sync

Fork-specific feature. Bidirectional sync between `tui-journal`'s local
SQLite storage and a Notion database. Covers initial import (`bootstrap`),
ongoing pull, and push back. Everything is manual and CLI-triggered — no
background jobs.

## One-time setup

1. **Create a Notion integration.** Go to
   <https://www.notion.so/profile/integrations>, create a new internal
   integration, give it read + write access, copy the secret token.

2. **Share the target database with the integration.** Open the Notion
   database, click `⋯` → `Connections` → `Connect to` → select your
   integration. Without this step every API call returns 404.

3. **Grab the database ID.** From the DB page URL:
   `https://www.notion.so/<DATABASE_ID>?v=...` — the 32-hex-char chunk
   before `?v=` is what you need. Dashed or undashed both work.

4. **Set the env vars.** Copy `.dev.vars.example` to `.dev.vars` in the
   repo root and fill in:

   ```bash
   NOTION_TOKEN=ntn_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
   NOTION_DATABASE_ID=xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
   ```

   `.dev.vars` and `.env` are both auto-loaded at startup by `dotenvy`.
   Already-exported shell vars take precedence, so you can override
   ad-hoc with `NOTION_TOKEN=... tjournal notion pull`.

5. **Enable the sync mode you want** in `~/.config/tui-journal/config.toml`:

   ```toml
   [notion]
   # Options: "local_only" (default) | "pull" | "push" | "two_way"
   sync_mode = "two_way"
   ```

   Push commands refuse to run unless `sync_mode` is `"push"` or
   `"two_way"`. This is the only gate — per-entry opt-in is deliberately
   not required (see "Privacy model" below).

## Commands

All commands live under `tjournal notion <sub>`. Run with `-v` to get
outcome lines in the log (`~/Library/Caches/tui-journal/tui-journal.log`
on macOS).

### `bootstrap`

One-shot initial import. Pulls every page in the configured Notion DB
into local storage. Refuses if the local backend already has entries
unless `--force` is passed.

```bash
tjournal -v notion bootstrap
# or with an override:
tjournal -v notion bootstrap --database-id <another-db-id>
# clobber the local DB and re-import:
tjournal -v notion bootstrap --force
```

Properties mapped: Notion title → local `title`, `Date Created` →
`date`, multi-select tags → `tags`. Page body markdown → `content`
(via notionrs's built-in converter; some `<empty-block/>` / `<unknown>`
lines get stripped in our mapper). Sync metadata populated:
`sync_provider="notion"`, `external_id=<page_id>`,
`source_last_edited_at=<Notion's last_edited_time>`.

### `pull`

Incremental. Fetches all pages from Notion, compares
`remote.last_edited_time` against local `source_last_edited_at`:

- **Remote unchanged** → skip (no markdown fetch, no DB write).
- **Remote changed, local unchanged** → apply remote locally.
- **Both changed** → latest timestamp wins. Local-loses cases are
  logged as `local_wins` in the outcome.
- **Page not in local** → insert as new entry.
- **Local-only entries** (`sync_provider IS NULL`) are ignored
  entirely.

```bash
tjournal -v notion pull
```

### `push`

Sends locally modified entries to Notion. Requires `sync_mode` to be
`"push"` or `"two_way"`. Per-entry semantics:

- **No `external_id`** → create a new Notion page, store returned
  page_id + source_last_edited_at. If `sync_provider` was NULL, gets
  promoted to `"notion"` (first push treats local-only entries as
  pushable by default).
- **Unchanged locally** (`updated_at <= last_synced_at`) → skip.
- **Locally modified, remote unchanged** → update page properties +
  replace page markdown.
- **Both sides changed** → skip with warning, logged as
  `skipped_conflict`. Run `pull` first to reconcile.
- **`deleted_at` set + `external_id` present** → archive the Notion
  page.

```bash
tjournal -v notion push
```

## Privacy model

Default behavior: when `sync_mode` is `"push"`/`"two_way"`, **every
local entry is pushable**. Entries with `sync_provider = NULL` are
promoted to synced on first push.

**If you want a specific entry to never sync**, today the only option
is to keep `sync_mode = "local_only"` globally, or archive the entry
after pushing. A proper per-entry "don't sync" flag isn't implemented —
logged as a future TODO.

**Keys never touch disk** beyond `.dev.vars`/`.env` — those are
gitignored. Config stores only the database_id, never the token.

## Conflict resolution

Last-write-wins by timestamp comparison:

- **Pull** compares `remote.last_edited_time` vs stored
  `source_last_edited_at` (for remote change detection) and
  `local.updated_at` vs `last_synced_at` (for local change detection).
- **Push** does the mirror check.
- If both sides changed since last sync, the side with the later
  timestamp wins. The other side's change is logged (`local_wins` on
  pull, `skipped_conflict` on push — for push, you need to run pull
  first to reconcile).

## Property mappings

By default the mapper assumes:

- Title comes from the `Name` property (Notion DBs always have exactly
  one title-type property)
- Date comes from a `Date Created` property
- Tags come from the first multi-select property found

Override in `config.toml` if your schema differs:

```toml
[notion.mappings]
title_property = "Name"
date_property = "Journal Date"
tags_property = "Categories"
```

## Known quirks

### AI summary loop

If your Notion DB has an `AI summary` property with auto-fill enabled,
Notion updates that property asynchronously after a page changes. That
bumps the page's `last_edited_time`, which a subsequent `pull` detects
as a remote change. The pull updates local state, which can then show
as "local changed" on the next push. The loop usually settles after
one cycle but is noisy. Mitigation is a future TODO (e.g. comparing
content hashes instead of timestamps, or ignoring changes where
`last_edited_by` is a bot).

### Empty titles

Notion renders blank title fields as "New page" in the UI; our local
list view just shows the date. Both sides have the same empty string
underneath — purely a UI divergence, not a data one.

### `<empty-block/>` / `<unknown>` markers

notionrs emits these as placeholder lines for blocks it can't cleanly
render. We strip both in the mapper's `sanitize_markdown` helper.

## Recovery

### Duplicate entries after sync

If bootstrap or pull inserts duplicates (most commonly caused by
running an older build where `add_entry` didn't persist sync metadata),
clean up with:

```bash
sqlite3 ~/Documents/tui-journal/entries.db "
DELETE FROM entries
WHERE sync_provider IS NULL
  AND EXISTS (
    SELECT 1 FROM entries AS dupes
    WHERE dupes.title = entries.title
      AND dupes.date = entries.date
      AND dupes.sync_provider = 'notion'
  );
"
```

This deletes NULL-provider entries that have a Notion-synced counterpart
(matched by title+date). Genuinely local-only entries survive because
they have no counterpart.

### Timestamp drift (updated_at > last_synced_at after sync)

If `push` reports `updated > 0` on entries you haven't touched, the
writes happened with an older binary. Rebuild (`cargo install --path .
--force`) and realign:

```bash
sqlite3 ~/Documents/tui-journal/entries.db "
UPDATE entries SET updated_at = last_synced_at
WHERE sync_provider = 'notion' AND updated_at > last_synced_at;
"
```

Verify with:

```bash
sqlite3 ~/Documents/tui-journal/entries.db \
  "SELECT COUNT(*) FROM entries WHERE sync_provider='notion' AND updated_at > last_synced_at;"
```

Should return `0`.

## Debug tips

- **Verify the integration has DB access**:

  ```bash
  curl -H "Authorization: Bearer $NOTION_TOKEN" \
       -H "Notion-Version: 2022-06-28" \
       "https://api.notion.com/v1/databases/$NOTION_DATABASE_ID" | head -50
  ```

  Expect a JSON blob. 401 = bad token. 404 = integration not shared
  with the DB.

- **Check current sync state across all entries**:

  ```bash
  sqlite3 ~/Documents/tui-journal/entries.db "
  SELECT
    COUNT(*) AS total,
    SUM(CASE WHEN sync_provider IS NULL THEN 1 ELSE 0 END) AS local_only,
    SUM(CASE WHEN sync_provider = 'notion' THEN 1 ELSE 0 END) AS synced,
    SUM(CASE WHEN updated_at > last_synced_at THEN 1 ELSE 0 END) AS locally_dirty
  FROM entries;
  "
  ```

- **Inspect most recently remote-edited entries** (useful for debugging
  AI-summary loops):

  ```bash
  sqlite3 ~/Documents/tui-journal/entries.db "
  SELECT id, substr(title, 1, 30) AS title,
    datetime(source_last_edited_at) AS remote,
    datetime(last_synced_at) AS synced
  FROM entries
  WHERE sync_provider = 'notion'
  ORDER BY source_last_edited_at DESC
  LIMIT 10;
  "
  ```

## Architecture pointers

For debugging or extending:

- `src/settings/notion.rs` — `NotionSettings`, `SyncMode`,
  `PropertyMappings`, env-var readers.
- `src/notion/client.rs` — thin wrapper over notionrs exposing the 6
  operations we actually need: retrieve_database, query_data_source,
  get_page_markdown, create_page, update_page_properties,
  replace_page_markdown, archive_page.
- `src/notion/mapper.rs` — bidirectional conversion between
  `PageResponse` and `Entry`/`EntryDraft`. `page_to_draft` for read,
  `entry_to_properties` for write.
- `src/notion/bootstrap.rs` — one-shot import logic.
- `src/notion/pull.rs` — incremental pull with `decide_update`
  deciding skip/apply/local-wins.
- `src/notion/push.rs` — push with `decide_push` deciding
  create/update/archive/skip/conflict. Rate-limited to ~3 req/sec.
- Schema: migrations `20260419000000_app_sync_fields.sql` (adds
  sync metadata columns), `20260420000000_entry_updated_at.sql` (adds
  `updated_at` and `source_last_edited_at` for conflict detection).

## TODOs

- Per-entry "never sync" flag or `#local` tag convention
- Exit-time sync prompt ("Push N unsynced changes?")
- Progress ETA in the sync UI
- Content-hash comparison to mitigate AI-summary loops
- Generic "sync provider" trait abstraction once a second provider
  (Obsidian, Logseq) is on the table
