-- Keep metadata, lifetime allocation, foreign keys and replay after body expiry.
ALTER TABLE issue_comments ALTER COLUMN body DROP NOT NULL;
CREATE TABLE issue_content_retention (
  project_id UUID NOT NULL,
  issue_id UUID NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('requirements','discussion')),
  PRIMARY KEY (issue_id,kind),
  version BIGINT NOT NULL CHECK (version > 0),
  document JSONB NOT NULL CHECK (jsonb_typeof(document)='object' AND octet_length(document::text)<=16384),
  expires_at_millis BIGINT GENERATED ALWAYS AS ((document->'plan'->>'expires_at')::bigint) STORED,
  completed BOOLEAN NOT NULL DEFAULT FALSE,
  FOREIGN KEY (project_id,issue_id) REFERENCES issues(project_id,id),
  CHECK (document->>'project_id'=project_id::text AND document->>'issue_id'=issue_id::text
    AND (document->>'version')::bigint=version AND document->'plan'->'scope'->>'kind'=kind)
);
CREATE INDEX issue_content_retention_due ON issue_content_retention(expires_at_millis,issue_id) WHERE NOT completed;
