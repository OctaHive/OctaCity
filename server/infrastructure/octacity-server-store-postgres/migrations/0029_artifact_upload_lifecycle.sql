-- Atomic, idempotent logical upload reservations for the Artifact application service.

ALTER TABLE artifact_uploads
  ADD COLUMN artifact_type TEXT,
  ADD COLUMN transport_media_type TEXT,
  ADD COLUMN producer_run_id TEXT,
  ADD COLUMN producer_task_id TEXT;

UPDATE artifact_uploads
SET artifact_type = CASE WHEN report_format IS NULL THEN 'artifact' ELSE 'report' END,
    transport_media_type = media_type,
    producer_run_id = '0',
    producer_task_id = '0';

ALTER TABLE artifact_uploads
  ALTER COLUMN artifact_type SET NOT NULL,
  ALTER COLUMN transport_media_type SET NOT NULL,
  ALTER COLUMN producer_run_id SET NOT NULL,
  ALTER COLUMN producer_task_id SET NOT NULL,
  ALTER COLUMN object_identity DROP NOT NULL,
  ADD CONSTRAINT artifact_uploads_non_negative_size CHECK (expected_size >= 0),
  ADD CONSTRAINT artifact_uploads_sha256_size CHECK (octet_length(expected_sha256) = 32),
  ADD CONSTRAINT artifact_uploads_transport_media_type_size CHECK (
    octet_length(transport_media_type) BETWEEN 1 AND 256
  ),
  ADD CONSTRAINT artifact_uploads_producer_ids CHECK (
    producer_run_id ~ '^(0|[1-9][0-9]{0,19})$'
    AND producer_task_id ~ '^(0|[1-9][0-9]{0,19})$'
  ),
  ADD CONSTRAINT artifact_uploads_type_shape CHECK (
    (artifact_type = 'artifact' AND report_format IS NULL)
    OR (
      artifact_type = 'report'
      AND report_format IS NOT NULL
      AND octet_length(report_format) BETWEEN 1 AND 256
    )
  ),
  ADD CONSTRAINT artifact_uploads_state CHECK (state IN ('pending', 'verifying', 'published', 'deleted')),
  ADD CONSTRAINT artifact_uploads_times CHECK (
    expires_at > created_at
    AND (completed_at IS NULL OR completed_at >= created_at)
  );

CREATE UNIQUE INDEX artifact_uploads_lease_idempotency_key
  ON artifact_uploads (lease_id, idempotency_key);

CREATE UNIQUE INDEX artifact_uploads_artifact_key
  ON artifact_uploads (artifact_id);
