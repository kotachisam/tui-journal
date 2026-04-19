ALTER TABLE entries ADD COLUMN sync_provider TEXT;
ALTER TABLE entries ADD COLUMN external_id TEXT;
ALTER TABLE entries ADD COLUMN last_synced_at DATETIME;
ALTER TABLE entries ADD COLUMN deleted_at DATETIME;
