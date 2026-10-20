-- Lifetime allocation is independent of retained bodies and Issue revision/status.
ALTER TABLE issues ADD COLUMN discussion_count BIGINT NOT NULL DEFAULT 0
  CHECK (discussion_count BETWEEN 0 AND 1000);
CREATE TABLE issue_comments (
  project_id UUID NOT NULL,
  issue_id UUID NOT NULL,
  id UUID PRIMARY KEY,
  sequence BIGINT NOT NULL CHECK (sequence BETWEEN 1 AND 1000),
  parent_id UUID,
  metadata JSONB NOT NULL CHECK (jsonb_typeof(metadata) = 'object' AND octet_length(metadata::text) <= 16384),
  body TEXT NOT NULL CHECK (octet_length(body) BETWEEN 1 AND 16384),
  UNIQUE (project_id, issue_id, sequence),
  UNIQUE (project_id, issue_id, id),
  FOREIGN KEY (project_id, issue_id) REFERENCES issues(project_id, id),
  FOREIGN KEY (project_id, issue_id, parent_id) REFERENCES issue_comments(project_id, issue_id, id),
  CHECK (metadata->>'project_id' = project_id::text AND metadata->>'issue_id' = issue_id::text
    AND metadata->>'id' = id::text AND (metadata->>'sequence')::bigint = sequence
    AND (metadata->>'parent_id')::uuid IS NOT DISTINCT FROM parent_id)
);
