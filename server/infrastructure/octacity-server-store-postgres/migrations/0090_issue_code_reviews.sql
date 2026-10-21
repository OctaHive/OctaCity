-- Optional native code-review metadata. No task revision/status, candidate, or execution pointer changes.
CREATE TABLE issue_code_review_associations (
  project_id UUID NOT NULL,
  issue_id UUID NOT NULL,
  security_scope TEXT NOT NULL,
  id BYTEA NOT NULL CHECK (octet_length(id) = 32),
  repository_identity BYTEA NOT NULL CHECK (octet_length(repository_identity) = 32),
  sequence BIGINT NOT NULL CHECK (sequence > 0),
  document JSONB NOT NULL CHECK (jsonb_typeof(document) = 'object' AND octet_length(document::text) <= 32768),
  PRIMARY KEY (project_id, issue_id, security_scope, id),
  UNIQUE (project_id, issue_id, security_scope, sequence),
  FOREIGN KEY (project_id, issue_id) REFERENCES issues(project_id, id),
  CHECK (document->>'project_id' = project_id::text AND document->>'issue_id' = issue_id::text
    AND document->>'id' = encode(id, 'hex') AND (document->>'sequence')::bigint = sequence)
);
CREATE TABLE issue_code_review_observations (
  project_id UUID NOT NULL,
  issue_id UUID NOT NULL,
  security_scope TEXT NOT NULL,
  association_id BYTEA NOT NULL,
  id BYTEA NOT NULL CHECK (octet_length(id) = 32),
  sequence BIGINT NOT NULL CHECK (sequence > 0),
  observed_at BIGINT NOT NULL CHECK (observed_at >= 0),
  document JSONB NOT NULL CHECK (jsonb_typeof(document) = 'object' AND octet_length(document::text) <= 32768),
  PRIMARY KEY (project_id, issue_id, security_scope, association_id, sequence),
  UNIQUE (project_id, issue_id, security_scope, association_id, id),
  FOREIGN KEY (project_id, issue_id, security_scope, association_id)
    REFERENCES issue_code_review_associations(project_id, issue_id, security_scope, id),
  CHECK (document->>'association_id' = encode(association_id, 'hex') AND document->>'id' = encode(id, 'hex')
    AND (document->>'sequence')::bigint = sequence
    AND (document->'observation'->>'observed_at')::bigint = observed_at)
);
CREATE INDEX issue_code_review_latest ON issue_code_review_observations
  (project_id, issue_id, security_scope, association_id, observed_at DESC, sequence DESC);
