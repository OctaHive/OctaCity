-- Factory recovery and retention metadata. Factory cleanup releases logical
-- references; the existing Build retention worker remains the sole owner of
-- physical Artifact deletion.

ALTER TABLE factory_runs
  ADD COLUMN visible BOOLEAN NOT NULL DEFAULT true,
  ADD COLUMN hidden_at TIMESTAMPTZ;

ALTER TABLE factory_runs
  ADD CONSTRAINT factory_runs_visibility_shape CHECK (
    (visible AND hidden_at IS NULL) OR (NOT visible AND hidden_at IS NOT NULL)
  );

CREATE TABLE factory_artifact_references (
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  -- Some Factory inputs are immutable objects supplied by an intake adapter
  -- rather than artifacts produced by an OctaCity Build. Restore reconciliation
  -- validates their committed metadata and digest, so this is intentionally not
  -- a foreign key to artifacts.
  artifact_id UUID NOT NULL,
  role TEXT NOT NULL,
  expected_sha256 BYTEA,
  created_at TIMESTAMPTZ NOT NULL,
  released_at TIMESTAMPTZ,
  PRIMARY KEY (run_id, artifact_id, role),
  CONSTRAINT factory_artifact_references_role_known CHECK (
    role IN ('task', 'acceptance', 'specification', 'build_output', 'changeset_bundle',
             'changeset_manifest', 'evidence')
  ),
  CONSTRAINT factory_artifact_references_digest_shape CHECK (
    expected_sha256 IS NULL OR octet_length(expected_sha256) = 32
  ),
  CONSTRAINT factory_artifact_references_release_order CHECK (
    released_at IS NULL OR released_at >= created_at
  )
);

CREATE TABLE factory_retention_work (
  run_id UUID PRIMARY KEY REFERENCES factory_runs(id),
  phase TEXT NOT NULL DEFAULT 'pending',
  available_at TIMESTAMPTZ NOT NULL,
  attempt_count BIGINT NOT NULL DEFAULT 0,
  claim_owner TEXT,
  claim_expires_at TIMESTAMPTZ,
  last_claim_owner TEXT,
  last_error_code TEXT,
  last_failure_at TIMESTAMPTZ,
  last_retry_at TIMESTAMPTZ,
  completed_at TIMESTAMPTZ,
  CONSTRAINT factory_retention_work_phase_known CHECK (
    phase IN ('pending', 'hidden', 'metadata', 'completed', 'dead_letter')
  ),
  CONSTRAINT factory_retention_work_attempt_count CHECK (attempt_count >= 0),
  CONSTRAINT factory_retention_work_claim_shape CHECK (
    (claim_owner IS NULL AND claim_expires_at IS NULL)
    OR (claim_owner IS NOT NULL AND claim_expires_at IS NOT NULL)
  ),
  CONSTRAINT factory_retention_work_owner_shape CHECK (
    claim_owner IS NULL OR octet_length(claim_owner) BETWEEN 1 AND 128
  ),
  CONSTRAINT factory_retention_work_failure_shape CHECK (
    last_error_code IS NULL OR octet_length(last_error_code) BETWEEN 1 AND 128
  )
);

CREATE INDEX factory_artifact_references_live_artifact_idx
  ON factory_artifact_references (artifact_id, run_id)
  WHERE released_at IS NULL;

CREATE INDEX factory_retention_work_due_idx
  ON factory_retention_work (available_at, run_id)
  WHERE completed_at IS NULL;

-- Schema 47 and 48 ship in the same server release. The immediately previous
-- binary supports schema 46, where Factory tables do not exist, so no partial
-- Factory history can legitimately require a 47 -> 48 data backfill.
