-- Page metadata in rank order. The application authorizes each Run and resolves canonical slots.
SELECT
  a.work_id, run.id AS run_id, (EXTRACT(EPOCH FROM run.admitted_at) * 1000)::bigint AS admitted_at,
  attempt.id AS node_attempt_id, (EXTRACT(EPOCH FROM completion.completed_at) * 1000)::bigint AS completed_at,
  definition.id AS flow_id, attempt.node_key, completion.outcome,
  declared.value->'issue_presentation' AS presentation
FROM issues AS issue
JOIN issue_work_associations AS a ON a.project_id = issue.project_id AND a.issue_id = issue.id
JOIN issue_execution_requests AS request
  ON request.project_id = a.project_id AND request.issue_id = a.issue_id AND request.id = a.request_id
JOIN factory_runs AS run
  ON run.project_id = a.project_id AND run.id = a.run_id AND run.work_envelope_id = a.work_id
JOIN factory_flow_records AS record ON record.run_id = run.id
JOIN factory_node_attempt_completions AS completion
  ON completion.run_id = run.id AND completion.node_attempt_id = record.node_attempt_id
  AND completion.output_digest = record.id
  AND completion.completion->'output_schema' = record.schema_reference
JOIN factory_node_attempts AS attempt
  ON attempt.run_id = run.id AND attempt.id = record.node_attempt_id AND attempt.input_digest = record.input_digest
JOIN factory_flow_runs AS flow ON flow.factory_run_id = run.id AND flow.id = attempt.flow_run_id
JOIN factory_configuration_flow_definitions AS pin
  ON pin.factory_configuration_id = run.factory_configuration_id
  AND pin.factory_configuration_version = run.factory_configuration_version
  AND pin.flow_definition_id = flow.flow_definition_id AND pin.flow_definition_version = flow.flow_definition_version
JOIN factory_flow_definition_versions AS definition
  ON definition.id = pin.flow_definition_id AND definition.version = pin.flow_definition_version
CROSS JOIN LATERAL jsonb_array_elements(definition.definition->'nodes') AS node
CROSS JOIN LATERAL jsonb_array_elements(node.value->'outcomes') AS declared
WHERE issue.project_id = $1 AND issue.id = $2 AND issue.current_version = $3
  AND request.requirements_version = (a.document->>'requirements_version')::bigint
  AND request.document->>'title_digest' = $4
  AND request.document->>'description_digest' = $5
  AND request.document->>'acceptance_digest' = $6
  AND run.visible AND ($7::uuid[] IS NULL OR run.id = ANY($7))
  AND node.value->>'key' = attempt.node_key
  AND declared.value->>'key' = completion.outcome
  AND declared.value->'schema' = record.schema_reference
  AND declared.value ? 'issue_presentation'
  AND ($8::bigint IS NULL OR (run.admitted_at, run.id, completion.completed_at, attempt.id) <
       (to_timestamp($8::double precision / 1000.0), $9::uuid,
        to_timestamp($10::double precision / 1000.0), $11::uuid))
ORDER BY run.admitted_at DESC, run.id DESC, completion.completed_at DESC, attempt.id DESC
LIMIT $12
