-- Reviewed import activation is independent from task pull, execution inputs and Source progress.
CREATE TABLE issue_source_imports (
  project_id UUID NOT NULL REFERENCES projects(id),
  security_scope TEXT NOT NULL,
  id UUID NOT NULL,
  version BIGINT NOT NULL CHECK (version > 0),
  connection_id UUID NOT NULL,
  connection_version BIGINT NOT NULL CHECK (connection_version > 0),
  document JSONB NOT NULL CHECK (jsonb_typeof(document) = 'object'),
  PRIMARY KEY (project_id, security_scope, id, version),
  FOREIGN KEY (connection_id, connection_version)
    REFERENCES integration_connection_versions(connection_id, version)
);
CREATE INDEX issue_source_import_heads ON issue_source_imports (project_id, security_scope, id, version DESC);
