-- Materialize the rebuildable PostgreSQL Build-log search projection without
-- coupling it to authoritative foreign keys. The projection can be emptied
-- and rebuilt while Builds, manifests, and durable work remain authoritative.

CREATE EXTENSION IF NOT EXISTS pg_trgm;

ALTER TABLE log_indexing_work
  ADD COLUMN last_claim_owner TEXT,
  ADD COLUMN last_failure_at TIMESTAMPTZ,
  ADD COLUMN last_retry_at TIMESTAMPTZ,
  ADD CONSTRAINT log_indexing_work_state_kind
    CHECK (state IN ('pending', 'claimed', 'retry_scheduled', 'completed', 'dead_letter')),
  ADD CONSTRAINT log_indexing_work_claim_shape CHECK (
    (state = 'claimed' AND claim_owner IS NOT NULL AND claim_expires_at IS NOT NULL)
    OR (state <> 'claimed' AND claim_owner IS NULL AND claim_expires_at IS NULL)
  ),
  ADD CONSTRAINT log_indexing_work_failure_shape CHECK (
    (last_retry_at IS NULL OR last_failure_at IS NOT NULL)
    AND (state <> 'retry_scheduled' OR last_retry_at IS NOT NULL)
  );

ALTER TABLE log_search_documents
  DROP CONSTRAINT log_search_documents_chunk_id_fkey,
  DROP CONSTRAINT log_search_documents_build_id_fkey,
  DROP CONSTRAINT log_search_documents_attempt_id_fkey,
  DROP CONSTRAINT log_search_documents_job_id_fkey,
  ADD COLUMN project_id UUID,
  ADD COLUMN occurred_at TIMESTAMPTZ;

UPDATE log_search_documents AS document
  SET project_id = build.project_id,
      occurred_at = manifest.created_at
  FROM builds AS build, log_chunk_manifests AS manifest
  WHERE build.id = document.build_id
    AND manifest.id = document.chunk_id;

ALTER TABLE log_search_documents
  ALTER COLUMN project_id SET NOT NULL,
  ALTER COLUMN occurred_at SET NOT NULL,
  ADD CONSTRAINT log_search_documents_chunk_key UNIQUE (chunk_id),
  ADD CONSTRAINT log_search_documents_stream_kind CHECK (stream IN ('stdout', 'stderr')),
  ADD CONSTRAINT log_search_documents_positive_sequence_range
    CHECK (first_sequence > 0 AND last_sequence >= first_sequence),
  ADD CONSTRAINT log_search_documents_bounded_text
    CHECK (octet_length(normalized_text) BETWEEN 1 AND 262144),
  ADD COLUMN search_vector TSVECTOR
    GENERATED ALWAYS AS (to_tsvector('simple', normalized_text)) STORED;

CREATE INDEX log_search_documents_full_text_idx
  ON log_search_documents USING GIN (search_vector);
CREATE INDEX log_search_documents_literal_trgm_idx
  ON log_search_documents USING GIN (normalized_text gin_trgm_ops);
CREATE INDEX log_search_documents_project_order_idx
  ON log_search_documents (project_id, occurred_at DESC, chunk_id DESC);
CREATE INDEX log_search_documents_scope_idx
  ON log_search_documents (project_id, build_id, attempt_id, job_id, stream, occurred_at DESC);

ALTER TABLE log_search_build_tombstones
  DROP CONSTRAINT log_search_build_tombstones_project_id_fkey,
  DROP CONSTRAINT log_search_build_tombstones_build_id_fkey;

CREATE TABLE log_search_applied_work (
  work_id UUID PRIMARY KEY,
  project_id UUID NOT NULL,
  position BIGINT NOT NULL,
  operation TEXT NOT NULL,
  request_digest BYTEA NOT NULL,
  disposition TEXT NOT NULL,
  applied_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  CONSTRAINT log_search_applied_work_positive_position CHECK (position > 0),
  CONSTRAINT log_search_applied_work_project_position_unique UNIQUE (project_id, position),
  CONSTRAINT log_search_applied_work_operation_kind CHECK (operation IN ('index', 'delete_build', 'rebuild')),
  CONSTRAINT log_search_applied_work_digest_length CHECK (octet_length(request_digest) = 32),
  CONSTRAINT log_search_applied_work_disposition_kind CHECK (disposition IN ('applied', 'superseded'))
);

CREATE TABLE log_search_project_positions (
  project_id UUID PRIMARY KEY,
  indexed_through BIGINT NOT NULL DEFAULT 0,
  CONSTRAINT log_search_project_positions_non_negative CHECK (indexed_through >= 0)
);
