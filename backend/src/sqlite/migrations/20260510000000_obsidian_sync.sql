ALTER TABLE entries ADD COLUMN obsidian_synced_at DATETIME;
ALTER TABLE entries ADD COLUMN obsidian_content_hash TEXT;
ALTER TABLE entries ADD COLUMN obsidian_filename TEXT;
ALTER TABLE entries ADD COLUMN obsidian_relative_dir TEXT;
