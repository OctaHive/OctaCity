-- Execution is optional: manual Issue creation has no execution prerequisites.
CREATE TABLE issue_execution_requests (
  id UUID PRIMARY KEY,
  project_id UUID NOT NULL,
  issue_id UUID NOT NULL,
  requirements_version BIGINT NOT NULL CHECK (requirements_version > 0),
  security_scope TEXT NOT NULL CHECK (octet_length(security_scope) BETWEEN 1 AND 128),
  document JSONB NOT NULL CHECK (jsonb_typeof(document) = 'object' AND octet_length(document::text) <= 16384),
  FOREIGN KEY (project_id, issue_id, requirements_version)
    REFERENCES issue_revisions(project_id, issue_id, version),
  UNIQUE (project_id, issue_id, id),
  CHECK (document->>'id' = id::text AND document->'intent'->>'project_id' = project_id::text
    AND document->'intent'->>'issue_id' = issue_id::text
    AND (document->'intent'->>'expected_version')::bigint = requirements_version)
);
CREATE INDEX issue_execution_requests_issue ON issue_execution_requests(project_id, issue_id, id);
