-- Versioned static Agent Pool policy and structural bounds.

UPDATE pools
SET admission_policy = '{"mode":"any"}'::jsonb
WHERE admission_policy = '{}'::jsonb;

ALTER TABLE pools
  ADD COLUMN static_capacity_limit BIGINT NOT NULL DEFAULT 10000,
  ADD CONSTRAINT pools_positive_version CHECK (version > 0),
  ADD CONSTRAINT pools_non_empty_name CHECK (name <> '' AND octet_length(name) <= 128),
  ADD CONSTRAINT pools_drain_state_known CHECK (
    drain_state IN ('accepting', 'graceful_drain', 'forced_drain', 'drained')
  ),
  ADD CONSTRAINT pools_admission_policy_object CHECK (jsonb_typeof(admission_policy) = 'object'),
  ADD CONSTRAINT pools_positive_concurrency CHECK (concurrency_limit > 0),
  ADD CONSTRAINT pools_static_capacity_bound CHECK (
    static_capacity_limit > 0 AND static_capacity_limit <= 10000
  ),
  ADD CONSTRAINT pools_concurrency_within_capacity CHECK (
    concurrency_limit <= static_capacity_limit
  );

CREATE INDEX pools_current_version_idx ON pools (id, version DESC);
