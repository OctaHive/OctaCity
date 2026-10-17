-- Explicit Project-use authority; publishing connection metadata creates no grants.
CREATE TABLE integration_project_access (
  connection_id UUID NOT NULL REFERENCES integration_connections(id),
  project_id UUID NOT NULL REFERENCES projects(id),
  current_version BIGINT NOT NULL CHECK (current_version > 0),
  PRIMARY KEY (connection_id, project_id)
);
CREATE TABLE integration_project_access_versions (
  connection_id UUID NOT NULL,
  project_id UUID NOT NULL,
  version BIGINT NOT NULL CHECK (version > 0),
  connection_version BIGINT NOT NULL CHECK (connection_version > 0),
  document JSONB NOT NULL CHECK (jsonb_typeof(document) = 'object'),
  PRIMARY KEY (connection_id, project_id, version),
  FOREIGN KEY (connection_id, project_id) REFERENCES integration_project_access(connection_id, project_id),
  FOREIGN KEY (connection_id, connection_version) REFERENCES integration_connection_versions(connection_id, version)
);
ALTER TABLE integration_project_access ADD CONSTRAINT integration_project_access_current_fk
  FOREIGN KEY (connection_id, project_id, current_version)
  REFERENCES integration_project_access_versions(connection_id, project_id, version)
  DEFERRABLE INITIALLY DEFERRED;
