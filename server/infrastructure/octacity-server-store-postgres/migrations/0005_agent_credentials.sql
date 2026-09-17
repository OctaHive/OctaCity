CREATE TABLE agent_enrollment_credentials (
  id UUID PRIMARY KEY,
  credential_hash BYTEA NOT NULL,
  pool_id UUID NOT NULL,
  pool_version BIGINT NOT NULL,
  expected_operating_system TEXT,
  expected_architecture TEXT,
  issued_at TIMESTAMPTZ NOT NULL,
  expires_at TIMESTAMPTZ NOT NULL,
  consumed_registration_id UUID UNIQUE,
  consumed_at TIMESTAMPTZ,
  revoked_at TIMESTAMPTZ,
  FOREIGN KEY (pool_id, pool_version) REFERENCES pools(id, version),
  CONSTRAINT agent_enrollment_credentials_hash_length CHECK (octet_length(credential_hash) = 32),
  CONSTRAINT agent_enrollment_credentials_expiry CHECK (expires_at > issued_at),
  CONSTRAINT agent_enrollment_credentials_platform_pair CHECK (
    (expected_operating_system IS NULL AND expected_architecture IS NULL)
    OR (expected_operating_system IS NOT NULL AND expected_architecture IS NOT NULL)
  ),
  CONSTRAINT agent_enrollment_credentials_consumption_pair CHECK (
    (consumed_registration_id IS NULL AND consumed_at IS NULL)
    OR (consumed_registration_id IS NOT NULL AND consumed_at IS NOT NULL)
  )
);

ALTER TABLE agents
  ADD COLUMN platform_operating_system TEXT,
  ADD COLUMN platform_architecture TEXT,
  ADD CONSTRAINT agents_platform_pair CHECK (
    (platform_operating_system IS NULL AND platform_architecture IS NULL)
    OR (platform_operating_system IS NOT NULL AND platform_architecture IS NOT NULL)
  );

ALTER TABLE agent_registrations
  ADD CONSTRAINT agent_registrations_hash_length CHECK (octet_length(credential_hash) = 32),
  ADD CONSTRAINT agent_registrations_expiry CHECK (expires_at > registered_at);

CREATE UNIQUE INDEX agent_registrations_one_current_per_agent_idx
  ON agent_registrations (agent_id)
  WHERE revoked_at IS NULL;

ALTER TABLE agent_enrollment_credentials
  ADD CONSTRAINT agent_enrollment_credentials_consumed_registration_fk
  FOREIGN KEY (consumed_registration_id) REFERENCES agent_registrations(id);
