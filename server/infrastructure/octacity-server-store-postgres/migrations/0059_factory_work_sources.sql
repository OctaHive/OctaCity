-- Source state is separate from the existing authoritative Work admission identity.
CREATE TABLE factory_source_inputs (
  id UUID NOT NULL,
  version BIGINT NOT NULL CHECK (version > 0),
  project_id UUID NOT NULL REFERENCES projects(id),
  configuration_id UUID NOT NULL,
  configuration_version BIGINT NOT NULL,
  repository_id UUID NOT NULL,
  repository_version BIGINT NOT NULL,
  document JSONB NOT NULL CHECK (jsonb_typeof(document) = 'object' AND octet_length(document::text) BETWEEN 2 AND 1048576),
  PRIMARY KEY (id, version),
  FOREIGN KEY (configuration_id, configuration_version) REFERENCES factory_configuration_versions(factory_configuration_id, version),
  FOREIGN KEY (repository_id, repository_version) REFERENCES repository_versions(repository_id, version)
);
CREATE INDEX factory_source_inputs_project ON factory_source_inputs(project_id, id, version DESC);
CREATE TABLE factory_source_groups (
  id UUID NOT NULL,
  version BIGINT NOT NULL CHECK (version > 0),
  project_id UUID NOT NULL REFERENCES projects(id),
  document JSONB NOT NULL CHECK (jsonb_typeof(document) = 'object' AND octet_length(document::text) BETWEEN 2 AND 4194304),
  PRIMARY KEY (id, version)
);
CREATE INDEX factory_source_groups_project ON factory_source_groups(project_id, id, version DESC);
CREATE TABLE factory_source_scans (
  kind TEXT NOT NULL CHECK (kind IN ('input','group')),
  id UUID NOT NULL,
  project_id UUID NOT NULL REFERENCES projects(id),
  document JSONB NOT NULL CHECK (jsonb_typeof(document) = 'object' AND octet_length(document::text) BETWEEN 2 AND 32768),
  PRIMARY KEY (kind, id)
);
-- Private frozen snapshots are never discovery/diagnostic payloads.
CREATE TABLE factory_source_preparations (
  id BYTEA PRIMARY KEY CHECK (octet_length(id)=32),
  project_id UUID NOT NULL REFERENCES projects(id),
  issue BYTEA NOT NULL CHECK (octet_length(issue)=32),
  input_id UUID NOT NULL,
  input_version BIGINT NOT NULL,
  scan_kind TEXT NOT NULL CHECK (scan_kind IN ('input','group')),
  scan_id UUID NOT NULL,
  settled BOOLEAN NOT NULL DEFAULT FALSE,
  work_id UUID REFERENCES factory_work_envelopes(id),
  run_id UUID REFERENCES factory_runs(id),
  CHECK ((NOT settled AND work_id IS NULL AND run_id IS NULL) OR (settled AND work_id IS NOT NULL AND run_id IS NOT NULL)),
  document BYTEA NOT NULL CHECK (octet_length(document) BETWEEN 2 AND 8388608),
  FOREIGN KEY (input_id,input_version) REFERENCES factory_source_inputs(id,version)
);
CREATE INDEX factory_source_preparations_pending ON factory_source_preparations(project_id,scan_kind,scan_id,id) WHERE NOT settled;
CREATE INDEX factory_source_preparations_issue ON factory_source_preparations(project_id,issue,input_id) WHERE settled;
