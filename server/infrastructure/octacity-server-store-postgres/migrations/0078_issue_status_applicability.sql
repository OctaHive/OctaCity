-- Applicability belongs to the original reviewed Work association, not Run state.
-- Backfill from its retained requirements revision, never the mutable Issue head.
ALTER TABLE issue_work_associations ADD COLUMN status_applicability_version BIGINT
  CHECK (status_applicability_version > 0);
UPDATE issue_work_associations a
SET status_applicability_version = (r.document->>'applicability_version')::bigint
FROM issue_execution_requests q, issue_revisions r
WHERE a.request_id=q.id AND r.project_id=a.project_id AND r.issue_id=a.issue_id
  AND r.version=q.requirements_version;
