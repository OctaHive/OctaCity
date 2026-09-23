-- Complete the previously reserved cache_sessions table for the task 7.5
-- fenced session lifecycle. No released server wrote this table before this
-- migration, so the new authoritative columns intentionally have no legacy
-- fallback values.

ALTER TABLE cache_sessions
  ADD COLUMN request_identity TEXT NOT NULL,
  ADD COLUMN scope_id TEXT NOT NULL,
  ADD COLUMN agent_id UUID NOT NULL REFERENCES agents(id),
  ADD COLUMN registration_epoch BIGINT NOT NULL,
  ADD COLUMN retention_until TIMESTAMPTZ NOT NULL;

ALTER TABLE cache_sessions
  ADD CONSTRAINT cache_sessions_request_identity_nonempty
    CHECK (length(request_identity) BETWEEN 1 AND 128),
  ADD CONSTRAINT cache_sessions_scope_id_shape
    CHECK (scope_id ~ '^[0-9a-f]{64}$'),
  ADD CONSTRAINT cache_sessions_namespace_bounded
    CHECK (octet_length(namespace) BETWEEN 1 AND 128),
  ADD CONSTRAINT cache_sessions_permissions_shape
    CHECK (permissions IN (
      '{"read": true, "write": false}'::jsonb,
      '{"read": false, "write": true}'::jsonb,
      '{"read": true, "write": true}'::jsonb
    )),
  ADD CONSTRAINT cache_sessions_credential_hash_shape
    CHECK (octet_length(credential_hash) = 32),
  ADD CONSTRAINT cache_sessions_registration_epoch_positive
    CHECK (registration_epoch > 0),
  ADD CONSTRAINT cache_sessions_quota_positive
    CHECK (quota_bytes > 0),
  ADD CONSTRAINT cache_sessions_expiry_after_creation
    CHECK (expires_at > created_at),
  ADD CONSTRAINT cache_sessions_retention_after_creation
    CHECK (retention_until > created_at),
  ADD CONSTRAINT cache_sessions_state_shape
    CHECK (state IN ('active', 'revoked', 'expired')),
  ADD CONSTRAINT cache_sessions_revocation_shape
    CHECK ((state = 'revoked') = (revoked_at IS NOT NULL)),
  ADD CONSTRAINT cache_sessions_lease_request_unique
    UNIQUE (lease_id, request_identity);

CREATE INDEX cache_sessions_build_created_idx
  ON cache_sessions (build_id, created_at, id);

CREATE INDEX cache_sessions_active_authorization_idx
  ON cache_sessions (id, expires_at)
  WHERE state = 'active';
