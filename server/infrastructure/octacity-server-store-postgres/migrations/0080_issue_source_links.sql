-- One immutable primary source per task; native identity includes the accepted partition.
CREATE TABLE issue_source_links (
  project_id UUID NOT NULL,
  issue_id UUID NOT NULL UNIQUE,
  security_scope TEXT NOT NULL CHECK (octet_length(security_scope) BETWEEN 1 AND 256),
  native_correlation BYTEA NOT NULL CHECK (octet_length(native_correlation) = 32),
  document JSONB NOT NULL CHECK (jsonb_typeof(document) = 'object' AND octet_length(document::text) <= 16384),
  PRIMARY KEY(project_id,security_scope,native_correlation),
  FOREIGN KEY(project_id,issue_id) REFERENCES issues(project_id,id),
  CHECK (document->>'issue_id' = issue_id::text AND document->'selection'->>'project_id' = project_id::text
    AND document->>'native_correlation' = encode(native_correlation,'hex'))
);
