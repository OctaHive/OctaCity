-- Credential-free recovery journal. Raw material belongs only in protected storage.
-- Existing metadata remains disabled until explicit, authorized administration.
CREATE TABLE integration_administration (
  connection_id UUID PRIMARY KEY REFERENCES integration_connections(id),
  revision BIGINT NOT NULL CHECK (revision > 0),
  document JSONB NOT NULL CHECK (jsonb_typeof(document) = 'object')
);
CREATE TABLE integration_administration_operations (
  operation BYTEA PRIMARY KEY CHECK (octet_length(operation) = 32),
  connection_id UUID NOT NULL REFERENCES integration_connections(id),
  metadata_version BIGINT NOT NULL CHECK (metadata_version > 0),
  security_scope TEXT NOT NULL,
  idempotency_key TEXT NOT NULL,
  requested_at BIGINT NOT NULL,
  receipt JSONB NOT NULL CHECK (jsonb_typeof(receipt) = 'object'),
  cleaned BOOLEAN NOT NULL DEFAULT FALSE,
  UNIQUE (security_scope, idempotency_key),
  FOREIGN KEY (connection_id, metadata_version)
    REFERENCES integration_connection_versions(connection_id, version)
);
CREATE INDEX integration_preparation_cleanup ON integration_administration_operations
  (connection_id, requested_at, operation) WHERE NOT cleaned AND receipt->'publication' <> 'null'::jsonb;
-- One authority owns writes to a logical handle, even across connection versions.
CREATE TABLE integration_credential_owners (
  reference JSONB PRIMARY KEY CHECK (jsonb_typeof(reference) = 'object'),
  connection_id UUID NOT NULL REFERENCES integration_connections(id)
);
