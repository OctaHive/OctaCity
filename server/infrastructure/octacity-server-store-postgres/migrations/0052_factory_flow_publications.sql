-- Project-owned management identities complement the exact closure retained by Configurations.
CREATE TABLE factory_flow_definitions (
  id UUID PRIMARY KEY,
  project_id UUID NOT NULL REFERENCES projects(id),
  current_version BIGINT NOT NULL CHECK (current_version > 0),
  created_at TIMESTAMPTZ NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX factory_flow_definitions_project_order_idx ON factory_flow_definitions (project_id, created_at DESC, id DESC);
CREATE TABLE factory_flow_publications (
  id UUID NOT NULL REFERENCES factory_flow_definitions(id),
  version BIGINT NOT NULL CHECK (version > 0),
  publication JSONB NOT NULL CHECK (jsonb_typeof(publication) = 'object' AND pg_column_size(publication) <= 16777216),
  PRIMARY KEY (id, version),
  FOREIGN KEY (id, version) REFERENCES factory_flow_definition_versions(id, version)
);
