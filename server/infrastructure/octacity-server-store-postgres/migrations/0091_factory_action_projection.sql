-- Read-side checkpoints only: source Run versions invalidate these rows in the
-- original source transaction without making execution depend on index writes.
CREATE TABLE factory_action_projections (
  run_id UUID PRIMARY KEY REFERENCES factory_runs(id),
  source_version BIGINT NOT NULL CHECK (source_version > 0),
  observed_at_millis BIGINT NOT NULL
);
CREATE TABLE factory_action_items (
  run_id UUID NOT NULL REFERENCES factory_action_projections(run_id),
  node_attempt_id UUID NOT NULL,
  deadline_millis BIGINT,
  description JSONB NOT NULL CHECK (
    jsonb_typeof(description) = 'object' AND octet_length(description::text) <= 1048576
  ),
  PRIMARY KEY (run_id, node_attempt_id)
);
-- Includes terminal Runs so backfill can distinguish covered emptiness from gaps.
CREATE INDEX factory_runs_action_backfill_idx ON factory_runs(updated_at, id) WHERE visible;
