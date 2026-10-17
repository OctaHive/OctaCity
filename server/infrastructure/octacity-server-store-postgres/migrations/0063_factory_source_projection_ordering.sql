-- Ordering is assigned with accepted status publication, not worker arrival or
-- wall-clock timestamps. No Work identity or Flow authority is duplicated here.
CREATE TABLE factory_source_projection_journals (
  project_id UUID NOT NULL REFERENCES projects(id),
  issue BYTEA NOT NULL CHECK (octet_length(issue) = 32),
  next_sequence BIGINT NOT NULL DEFAULT 0 CHECK (next_sequence >= 0),
  document JSONB NOT NULL CHECK (
    jsonb_typeof(document) = 'object'
    AND octet_length(document::text) BETWEEN 2 AND 4194304
  ),
  PRIMARY KEY (project_id, issue)
);
CREATE TABLE factory_source_projection_intents (
  operation_id BYTEA PRIMARY KEY CHECK (octet_length(operation_id) = 32),
  project_id UUID NOT NULL,
  issue BYTEA NOT NULL,
  sequence BIGINT NOT NULL CHECK (sequence > 0),
  managed_fields JSONB CHECK (managed_fields IS NULL OR (
    jsonb_typeof(managed_fields) = 'array'
    AND jsonb_array_length(managed_fields) <= 512
    AND octet_length(managed_fields::text) <= 65536
  )),
  UNIQUE (project_id, issue, sequence),
  FOREIGN KEY (project_id, issue) REFERENCES factory_source_projection_journals(project_id, issue)
);
CREATE INDEX factory_source_projection_unknown ON factory_source_projection_intents(project_id, issue, sequence) WHERE managed_fields IS NULL;

-- Existing history has only retained timestamps/digests, so seed a deterministic
-- legacy order without rewriting it. Every subsequent acceptance uses the locked
-- per-item counter. Unknown legacy scopes remain fail-closed until inspected.
WITH prepared AS (
  SELECT p.project_id,p.run_id,
    convert_from(p.document,'UTF8')::jsonb AS document
  FROM factory_source_preparations p WHERE p.settled
), scoped AS (
  SELECT project_id,run_id,document,
    -- Same length framing as WorkSourcePreparation::projection_item. Admission
    -- correlation stays installation-scoped; only the technical bucket changes.
    sha256(int8send(octet_length('octacity.factory.projection-item.v1')::bigint)
      || convert_to('octacity.factory.projection-item.v1','UTF8')
      || int8send(octet_length(document #>> '{item,resource}')::bigint)
      || convert_to(document #>> '{item,resource}','UTF8')
      || int8send(octet_length(document #>> '{item,item}')::bigint)
      || convert_to(document #>> '{item,item}','UTF8')) AS issue
  FROM prepared
), accepted AS (
  SELECT DISTINCT ON (o.operation_id) p.project_id,p.issue,o.operation_id,o.recorded_at
  FROM scoped p JOIN factory_outbox_records o ON o.run_id=p.run_id
  JOIN factory_flow_records r ON r.run_id=o.run_id AND r.id=o.input_digest
  JOIN factory_node_attempts a ON a.id=r.node_attempt_id AND a.run_id=o.run_id
  JOIN factory_flow_runs f ON f.id=a.flow_run_id AND f.factory_run_id=o.run_id
  JOIN factory_flow_definition_versions d ON d.id=f.flow_definition_id AND d.version=f.flow_definition_version
  CROSS JOIN LATERAL jsonb_array_elements(d.definition->'nodes') AS n(node)
  WHERE o.kind='work.status.projection' AND o.attempt=0 AND o.state='pending'
    AND n.node->>'key'=a.node_key AND n.node->>'kind'='trusted_action'
    AND n.node #>> '{action,action}'='set_status'
    -- Same eligibility as live publication: unrelated status adapters must not
    -- create unknown metadata that this Source's Reporter cannot ever register.
    AND n.node #> '{action,plugin}'=p.document #> '{input,definition,reporter,capability}'
    AND (p.document #> '{input,definition,reporter}') IS NOT NULL
    AND (p.document #> '{input,definition,reporter}') <> 'null'::jsonb
  ORDER BY o.operation_id,o.recorded_at,p.issue
), numbered AS (
  SELECT *,row_number() OVER (PARTITION BY project_id,issue ORDER BY recorded_at,operation_id) AS ordinal FROM accepted
), journals AS (
  INSERT INTO factory_source_projection_journals(project_id,issue,document)
  SELECT DISTINCT project_id,issue,'{"heads":{},"leases":{}}'::jsonb FROM numbered
  RETURNING project_id,issue
)
INSERT INTO factory_source_projection_intents(operation_id,project_id,issue,sequence)
SELECT n.operation_id,n.project_id,n.issue,n.ordinal FROM numbered n JOIN journals j USING(project_id,issue);
UPDATE factory_source_projection_journals j SET next_sequence=(
  SELECT max(i.sequence) FROM factory_source_projection_intents i WHERE i.project_id=j.project_id AND i.issue=j.issue
);
