-- Freeze upgrade membership once. Never capture a new admission awaiting its
-- ordinary Issue association, and never turn GETs into migration commands.
CREATE TABLE issue_legacy_backfill (
  work_id UUID PRIMARY KEY REFERENCES factory_work_envelopes(id)
);
INSERT INTO issue_legacy_backfill(work_id)
SELECT w.id FROM factory_work_envelopes w JOIN factory_runs r ON r.work_envelope_id=w.id
WHERE r.visible
  AND NOT EXISTS (SELECT 1 FROM issue_work_associations a WHERE a.work_id=w.id)
  AND NOT EXISTS (
    SELECT 1 FROM project_text_preparations p JOIN issue_execution_requests q
      ON q.project_id=p.project_id AND q.security_scope=p.security_scope
      AND p.document->'intent'->>'external_identity'='issue-execution/' || q.id::text
    WHERE p.run_id=r.id
  );
