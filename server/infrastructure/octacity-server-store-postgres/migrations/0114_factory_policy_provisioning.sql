-- Authorized lifecycle provenance, never created by GET. Policy definitions remain
-- in the existing policy journal; this record only prevents reinitialization after edits.
CREATE TABLE factory_project_policy_provisioning (
  project_id UUID PRIMARY KEY REFERENCES projects(id),
  document JSONB NOT NULL CHECK (jsonb_typeof(document) = 'object' AND octet_length(document::text) <= 8192)
);

-- The key covers the complete typed legacy reference, including version and digest.
CREATE TABLE factory_project_policy_mappings (
  project_id UUID NOT NULL REFERENCES projects(id),
  legacy_key BYTEA NOT NULL CHECK (octet_length(legacy_key) = 32),
  document JSONB NOT NULL CHECK (jsonb_typeof(document) = 'object' AND octet_length(document::text) <= 8192),
  PRIMARY KEY (project_id, legacy_key)
);
