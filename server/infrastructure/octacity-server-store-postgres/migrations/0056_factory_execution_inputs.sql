-- Private compiler documents belong to a persisted node operation, not an Agent Lease.
CREATE TABLE factory_execution_input_manifests (
  operation_id BYTEA PRIMARY KEY CHECK (octet_length(operation_id) = 32),
  run_id UUID NOT NULL,
  manifest_digest BYTEA NOT NULL CHECK (octet_length(manifest_digest) = 32),
  reserved_at_millis BIGINT NOT NULL CHECK (reserved_at_millis >= 0),
  published_at_millis BIGINT CHECK (published_at_millis >= reserved_at_millis),
  expires_at_millis BIGINT NOT NULL CHECK (expires_at_millis > reserved_at_millis),
  FOREIGN KEY (run_id, operation_id) REFERENCES factory_flow_build_intents(run_id, operation_id)
);
CREATE TABLE factory_execution_inputs (
  artifact_id UUID PRIMARY KEY,
  upload_id UUID NOT NULL UNIQUE,
  operation_id BYTEA NOT NULL REFERENCES factory_execution_input_manifests(operation_id),
  size_bytes BIGINT NOT NULL CHECK (size_bytes BETWEEN 0 AND 67108864),
  content_digest BYTEA NOT NULL CHECK (octet_length(content_digest) = 32),
  media_type TEXT NOT NULL CHECK (octet_length(media_type) BETWEEN 1 AND 255),
  deleted_at_millis BIGINT CHECK (deleted_at_millis >= 0)
);
CREATE INDEX factory_execution_inputs_expiry ON factory_execution_input_manifests(expires_at_millis, operation_id);
