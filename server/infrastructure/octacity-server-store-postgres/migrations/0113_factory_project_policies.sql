-- Additive, immutable Project policy history. Existing Configuration/Run bytes are untouched.
-- Core publications are capped at 256 KiB; the established 1 MiB JSONB envelope
-- accommodates the typed wrapper and PostgreSQL formatting.
CREATE TABLE factory_project_policies (
  project_id UUID NOT NULL REFERENCES projects(id),
  kind TEXT NOT NULL CHECK (kind IN ('launch', 'access')),
  id UUID NOT NULL CHECK (id <> '00000000-0000-0000-0000-000000000000'),
  version BIGINT NOT NULL CHECK (version > 0),
  document JSONB NOT NULL CHECK (jsonb_typeof(document) = 'object' AND octet_length(document::text) <= 1048576),
  PRIMARY KEY (project_id, kind, id, version)
);

CREATE INDEX factory_project_policy_catalog ON factory_project_policies (project_id, kind, id, version DESC);

-- Selections are a separate append-only history; publishing content never repins them.
CREATE TABLE factory_project_policy_defaults (
  project_id UUID NOT NULL REFERENCES projects(id),
  version BIGINT NOT NULL CHECK (version > 0),
  document JSONB NOT NULL CHECK (jsonb_typeof(document) = 'object' AND octet_length(document::text) <= 8192),
  PRIMARY KEY (project_id, version)
);
