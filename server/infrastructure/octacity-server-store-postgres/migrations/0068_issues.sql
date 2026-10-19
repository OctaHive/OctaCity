-- Persistent organizational tasks, independent of execution and provider state.
CREATE TABLE issue_numbers (
  project_id UUID PRIMARY KEY REFERENCES projects(id),
  last_number BIGINT NOT NULL CHECK (last_number > 0)
);
CREATE TABLE issues (
  project_id UUID NOT NULL REFERENCES projects(id),
  id UUID PRIMARY KEY,
  number BIGINT NOT NULL CHECK (number > 0),
  current_version BIGINT NOT NULL CHECK (current_version > 0),
  field_policy JSONB CHECK (field_policy IS NULL OR (jsonb_typeof(field_policy) = 'object' AND octet_length(field_policy::text) <= 16384)),
  UNIQUE (project_id, id),
  UNIQUE (project_id, number)
);
CREATE TABLE issue_revisions (
  project_id UUID NOT NULL,
  issue_id UUID NOT NULL,
  version BIGINT NOT NULL CHECK (version > 0),
  workflow_id UUID NOT NULL,
  workflow_version BIGINT NOT NULL CHECK (workflow_version > 0),
  document JSONB NOT NULL CHECK (jsonb_typeof(document) = 'object' AND octet_length(document::text) <= 1048576),
  PRIMARY KEY (project_id, issue_id, version),
  FOREIGN KEY (project_id, issue_id) REFERENCES issues(project_id, id),
  FOREIGN KEY (project_id, workflow_id, workflow_version)
    REFERENCES issue_workflow_versions(project_id, workflow_id, version),
  CHECK (document->>'id' = issue_id::text AND document->>'project_id' = project_id::text
    AND (document->>'version')::bigint = version
    AND document->>'workflow_id' = workflow_id::text
    AND (document->>'workflow_version')::bigint = workflow_version)
);
ALTER TABLE issues ADD CONSTRAINT issues_current_revision_fk
  FOREIGN KEY (project_id, id, current_version) REFERENCES issue_revisions(project_id, issue_id, version)
  DEFERRABLE INITIALLY DEFERRED;
CREATE TABLE issue_status_history (
  project_id UUID NOT NULL,
  issue_id UUID NOT NULL,
  version BIGINT NOT NULL,
  document JSONB NOT NULL CHECK (jsonb_typeof(document) = 'object' AND octet_length(document::text) <= 16384),
  PRIMARY KEY (project_id, issue_id, version),
  FOREIGN KEY (project_id, issue_id, version) REFERENCES issue_revisions(project_id, issue_id, version)
);
