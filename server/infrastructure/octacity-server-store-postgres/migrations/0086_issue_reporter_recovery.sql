-- One resumable data-migration cursor; this is not a delivery queue.
-- New Issue commands already publish their original Reporter effect atomically.
CREATE TABLE issue_reporter_recovery (
  singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
  completed BOOLEAN NOT NULL DEFAULT FALSE,
  last_aggregate TEXT NOT NULL DEFAULT '',
  last_version BIGINT NOT NULL DEFAULT 0 CHECK (last_version >= 0),
  last_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000'
);
INSERT INTO issue_reporter_recovery(singleton) VALUES (TRUE);
