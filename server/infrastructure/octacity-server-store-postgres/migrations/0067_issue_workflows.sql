-- Business vocabulary is independent of Factory execution and provider resources.
CREATE TABLE issue_workflows (
  project_id UUID NOT NULL REFERENCES projects(id),
  id UUID NOT NULL,
  current_version BIGINT NOT NULL CHECK (current_version > 0),
  PRIMARY KEY (project_id, id)
);
CREATE TABLE issue_workflow_versions (
  project_id UUID NOT NULL,
  workflow_id UUID NOT NULL,
  version BIGINT NOT NULL CHECK (version > 0),
  definition JSONB NOT NULL CHECK (jsonb_typeof(definition) = 'object'),
  published_at_millis BIGINT NOT NULL CHECK (published_at_millis >= 0),
  PRIMARY KEY (project_id, workflow_id, version),
  FOREIGN KEY (project_id, workflow_id) REFERENCES issue_workflows(project_id, id)
);
ALTER TABLE issue_workflows ADD CONSTRAINT issue_workflows_current_fk
  FOREIGN KEY (project_id, id, current_version)
  REFERENCES issue_workflow_versions(project_id, workflow_id, version)
  DEFERRABLE INITIALLY DEFERRED;
-- A default pins an exact version; publication never advances it implicitly.
CREATE TABLE issue_workflow_defaults (
  project_id UUID PRIMARY KEY REFERENCES projects(id),
  selection_version BIGINT NOT NULL CHECK (selection_version > 0),
  workflow_id UUID NOT NULL,
  workflow_version BIGINT NOT NULL CHECK (workflow_version > 0),
  FOREIGN KEY (project_id, workflow_id, workflow_version)
    REFERENCES issue_workflow_versions(project_id, workflow_id, version)
);
