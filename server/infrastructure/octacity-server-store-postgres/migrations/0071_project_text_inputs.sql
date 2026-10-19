-- Shared by every Project text ingress, with no Issue/Source/Build prerequisite.
CREATE TABLE project_text_preparations (
  id BYTEA PRIMARY KEY CHECK (octet_length(id) = 32),
  project_id UUID NOT NULL REFERENCES projects(id),
  security_scope TEXT NOT NULL CHECK (octet_length(security_scope) BETWEEN 1 AND 128),
  replay_key TEXT NOT NULL CHECK (octet_length(replay_key) BETWEEN 1 AND 128),
  document JSONB NOT NULL CHECK (jsonb_typeof(document) = 'object' AND octet_length(document::text) <= 8388608),
  UNIQUE (project_id, security_scope, replay_key),
  UNIQUE (project_id, id)
);
CREATE TABLE project_text_input_objects (
  artifact_id UUID PRIMARY KEY,
  preparation_id BYTEA NOT NULL REFERENCES project_text_preparations(id),
  upload_id UUID NOT NULL UNIQUE,
  role TEXT NOT NULL CHECK (role IN ('task','acceptance')),
  UNIQUE (preparation_id, role)
);
