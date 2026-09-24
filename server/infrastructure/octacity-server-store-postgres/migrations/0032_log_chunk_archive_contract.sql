-- Enforce backend-neutral immutable log-manifest shape. Rust owns sequence
-- coverage and Project-local position allocation inside the locked append
-- operation; PostgreSQL retains declarative structural protection.

ALTER TABLE log_chunk_manifests
  ADD CONSTRAINT log_chunk_manifests_stream_kind CHECK (stream IN ('stdout', 'stderr')),
  ADD CONSTRAINT log_chunk_manifests_positive_sequence_range
    CHECK (first_sequence > 0 AND last_sequence >= first_sequence),
  ADD CONSTRAINT log_chunk_manifests_bounded_bytes
    CHECK (byte_length > 0 AND byte_length <= 262144),
  ADD CONSTRAINT log_chunk_manifests_sha256_length CHECK (octet_length(sha256) = 32),
  ADD CONSTRAINT log_chunk_manifests_visibility_shape
    CHECK ((visible AND deleted_at IS NULL) OR (NOT visible)),
  ADD CONSTRAINT log_chunk_manifests_object_identity_key UNIQUE (object_identity),
  ADD CONSTRAINT log_chunk_manifests_job_stream_range_key
    UNIQUE (job_id, stream, first_sequence, last_sequence);

CREATE INDEX log_chunk_manifests_visible_build_order_idx
  ON log_chunk_manifests (build_id, attempt_id, job_id, first_sequence)
  WHERE visible AND deleted_at IS NULL;
