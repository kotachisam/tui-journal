CREATE TABLE entry_revisions (
  id         INTEGER PRIMARY KEY NOT NULL,
  entry_id   INTEGER             NOT NULL,
  title      TEXT                NOT NULL,
  date       DATETIME            NOT NULL,
  content    TEXT                NOT NULL,
  priority   INTEGER,
  tags       TEXT,
  saved_at   DATETIME            NOT NULL,
  FOREIGN KEY (entry_id) REFERENCES entries(id) ON DELETE CASCADE
);

CREATE INDEX idx_entry_revisions_entry_id ON entry_revisions(entry_id);
