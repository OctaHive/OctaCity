-- Immutable per-link synchronization definitions, independently of workflow versions.
CREATE TABLE issue_source_mappings (
  project_id UUID NOT NULL,
  issue_id UUID NOT NULL REFERENCES issue_source_links(issue_id),
  version BIGINT NOT NULL CHECK(version > 0),
  document JSONB NOT NULL CHECK(jsonb_typeof(document) = 'object' AND octet_length(document::text) <= 131072),
  PRIMARY KEY(project_id,issue_id,version),
  FOREIGN KEY(project_id,issue_id) REFERENCES issues(project_id,id),
  CHECK(document->>'project_id' = project_id::text AND document->>'issue_id' = issue_id::text
    AND (document->>'version')::bigint = version)
);
