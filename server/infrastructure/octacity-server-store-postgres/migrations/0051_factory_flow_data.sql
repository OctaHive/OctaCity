-- Schemas 47-51 are the same unreleased Factory slice; preceding binary schema is 46.

-- Common, byte-bounded data replaces phase-specific journals. Original bytes
-- preserve the complete identities checked by the Rust restore contracts.
CREATE TABLE factory_flow_incoming (
  run_id UUID PRIMARY KEY REFERENCES factory_runs(id),
  id BYTEA NOT NULL CHECK (octet_length(id) = 32),
  schema_reference JSONB NOT NULL CHECK (jsonb_typeof(schema_reference) = 'object' AND octet_length(schema_reference::text) BETWEEN 2 AND 1024),
  document BYTEA NOT NULL CHECK (octet_length(document) BETWEEN 2 AND 1048576)
);
CREATE TABLE factory_flow_inputs (
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  id BYTEA NOT NULL CHECK (octet_length(id) = 32),
  flow_run_id UUID NOT NULL,
  cycle_id UUID NOT NULL,
  generation BIGINT NOT NULL CHECK (generation BETWEEN 0 AND 4294967295),
  schema_reference JSONB NOT NULL CHECK (jsonb_typeof(schema_reference) = 'object' AND octet_length(schema_reference::text) BETWEEN 2 AND 1024),
  document BYTEA NOT NULL CHECK (octet_length(document) BETWEEN 2 AND 1048576),
  PRIMARY KEY (run_id, id),
  FOREIGN KEY (flow_run_id, run_id) REFERENCES factory_flow_runs(id, factory_run_id),
  FOREIGN KEY (cycle_id, run_id) REFERENCES factory_workflow_cycles(id, factory_run_id)
);
CREATE TABLE factory_flow_records (
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  node_attempt_id UUID PRIMARY KEY,
  id BYTEA NOT NULL CHECK (octet_length(id) = 32),
  input_digest BYTEA NOT NULL CHECK (octet_length(input_digest) = 32),
  schema_reference JSONB NOT NULL CHECK (jsonb_typeof(schema_reference) = 'object' AND octet_length(schema_reference::text) BETWEEN 2 AND 1024),
  document BYTEA NOT NULL CHECK (octet_length(document) BETWEEN 2 AND 1048576),
  FOREIGN KEY (node_attempt_id, run_id) REFERENCES factory_node_attempts(id, run_id),
  FOREIGN KEY (run_id, input_digest) REFERENCES factory_flow_inputs(run_id, id)
);
CREATE INDEX factory_flow_records_run_idx ON factory_flow_records (run_id, node_attempt_id);
-- A stable request exists before dispatch; restart never reconstructs it from current settings.
CREATE TABLE factory_flow_build_intents (
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  node_attempt_id UUID PRIMARY KEY,
  operation_id BYTEA NOT NULL CHECK (octet_length(operation_id) = 32),
  input_digest BYTEA NOT NULL CHECK (octet_length(input_digest) = 32),
  document BYTEA NOT NULL CHECK (octet_length(document) BETWEEN 2 AND 1048576),
  FOREIGN KEY (node_attempt_id, run_id) REFERENCES factory_node_attempts(id, run_id),
  FOREIGN KEY (run_id, input_digest) REFERENCES factory_flow_inputs(run_id, id),
  UNIQUE (run_id, operation_id)
);
CREATE TABLE factory_flow_build_executions (
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  id BYTEA PRIMARY KEY CHECK (octet_length(id)=32),
  node_attempt_id UUID NOT NULL,
  operation_id BYTEA NOT NULL CHECK (octet_length(operation_id)=32),
  build_id UUID NOT NULL REFERENCES builds(id),
  attempt_id UUID NOT NULL REFERENCES attempts(id),
  document BYTEA NOT NULL CHECK (octet_length(document) BETWEEN 2 AND 1048576),
  FOREIGN KEY (node_attempt_id,run_id) REFERENCES factory_node_attempts(id,run_id),
  FOREIGN KEY (run_id,operation_id) REFERENCES factory_flow_build_intents(run_id,operation_id),
  UNIQUE (node_attempt_id,attempt_id)
);
-- Retention sees both ordinary compatibility projections and configured nodes.
CREATE VIEW factory_run_builds AS
  SELECT run_id,build_id FROM factory_build_links
  UNION SELECT run_id,build_id FROM factory_flow_build_executions;
