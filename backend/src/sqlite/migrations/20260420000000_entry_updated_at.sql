ALTER TABLE entries ADD COLUMN updated_at DATETIME;
ALTER TABLE entries ADD COLUMN source_last_edited_at DATETIME;
UPDATE entries SET updated_at = COALESCE(last_synced_at, date);
