-- Versioned whole-aggregate retention holds are independent of the immutable
-- automatic-retention deadlines materialized with each Build.

CREATE TABLE build_result_retention_holds (
  build_id UUID NOT NULL REFERENCES builds(id),
  version BIGINT NOT NULL,
  reason TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  expires_at TIMESTAMPTZ,
  expired_at TIMESTAMPTZ,
  released_at TIMESTAMPTZ,
  actor_kind TEXT NOT NULL,
  actor_identity TEXT,
  request_identity TEXT NOT NULL,
  release_actor_kind TEXT,
  release_actor_identity TEXT,
  release_request_identity TEXT,
  PRIMARY KEY (build_id, version),
  CONSTRAINT build_result_retention_holds_version_positive CHECK (version > 0),
  CONSTRAINT build_result_retention_holds_reason_bounded CHECK (octet_length(reason) BETWEEN 1 AND 512),
  CONSTRAINT build_result_retention_holds_actor_kind_bounded CHECK (octet_length(actor_kind) BETWEEN 1 AND 64),
  CONSTRAINT build_result_retention_holds_actor_identity_bounded
    CHECK (actor_identity IS NULL OR octet_length(actor_identity) BETWEEN 1 AND 128),
  CONSTRAINT build_result_retention_holds_request_identity_bounded
    CHECK (octet_length(request_identity) BETWEEN 1 AND 128),
  CONSTRAINT build_result_retention_holds_expiry_ordered CHECK (expires_at IS NULL OR expires_at > created_at),
  CONSTRAINT build_result_retention_holds_expired_ordered
    CHECK (expired_at IS NULL OR (expires_at IS NOT NULL AND expired_at >= expires_at)),
  CONSTRAINT build_result_retention_holds_release_ordered CHECK (released_at IS NULL OR released_at >= created_at),
  CONSTRAINT build_result_retention_holds_terminal_shape CHECK (expired_at IS NULL OR released_at IS NULL),
  CONSTRAINT build_result_retention_holds_release_audit_shape CHECK (
    (released_at IS NULL AND release_actor_kind IS NULL AND release_actor_identity IS NULL
      AND release_request_identity IS NULL)
    OR
    (released_at IS NOT NULL AND release_actor_kind IS NOT NULL AND release_request_identity IS NOT NULL)
  ),
  CONSTRAINT build_result_retention_holds_release_actor_bounded
    CHECK (release_actor_kind IS NULL OR octet_length(release_actor_kind) BETWEEN 1 AND 64),
  CONSTRAINT build_result_retention_holds_release_identity_bounded
    CHECK (release_actor_identity IS NULL OR octet_length(release_actor_identity) BETWEEN 1 AND 128),
  CONSTRAINT build_result_retention_holds_release_request_bounded
    CHECK (release_request_identity IS NULL OR octet_length(release_request_identity) BETWEEN 1 AND 128)
);
