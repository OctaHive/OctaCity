-- Materialize immutable Build Result deadlines and make every automatic
-- retention transition resumable from authoritative state.

ALTER TABLE builds
  ADD COLUMN metadata_retention_until TIMESTAMPTZ,
  ADD COLUMN log_retention_until TIMESTAMPTZ,
  ADD COLUMN artifact_retention_until TIMESTAMPTZ,
  ADD COLUMN report_retention_until TIMESTAMPTZ,
  ADD COLUMN metadata_visible BOOLEAN NOT NULL DEFAULT true,
  ADD COLUMN metadata_deleted_at TIMESTAMPTZ,
  ADD COLUMN logs_visible BOOLEAN NOT NULL DEFAULT true,
  ADD COLUMN logs_hidden_at TIMESTAMPTZ,
  ADD COLUMN artifacts_visible BOOLEAN NOT NULL DEFAULT true,
  ADD COLUMN artifacts_hidden_at TIMESTAMPTZ,
  ADD COLUMN reports_visible BOOLEAN NOT NULL DEFAULT true,
  ADD COLUMN reports_hidden_at TIMESTAMPTZ;

-- Existing rows use the immutable effective-policy snapshot captured with the
-- Build. Malformed pre-contract snapshots expire immediately instead of
-- silently retaining data forever.
UPDATE builds
SET metadata_retention_until = created_at +
      COALESCE(NULLIF(effective_policy_snapshot #>> '{project,policy,retention,build_seconds}', ''), '0')::BIGINT
        * INTERVAL '1 second',
    log_retention_until = created_at +
      COALESCE(NULLIF(effective_policy_snapshot #>> '{project,policy,retention,log_seconds}', ''), '0')::BIGINT
        * INTERVAL '1 second',
    artifact_retention_until = created_at +
      COALESCE(NULLIF(effective_policy_snapshot #>> '{project,policy,retention,artifact_seconds}', ''), '0')::BIGINT
        * INTERVAL '1 second',
    report_retention_until = created_at +
      COALESCE(NULLIF(effective_policy_snapshot #>> '{project,policy,retention,artifact_seconds}', ''), '0')::BIGINT
        * INTERVAL '1 second';

ALTER TABLE builds
  ALTER COLUMN metadata_retention_until SET NOT NULL,
  ALTER COLUMN log_retention_until SET NOT NULL,
  ALTER COLUMN artifact_retention_until SET NOT NULL,
  ALTER COLUMN report_retention_until SET NOT NULL,
  ADD CONSTRAINT builds_retention_deadlines_ordered CHECK (
    metadata_retention_until >= created_at
    AND log_retention_until >= created_at
    AND artifact_retention_until >= created_at
    AND report_retention_until >= created_at
  ),
  ADD CONSTRAINT builds_metadata_visibility_shape CHECK (
    (metadata_visible AND metadata_deleted_at IS NULL) OR NOT metadata_visible
  ),
  ADD CONSTRAINT builds_log_visibility_shape CHECK (
    (logs_visible AND logs_hidden_at IS NULL) OR (NOT logs_visible AND logs_hidden_at IS NOT NULL)
  ),
  ADD CONSTRAINT builds_artifact_visibility_shape CHECK (
    (artifacts_visible AND artifacts_hidden_at IS NULL)
    OR (NOT artifacts_visible AND artifacts_hidden_at IS NOT NULL)
  ),
  ADD CONSTRAINT builds_report_visibility_shape CHECK (
    (reports_visible AND reports_hidden_at IS NULL) OR (NOT reports_visible AND reports_hidden_at IS NOT NULL)
  );

ALTER TABLE log_chunk_manifests
  ADD COLUMN hidden_at TIMESTAMPTZ;

UPDATE log_chunk_manifests
SET hidden_at = COALESCE(deleted_at, created_at)
WHERE NOT visible;

ALTER TABLE log_chunk_manifests
  ADD CONSTRAINT log_chunk_retention_visibility_shape CHECK (
    (visible AND hidden_at IS NULL AND deleted_at IS NULL)
    OR (NOT visible AND hidden_at IS NOT NULL)
  );

UPDATE artifacts AS artifact
SET retention_until = CASE
  WHEN artifact.artifact_type = 'report' THEN build.report_retention_until
  ELSE build.artifact_retention_until
END
FROM builds AS build
WHERE build.id = artifact.build_id
  AND artifact.retention_until IS NULL;

ALTER TABLE retention_work
  ADD COLUMN project_id UUID REFERENCES projects(id),
  ADD COLUMN build_id UUID REFERENCES builds(id),
  ADD COLUMN deadline_at TIMESTAMPTZ,
  ADD COLUMN search_work_id UUID,
  ADD COLUMN search_position BIGINT,
  ADD COLUMN last_error_code TEXT,
  ADD COLUMN last_failure_at TIMESTAMPTZ,
  ADD COLUMN last_claim_owner TEXT,
  ADD COLUMN last_retry_at TIMESTAMPTZ;

UPDATE retention_work SET deadline_at = available_at WHERE build_id IS NOT NULL;

INSERT INTO retention_work
  (id, resource_kind, resource_identity, phase, available_at, deadline_at, attempt_count, project_id, build_id)
SELECT md5('octacity.retention.v1:' || build.id::TEXT || ':' || component.kind)::UUID,
       component.kind, build.id::TEXT, 'pending', component.deadline, component.deadline, 0,
       build.project_id, build.id
FROM builds AS build
CROSS JOIN LATERAL (VALUES
  ('metadata', build.metadata_retention_until),
  ('logs', build.log_retention_until),
  ('artifacts', build.artifact_retention_until),
  ('reports', build.report_retention_until)
) AS component(kind, deadline)
ON CONFLICT DO NOTHING;

CREATE UNIQUE INDEX retention_work_build_component_key
  ON retention_work (build_id, resource_kind)
  WHERE build_id IS NOT NULL;

CREATE INDEX retention_work_due_claim_idx
  ON retention_work (available_at, id)
  WHERE completed_at IS NULL;

ALTER TABLE retention_work
  ADD CONSTRAINT retention_work_build_deadline_shape CHECK (
    (build_id IS NULL AND deadline_at IS NULL) OR (build_id IS NOT NULL AND deadline_at IS NOT NULL)
  ),
  ADD CONSTRAINT retention_work_search_shape CHECK (
    (search_work_id IS NULL AND search_position IS NULL)
    OR (resource_kind = 'logs' AND search_work_id IS NOT NULL AND search_position > 0)
  );

CREATE TABLE orphan_log_chunk_work (
  chunk_id UUID PRIMARY KEY,
  job_id UUID NOT NULL REFERENCES jobs(id),
  stream TEXT NOT NULL,
  first_sequence BIGINT NOT NULL,
  last_sequence BIGINT NOT NULL,
  object_identity TEXT NOT NULL,
  byte_length BIGINT NOT NULL,
  sha256 BYTEA NOT NULL,
  state TEXT NOT NULL DEFAULT 'pending',
  attempt_count BIGINT NOT NULL DEFAULT 0,
  available_at TIMESTAMPTZ NOT NULL,
  claim_owner TEXT,
  claim_expires_at TIMESTAMPTZ,
  last_claim_owner TEXT,
  last_error_code TEXT,
  last_failure_at TIMESTAMPTZ,
  last_retry_at TIMESTAMPTZ,
  completed_at TIMESTAMPTZ,
  CONSTRAINT orphan_log_chunk_work_stream CHECK (stream IN ('stdout', 'stderr')),
  CONSTRAINT orphan_log_chunk_work_sequence CHECK (first_sequence > 0 AND last_sequence >= first_sequence),
  CONSTRAINT orphan_log_chunk_work_bytes CHECK (byte_length > 0 AND octet_length(sha256) = 32),
  CONSTRAINT orphan_log_chunk_work_state CHECK (state IN ('pending', 'claimed', 'retry_scheduled', 'completed', 'dead_letter'))
);

CREATE INDEX orphan_log_chunk_work_due_idx
  ON orphan_log_chunk_work (available_at, chunk_id)
  WHERE state IN ('pending', 'claimed', 'retry_scheduled');
