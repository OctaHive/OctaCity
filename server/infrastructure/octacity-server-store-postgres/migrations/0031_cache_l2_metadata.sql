-- Authoritative visibility and quota metadata for Octa HTTP cache v1.
-- Immutable bytes remain in the configured object adapter and become visible
-- only after a successful cache_blobs insert commits.

ALTER TABLE cache_sessions
  ADD CONSTRAINT cache_sessions_credential_hash_unique UNIQUE (credential_hash);

CREATE TABLE cache_blobs (
  scope_id TEXT NOT NULL,
  project_id UUID NOT NULL REFERENCES projects(id),
  namespace TEXT NOT NULL,
  blob_key TEXT NOT NULL,
  descriptor JSONB NOT NULL,
  charged_bytes BIGINT NOT NULL,
  retention_until TIMESTAMPTZ NOT NULL,
  reserved_at TIMESTAMPTZ NOT NULL,
  published_at TIMESTAMPTZ,
  PRIMARY KEY (scope_id, blob_key),
  CONSTRAINT cache_blobs_scope_shape CHECK (scope_id ~ '^[0-9a-f]{64}$'),
  CONSTRAINT cache_blobs_namespace_bounded CHECK (octet_length(namespace) BETWEEN 1 AND 128),
  CONSTRAINT cache_blobs_key_nonempty CHECK (length(blob_key) > 0),
  CONSTRAINT cache_blobs_descriptor_object CHECK (jsonb_typeof(descriptor) = 'object'),
  CONSTRAINT cache_blobs_charged_bytes_positive CHECK (charged_bytes > 0),
  CONSTRAINT cache_blobs_retention_after_reservation CHECK (retention_until > reserved_at),
  CONSTRAINT cache_blobs_publication_order CHECK (published_at IS NULL OR published_at >= reserved_at)
);

CREATE INDEX cache_blobs_project_quota_idx ON cache_blobs (project_id);
CREATE INDEX cache_blobs_retention_idx ON cache_blobs (retention_until, scope_id, blob_key);

CREATE TABLE cache_actions (
  scope_id TEXT NOT NULL,
  project_id UUID NOT NULL REFERENCES projects(id),
  namespace TEXT NOT NULL,
  action_key TEXT NOT NULL,
  result JSONB NOT NULL,
  blob_key TEXT,
  charged_bytes BIGINT NOT NULL,
  retention_until TIMESTAMPTZ NOT NULL,
  published_at TIMESTAMPTZ NOT NULL,
  PRIMARY KEY (scope_id, action_key),
  CONSTRAINT cache_actions_scope_shape CHECK (scope_id ~ '^[0-9a-f]{64}$'),
  CONSTRAINT cache_actions_namespace_bounded CHECK (octet_length(namespace) BETWEEN 1 AND 128),
  CONSTRAINT cache_actions_key_nonempty CHECK (length(action_key) > 0),
  CONSTRAINT cache_actions_result_object CHECK (jsonb_typeof(result) = 'object'),
  CONSTRAINT cache_actions_charged_bytes_positive CHECK (charged_bytes > 0),
  CONSTRAINT cache_actions_retention_after_publication CHECK (retention_until > published_at),
  CONSTRAINT cache_actions_blob_reference
    FOREIGN KEY (scope_id, blob_key) REFERENCES cache_blobs(scope_id, blob_key)
);

CREATE INDEX cache_actions_project_quota_idx ON cache_actions (project_id);
CREATE INDEX cache_actions_retention_idx ON cache_actions (retention_until, scope_id, action_key);
CREATE INDEX cache_actions_blob_idx ON cache_actions (scope_id, blob_key) WHERE blob_key IS NOT NULL;
