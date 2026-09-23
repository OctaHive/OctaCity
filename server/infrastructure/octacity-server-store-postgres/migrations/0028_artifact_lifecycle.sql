-- Declarative shape and race constraints for Rust-owned Artifact lifecycle decisions.

DROP INDEX artifacts_published_job_name_idx;

ALTER TABLE artifacts
  ADD COLUMN artifact_type TEXT,
  ADD COLUMN lease_id UUID REFERENCES leases(id),
  ADD COLUMN version BIGINT,
  ADD COLUMN created_at TIMESTAMPTZ;

UPDATE artifacts
SET artifact_type = CASE WHEN report_format IS NULL THEN 'artifact' ELSE 'report' END,
    lease_id = (
      SELECT lease.id
      FROM leases AS lease
      WHERE lease.job_id = artifacts.job_id
      ORDER BY lease.leased_at
      LIMIT 1
    ),
    version = 1,
    created_at = COALESCE(published_at, deleted_at, now());

ALTER TABLE artifacts
  ALTER COLUMN artifact_type SET NOT NULL,
  ALTER COLUMN lease_id SET NOT NULL,
  ALTER COLUMN version SET NOT NULL,
  ALTER COLUMN created_at SET NOT NULL,
  ALTER COLUMN object_identity DROP NOT NULL,
  ALTER COLUMN object_generation DROP NOT NULL,
  ADD CONSTRAINT artifacts_positive_version CHECK (version > 0),
  ADD CONSTRAINT artifacts_non_negative_size CHECK (byte_length >= 0),
  ADD CONSTRAINT artifacts_sha256_size CHECK (octet_length(sha256) = 32),
  ADD CONSTRAINT artifacts_logical_name_size CHECK (octet_length(logical_name) BETWEEN 1 AND 256),
  ADD CONSTRAINT artifacts_media_type_size CHECK (octet_length(media_type) BETWEEN 1 AND 256),
  ADD CONSTRAINT artifacts_type_shape CHECK (
    (artifact_type = 'artifact' AND report_format IS NULL)
    OR (
      artifact_type = 'report'
      AND report_format IS NOT NULL
      AND octet_length(report_format) BETWEEN 1 AND 256
    )
  ),
  ADD CONSTRAINT artifacts_state CHECK (state IN ('pending', 'verifying', 'published', 'expired', 'deleted')),
  ADD CONSTRAINT artifacts_lifecycle_shape CHECK (
    (state IN ('pending', 'verifying') AND published_at IS NULL AND deleted_at IS NULL)
    OR (state IN ('published', 'expired') AND published_at IS NOT NULL AND deleted_at IS NULL)
    OR (state = 'deleted' AND deleted_at IS NOT NULL)
  ),
  ADD CONSTRAINT artifacts_timestamps_ordered CHECK (
    (retention_until IS NULL OR retention_until > created_at)
    AND (published_at IS NULL OR published_at >= created_at)
    AND (deleted_at IS NULL OR deleted_at >= created_at)
    AND (published_at IS NULL OR deleted_at IS NULL OR deleted_at >= published_at)
  ),
  ADD CONSTRAINT artifacts_expiry_has_policy CHECK (state <> 'expired' OR retention_until IS NOT NULL);

CREATE UNIQUE INDEX artifacts_job_logical_name_key ON artifacts (job_id, logical_name);
