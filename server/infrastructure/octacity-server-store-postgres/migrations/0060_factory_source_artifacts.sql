-- Private source inputs are not Agent-produced Build outputs.
CREATE TABLE factory_source_materializations (
  preparation_id BYTEA PRIMARY KEY REFERENCES factory_source_preparations(id),
  project_id UUID NOT NULL REFERENCES projects(id),
  document JSONB NOT NULL CHECK (jsonb_typeof(document)='object' AND octet_length(document::text) BETWEEN 2 AND 65536)
);
CREATE TABLE factory_source_input_objects (
  artifact_id UUID PRIMARY KEY,
  upload_id UUID NOT NULL UNIQUE,
  preparation_id BYTEA NOT NULL REFERENCES factory_source_materializations(preparation_id)
);
CREATE INDEX factory_source_input_objects_preparation ON factory_source_input_objects(preparation_id);
ALTER TABLE factory_source_preparations
  ADD COLUMN snapshot_expires_at_millis BIGINT NOT NULL DEFAULT 2592000000 CHECK (snapshot_expires_at_millis>=0),
  ADD COLUMN snapshot_cleaned BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE factory_source_materializations ADD COLUMN expires_at_millis BIGINT NOT NULL CHECK (expires_at_millis>=0);
CREATE INDEX factory_source_snapshots_expiry ON factory_source_preparations(snapshot_expires_at_millis,id);
CREATE INDEX factory_source_materializations_expiry ON factory_source_materializations(expires_at_millis,preparation_id);
