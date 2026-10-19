-- Safe immutable read receipts keep original installed mapping/provenance, never native bodies.
CREATE TABLE issue_source_observations (
  project_id UUID NOT NULL,
  security_scope TEXT NOT NULL,
  native_correlation BYTEA NOT NULL,
  id BYTEA NOT NULL CHECK (octet_length(id) = 32),
  document JSONB NOT NULL CHECK (jsonb_typeof(document) = 'object' AND octet_length(document::text) <= 16384),
  PRIMARY KEY(project_id,security_scope,native_correlation,id),
  FOREIGN KEY(project_id,security_scope,native_correlation)
    REFERENCES issue_source_links(project_id,security_scope,native_correlation)
);
