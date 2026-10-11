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
-- Technical cancellation is distinct from a configured outcome and its completion.
-- A receipt retains measured consumption after every owned operation has stopped.
CREATE TABLE factory_node_stops (
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  node_attempt_id UUID PRIMARY KEY,
  id BYTEA NOT NULL UNIQUE CHECK (octet_length(id) = 32),
  execution_id BYTEA REFERENCES factory_flow_build_executions(id),
  document BYTEA NOT NULL CHECK (octet_length(document) BETWEEN 2 AND 16384),
  FOREIGN KEY (node_attempt_id, run_id) REFERENCES factory_node_attempts(id, run_id)
);
CREATE INDEX factory_node_stops_run_idx ON factory_node_stops (run_id, node_attempt_id);

-- A common node operation can initiate ordinary execution without a Trigger.
-- The immutable origin remains readable after Factory metadata retention.
ALTER TABLE builds
  ALTER COLUMN trigger_occurrence_id DROP NOT NULL,
  ADD COLUMN factory_origin JSONB,
  ADD COLUMN factory_materialization_digest BYTEA,
  ADD COLUMN factory_permission_digest BYTEA,
  ADD CONSTRAINT builds_factory_acceptance_bound CHECK (
    (factory_origin IS NULL AND factory_materialization_digest IS NULL AND factory_permission_digest IS NULL)
    OR (factory_origin IS NOT NULL AND factory_materialization_digest IS NOT NULL
        AND factory_permission_digest IS NOT NULL AND octet_length(factory_materialization_digest) = 32
        AND octet_length(factory_permission_digest) = 32)
  ),
  ADD CONSTRAINT builds_exactly_one_origin CHECK (
    (trigger_occurrence_id IS NOT NULL) <> (factory_origin IS NOT NULL)
  ),
  ADD CONSTRAINT builds_factory_origin_bounded CHECK (
    factory_origin IS NULL OR (
      jsonb_typeof(factory_origin) = 'object'
      AND pg_column_size(factory_origin) <= 1024
      AND factory_origin ?& ARRAY['operation_id', 'run_id', 'node_attempt_id', 'input_digest']
      AND factory_origin - ARRAY['operation_id', 'run_id', 'node_attempt_id', 'input_digest'] = '{}'::jsonb
      AND jsonb_typeof(factory_origin->'operation_id') = 'string'
      AND jsonb_typeof(factory_origin->'run_id') = 'string'
      AND jsonb_typeof(factory_origin->'node_attempt_id') = 'string'
      AND jsonb_typeof(factory_origin->'input_digest') = 'string'
      AND factory_origin->>'operation_id' ~ '^[0-9a-f]{64}$'
      AND factory_origin->>'input_digest' ~ '^[0-9a-f]{64}$'
      AND factory_origin->>'run_id' ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
      AND factory_origin->>'node_attempt_id' ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
    )
  );

-- Retain ownership even when creation succeeded before its response was journaled.
CREATE UNIQUE INDEX builds_factory_operation_idx ON builds ((factory_origin->>'operation_id'))
  WHERE factory_origin IS NOT NULL;
CREATE VIEW factory_run_builds AS
  SELECT DISTINCT run_id, build_id FROM factory_flow_build_executions
  UNION
  SELECT (factory_origin->>'run_id')::uuid AS run_id, id AS build_id
  FROM builds WHERE factory_origin IS NOT NULL;
