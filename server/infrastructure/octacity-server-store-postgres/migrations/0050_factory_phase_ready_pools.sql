-- Phase readiness is a projection of pinned Flow history. Selection and
-- reservation records are immutable and acquire ordinary fenced Run ownership.
CREATE TABLE factory_phase_pool_policies (
  id BYTEA PRIMARY KEY CHECK (octet_length(id) = 32),
  policy JSONB NOT NULL CHECK (jsonb_typeof(policy) = 'object' AND octet_length(policy::text) BETWEEN 2 AND 16384)
);

CREATE TABLE factory_phase_pool_entries (
  id BYTEA PRIMARY KEY CHECK (octet_length(id) = 32),
  policy_id BYTEA NOT NULL REFERENCES factory_phase_pool_policies(id),
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  run_version BIGINT NOT NULL CHECK (run_version > 0),
  flow_run_id UUID NOT NULL,
  cycle_id UUID NOT NULL,
  node_key TEXT NOT NULL CHECK (octet_length(node_key) BETWEEN 1 AND 128 AND node_key ~ '^[a-z0-9][a-z0-9._-]*$'),
  generation BIGINT NOT NULL CHECK (generation BETWEEN 0 AND 4294967295),
  rank_one BIGINT NOT NULL,
  rank_two BIGINT NOT NULL,
  rank_three BIGINT NOT NULL,
  work_id UUID NOT NULL REFERENCES factory_work_envelopes(id),
  entry JSONB NOT NULL CHECK (jsonb_typeof(entry) = 'object' AND octet_length(entry::text) BETWEEN 2 AND 65536),
  UNIQUE (policy_id, run_id, run_version, flow_run_id, cycle_id, node_key, generation),
  FOREIGN KEY (flow_run_id, run_id) REFERENCES factory_flow_runs(id, factory_run_id),
  FOREIGN KEY (cycle_id, run_id) REFERENCES factory_workflow_cycles(id, factory_run_id)
);

-- Mutable scan progress is separate from the immutable policy bytes. A blocked
-- bounded window must not hide all later eligible Work on every fresh pass.
-- This disposable cursor has no history FK: retention can remove entries, and
-- the changed scope fingerprint then discards continuation on the next pass.
ALTER TABLE factory_phase_pool_policies
  ADD COLUMN scan_after BYTEA CHECK (scan_after IS NULL OR octet_length(scan_after) = 32),
  ADD COLUMN scan_guard BYTEA CHECK (scan_guard IS NULL OR octet_length(scan_guard) = 32),
  ADD CONSTRAINT factory_phase_pool_scan_pair CHECK ((scan_after IS NULL) = (scan_guard IS NULL));

CREATE TABLE factory_phase_pool_passes (
  id BYTEA PRIMARY KEY CHECK (octet_length(id) = 32),
  policy_id BYTEA NOT NULL REFERENCES factory_phase_pool_policies(id),
  request JSONB NOT NULL CHECK (jsonb_typeof(request) = 'object' AND octet_length(request::text) BETWEEN 2 AND 32768),
  selections JSONB NOT NULL CHECK (jsonb_typeof(selections) = 'array' AND jsonb_array_length(selections) <= 100 AND octet_length(selections::text) <= 8388608)
);

CREATE TABLE factory_phase_pool_selections (
  entry_id BYTEA PRIMARY KEY REFERENCES factory_phase_pool_entries(id),
  pass_id BYTEA NOT NULL REFERENCES factory_phase_pool_passes(id),
  policy_id BYTEA NOT NULL REFERENCES factory_phase_pool_policies(id),
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  flow_run_id UUID NOT NULL REFERENCES factory_flow_runs(id),
  cycle_id UUID NOT NULL REFERENCES factory_workflow_cycles(id),
  node_key TEXT NOT NULL,
  generation BIGINT NOT NULL CHECK (generation BETWEEN 0 AND 4294967295),
  claim_id BYTEA NOT NULL REFERENCES factory_run_claims(id),
  selection JSONB NOT NULL CHECK (jsonb_typeof(selection) = 'object' AND octet_length(selection::text) BETWEEN 2 AND 98304),
  UNIQUE (run_id, flow_run_id, cycle_id, node_key, generation)
);

CREATE INDEX factory_phase_pool_entries_scope_idx ON factory_phase_pool_entries (policy_id, run_id, run_version);
CREATE INDEX factory_phase_pool_entries_order_idx ON factory_phase_pool_entries (policy_id, rank_one, rank_two, rank_three, work_id, run_id, flow_run_id, cycle_id, node_key, id);
CREATE INDEX factory_phase_pool_selections_capacity_idx ON factory_phase_pool_selections (policy_id, entry_id);

-- Schemas 47-50 belong to the same unreleased Factory slice. Rollback metadata
-- continues to name schema 46, supported by the preceding server binary.
