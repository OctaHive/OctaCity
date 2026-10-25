-- Additive Project settings only; installed inventory and existing Role/Profile pins are unchanged.
-- A Project row lock serializes publication, so no duplicate mutable head is required.
CREATE TABLE factory_runtime_settings (
  project_id UUID NOT NULL REFERENCES projects(id),
  runtime TEXT NOT NULL CHECK (octet_length(runtime) BETWEEN 1 AND 128),
  version BIGINT NOT NULL CHECK (version > 0),
  settings JSONB NOT NULL CHECK (jsonb_typeof(settings) = 'object' AND octet_length(settings::text) <= 1048576),
  PRIMARY KEY (project_id, runtime, version)
);
