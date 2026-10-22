-- Additive catalog only: existing static Flows and admitted Runs are untouched.
-- The existing 1 MiB Factory JSON envelope bound leaves room for references
-- and JSONB formatting; the core separately enforces 256 KiB definition content.
CREATE TABLE factory_roles (
  id UUID PRIMARY KEY,
  project_id UUID NOT NULL REFERENCES projects(id),
  current_version BIGINT NOT NULL CHECK (current_version > 0),
  UNIQUE (id, project_id)
);
CREATE TABLE factory_role_versions (
  id UUID NOT NULL,
  project_id UUID NOT NULL,
  version BIGINT NOT NULL CHECK (version > 0),
  definition JSONB NOT NULL CHECK (jsonb_typeof(definition) = 'object' AND octet_length(definition::text) <= 1048576),
  PRIMARY KEY (id, version),
  UNIQUE (id, project_id, version),
  FOREIGN KEY (id, project_id) REFERENCES factory_roles(id, project_id)
);
CREATE TABLE factory_role_profiles (
  id UUID PRIMARY KEY,
  project_id UUID NOT NULL REFERENCES projects(id),
  current_version BIGINT NOT NULL CHECK (current_version > 0),
  UNIQUE (id, project_id)
);
CREATE TABLE factory_role_profile_versions (
  id UUID NOT NULL,
  project_id UUID NOT NULL,
  version BIGINT NOT NULL CHECK (version > 0),
  role_id UUID NOT NULL,
  role_version BIGINT NOT NULL,
  definition JSONB NOT NULL CHECK (jsonb_typeof(definition) = 'object' AND octet_length(definition::text) <= 1048576),
  PRIMARY KEY (id, version),
  FOREIGN KEY (id, project_id) REFERENCES factory_role_profiles(id, project_id),
  FOREIGN KEY (role_id, project_id, role_version) REFERENCES factory_role_versions(id, project_id, version)
);
