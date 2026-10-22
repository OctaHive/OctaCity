-- Private metadata extends retained inputs; object bytes keep the existing backend.
CREATE TABLE IF NOT EXISTS project_skill_imports (
  id bytea PRIMARY KEY CHECK (octet_length(id)=32),
  project_id uuid NOT NULL REFERENCES projects(id),
  security_scope text NOT NULL,
  replay_key text NOT NULL,
  skill_identity text NOT NULL,
  skill_version text NOT NULL,
  document jsonb NOT NULL,
  published_at_millis bigint,
  cleanup_started_at_millis bigint,
  CHECK (cleanup_started_at_millis IS NULL OR published_at_millis IS NULL),
  staging_until_millis bigint NOT NULL,
  UNIQUE (project_id,security_scope,replay_key),
  UNIQUE (project_id,skill_identity,skill_version)
);
CREATE TABLE IF NOT EXISTS project_skill_input_objects (
  artifact_id uuid PRIMARY KEY,
  import_id bytea NOT NULL REFERENCES project_skill_imports(id),
  recheck_at_millis bigint
);
CREATE INDEX IF NOT EXISTS project_skill_import_staging_cleanup
  ON project_skill_imports(staging_until_millis,id) WHERE published_at_millis IS NULL;
CREATE INDEX IF NOT EXISTS project_skill_input_import ON project_skill_input_objects(import_id);
