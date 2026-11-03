-- Index exact accepted results for both committed and rejected local classifications.
CREATE INDEX issue_classification_outcome_receipts
ON idempotency_records (
  (COALESCE(outcome #>> '{classification,run_id}', outcome #>> '{provenance,run_id}')),
  (COALESCE(outcome #>> '{classification,record}', outcome #>> '{provenance,record}'))
)
WHERE scope = 'apply-issue-classification';
