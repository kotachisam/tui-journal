CREATE TABLE activity_log (
  id          INTEGER PRIMARY KEY NOT NULL,
  timestamp   DATETIME            NOT NULL,
  action_type TEXT                NOT NULL,
  entry_id    INTEGER,
  details     TEXT
);

CREATE INDEX idx_activity_log_timestamp ON activity_log(timestamp DESC);
