-- Additive metadata only: no existing installations, repositories, inputs,
-- credentials or Project-use authorities are inferred or migrated.
CREATE TABLE integration_connections (
  id UUID PRIMARY KEY,
  current_version BIGINT NOT NULL CHECK (current_version > 0)
);

CREATE TABLE integration_connection_versions (
  connection_id UUID NOT NULL REFERENCES integration_connections(id),
  version BIGINT NOT NULL CHECK (version > 0),
  definition_digest BYTEA NOT NULL CHECK (octet_length(definition_digest) = 32),
  document JSONB NOT NULL CHECK (jsonb_typeof(document) = 'object'),
  PRIMARY KEY (connection_id, version)
);

ALTER TABLE integration_connections ADD CONSTRAINT integration_connections_current_version_fk
  FOREIGN KEY (id, current_version)
  REFERENCES integration_connection_versions(connection_id, version)
  DEFERRABLE INITIALLY DEFERRED;
