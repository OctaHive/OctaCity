SELECT convert_from(record.document, 'UTF8')::jsonb #> '{payload,value}' AS value, work.envelope->'artifacts' AS artifacts,
  effect.classification, effect.rejected
FROM issue_work_associations AS a
JOIN factory_runs AS run ON run.id = a.run_id AND run.project_id = a.project_id
JOIN factory_work_envelopes AS work ON work.id = run.work_envelope_id AND work.id = a.work_id
JOIN factory_flow_records AS record ON record.run_id = run.id
JOIN issues AS issue ON issue.project_id = a.project_id AND issue.id = a.issue_id
LEFT JOIN LATERAL (
  SELECT receipt.outcome->'classification' AS classification,
    receipt.outcome->'classification_rejected' = 'true'::jsonb AS rejected
  FROM idempotency_records AS receipt
  WHERE $7 AND receipt.scope = 'apply-issue-classification'
    AND COALESCE(receipt.outcome #>> '{classification,run_id}', receipt.outcome #>> '{provenance,run_id}') = run.id::text
    AND COALESCE(receipt.outcome #>> '{classification,record}', receipt.outcome #>> '{provenance,record}') = encode(record.id, 'hex')
    AND (
      (receipt.outcome #>> '{provenance,project_id}' = a.project_id::text
        AND receipt.outcome #>> '{provenance,issue_id}' = a.issue_id::text
        AND receipt.outcome #>> '{provenance,work_id}' = work.id::text)
      OR
      (receipt.outcome->'provenance' IS NULL
        AND receipt.outcome #>> '{value,issue,project_id}' = a.project_id::text
        AND receipt.outcome #>> '{value,issue,id}' = a.issue_id::text)
    )
    AND (receipt.outcome->'classification_rejected' = 'true'::jsonb
      OR receipt.outcome #>> '{classification,priority}' IS NOT NULL
      OR receipt.outcome #>> '{classification,size_label}' IS NOT NULL)
  -- A conflicting replay cannot erase a confirmed write for the same result.
  ORDER BY (receipt.outcome ? 'classification') DESC, receipt.created_at DESC,
    receipt.idempotency_key, receipt.security_scope
  LIMIT 1
) AS effect ON TRUE
WHERE issue.project_id = $1 AND issue.id = $2 AND issue.current_version = $3 AND run.visible
  AND run.id = $4 AND work.id = $5 AND record.node_attempt_id = $6
