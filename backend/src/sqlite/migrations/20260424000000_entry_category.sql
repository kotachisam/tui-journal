ALTER TABLE entries ADD COLUMN category TEXT NOT NULL DEFAULT 'journal';

CREATE INDEX IF NOT EXISTS idx_entries_category ON entries(category);
