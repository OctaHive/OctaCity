ALTER TABLE agents
  ADD COLUMN last_seen_at TIMESTAMPTZ;

UPDATE agents SET last_seen_at = updated_at WHERE last_seen_at IS NULL;

ALTER TABLE agents
  ALTER COLUMN last_seen_at SET NOT NULL,
  ALTER COLUMN last_seen_at SET DEFAULT CURRENT_TIMESTAMP,
  ADD CONSTRAINT agents_version_positive CHECK (version > 0),
  ADD CONSTRAINT agents_state_known CHECK (state IN ('online', 'offline', 'draining'));

CREATE INDEX agents_pool_assignment_idx ON agents (pool_id, pool_version, id);
