-- Address only Issue revision receipts during atomic body/replay retirement.
-- Other management, agent and CI receipts do not enter this index.
CREATE INDEX issue_revision_receipts
ON idempotency_records (
  (outcome #>> '{value,issue,project_id}'),
  (outcome #>> '{value,issue,id}'),
  ((outcome #>> '{value,issue,version}')::bigint)
)
WHERE outcome #>> '{value,issue,project_id}' IS NOT NULL;
