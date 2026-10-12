-- Keep the summary aligned with typed candidate-bearing owner observations.
-- Observation families never choose execution transitions or logical node routes.
-- Bounded child pages follow immutable identities within one Run. These scans
-- must not walk another Run's archive or sort a complete retained history.
CREATE INDEX factory_flow_runs_diagnostics_idx ON factory_flow_runs (factory_run_id, id);
CREATE INDEX factory_workflow_cycles_diagnostics_idx ON factory_workflow_cycles (factory_run_id, id);
CREATE INDEX factory_flow_build_executions_diagnostics_idx ON factory_flow_build_executions (run_id, id);
CREATE INDEX factory_run_budgets_diagnostics_idx ON factory_run_budgets (run_id, id);
CREATE INDEX factory_node_stops_diagnostics_idx ON factory_node_stops (run_id, id);
CREATE INDEX factory_flow_records_diagnostics_idx ON factory_flow_records (run_id, (discovery_facts ->> 'kind'), id);

CREATE OR REPLACE VIEW factory_run_discovery AS
SELECT run.id, decision.facts ->> 'outcome' AS decision_outcome,
  delivery.facts ->> 'state' AS delivery_state,
  jsonb_build_object(
    'base_revision', work.envelope #> '{subject,base_revision}',
    'candidate', candidate.subject,
    'current_nodes', COALESCE(active.items, '[]'::jsonb),
    'active_node_count', active_count.total,
    'budget', jsonb_build_object('usage', budget.usage, 'limit', run.flow_admission_limits -> 'budget'),
    'latest_decision', CASE WHEN decision.facts IS NULL THEN NULL ELSE jsonb_build_object(
      'id', decision.facts -> 'id', 'subject', decision.facts -> 'subject', 'outcome', decision.facts -> 'outcome') END,
    'delivery', CASE WHEN delivery.facts IS NULL THEN NULL ELSE jsonb_build_object(
      'id', delivery.facts -> 'id', 'decision_id', delivery.facts -> 'decision_id',
      'subject', delivery.facts -> 'subject', 'state', delivery.facts -> 'state') END,
    'escalation', escalation.facts -> 'id'
  ) AS diagnostics
FROM factory_runs run
JOIN factory_work_envelopes work ON work.id = run.work_envelope_id
JOIN factory_run_current current ON current.run_id = run.id
JOIN factory_run_budgets budget ON budget.id = current.budget_id
LEFT JOIN LATERAL (
  SELECT subject FROM (
    SELECT record.discovery_facts -> 'subject' AS subject, record.observed_at_millis AS at, record.node_attempt_id AS id, CASE WHEN record.discovery_facts ->> 'kind' IN ('candidate','changeset') THEN 0 ELSE 2 END AS priority
    FROM factory_flow_records record WHERE record.run_id = run.id AND record.discovery_facts ->> 'kind' IN ('candidate','changeset','evidence','review_plan','assessment','decision','delivery','promotion')
    UNION ALL
    SELECT convert_from(input.document, 'UTF8')::jsonb #> '{context,subject,value}' AS subject,
      FLOOR(EXTRACT(EPOCH FROM attempt.created_at) * 1000)::bigint AS at, attempt.id, 1 AS priority
    FROM factory_node_attempts attempt JOIN factory_flow_inputs input ON input.run_id = attempt.run_id AND input.id = attempt.input_digest
    WHERE attempt.run_id = run.id AND convert_from(input.document, 'UTF8')::jsonb #>> '{context,subject,kind}' = 'candidate'
  ) observations ORDER BY priority ASC, at DESC, id DESC LIMIT 1
) candidate ON TRUE
LEFT JOIN LATERAL (
  SELECT record.discovery_facts AS facts FROM factory_flow_records record
  WHERE record.run_id = run.id AND record.discovery_facts ->> 'kind' = 'decision'
    AND record.discovery_facts -> 'subject' = candidate.subject
  ORDER BY record.observed_at_millis DESC, record.node_attempt_id DESC LIMIT 1
) decision ON TRUE
LEFT JOIN LATERAL (
  SELECT record.discovery_facts AS facts FROM factory_flow_records record
  WHERE record.run_id = run.id AND record.discovery_facts ->> 'kind' = 'delivery'
    AND record.discovery_facts -> 'subject' = candidate.subject
  ORDER BY record.observed_at_millis DESC, record.node_attempt_id DESC LIMIT 1
) delivery ON TRUE
LEFT JOIN LATERAL (
  SELECT record.discovery_facts AS facts FROM factory_flow_records record
  WHERE record.run_id = run.id AND record.discovery_facts ->> 'kind' = 'escalation'
  ORDER BY record.observed_at_millis DESC, record.node_attempt_id DESC LIMIT 1
) escalation ON TRUE
LEFT JOIN LATERAL (
  SELECT count(*) AS total FROM factory_node_attempts node WHERE node.run_id = run.id
    AND NOT EXISTS (SELECT 1 FROM factory_node_attempt_completions result WHERE result.node_attempt_id = node.id)
    AND NOT EXISTS (SELECT 1 FROM factory_node_stops stop WHERE stop.node_attempt_id = node.id)
) active_count ON TRUE
LEFT JOIN LATERAL (
  SELECT jsonb_agg(item ORDER BY flow_run_id, workflow_cycle_id, node_key, attempt_number) AS items FROM (
    SELECT node.flow_run_id, node.workflow_cycle_id, node.node_key, node.attempt_number,
      jsonb_build_object('id', node.id, 'flow_run_id', node.flow_run_id, 'cycle_id', node.workflow_cycle_id,
        'node', node.node_key, 'attempt', node.attempt_number) AS item
    FROM factory_node_attempts node WHERE node.run_id = run.id
      AND NOT EXISTS (SELECT 1 FROM factory_node_attempt_completions result WHERE result.node_attempt_id = node.id)
      AND NOT EXISTS (SELECT 1 FROM factory_node_stops stop WHERE stop.node_attempt_id = node.id)
    ORDER BY node.flow_run_id, node.workflow_cycle_id, node.node_key, node.attempt_number LIMIT 32
  ) bounded
) active ON TRUE;
