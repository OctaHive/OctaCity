-- Allocation occurs after locking the Issue: a committed watermark cannot skip
-- an earlier in-flight request for that same Issue. Retained rows gain a stable
-- sequence on upgrade; requirements and original request identities stay intact.
-- An Issue-local counter keeps other Issues out of public observation metadata.
ALTER TABLE issue_execution_requests ADD COLUMN sequence BIGINT;
WITH retained AS (
  SELECT id, row_number() OVER (
    PARTITION BY project_id, issue_id ORDER BY (document->>'requested_at')::bigint, id
  ) AS sequence FROM issue_execution_requests
)
UPDATE issue_execution_requests r SET sequence = retained.sequence
  FROM retained WHERE r.id = retained.id;
ALTER TABLE issue_execution_requests
  ALTER COLUMN sequence SET NOT NULL,
  ADD CONSTRAINT issue_execution_requests_sequence_positive CHECK (sequence > 0);
CREATE UNIQUE INDEX issue_execution_requests_sequence
  ON issue_execution_requests(project_id, issue_id, sequence);
CREATE INDEX issue_execution_requests_history
  ON issue_execution_requests(project_id, issue_id, ((document->>'requested_at')::bigint), id)
  INCLUDE (sequence);
