-- Extend the existing fenced Reporter journal to accepted Issue revisions.
-- An Issue effect owns no synthetic execution and retains its original input.
ALTER TABLE factory_outbox_records ALTER COLUMN run_id DROP NOT NULL;
ALTER TABLE factory_outbox_records ADD COLUMN issue JSONB;
ALTER TABLE factory_outbox_records ADD CONSTRAINT factory_outbox_subject CHECK (
  (run_id IS NOT NULL AND issue IS NULL)
  OR (run_id IS NULL AND kind='issue.status.projection'
    AND issue IS NOT NULL AND jsonb_typeof(issue)='object'
    AND issue ?& ARRAY['project_id','issue_id','version']
    AND octet_length(issue::text) <= 512)
);
CREATE INDEX factory_issue_reporter_subject ON factory_outbox_records
  ((issue->>'project_id'),(issue->>'issue_id'),(issue->>'version')) WHERE issue IS NOT NULL;
CREATE INDEX IF NOT EXISTS issue_reporter_desired ON outbox_entries
  (aggregate_identity,((payload->'push_intent'->>'version')::bigint),id)
  WHERE payload->'push_intent'->'desired' IS NOT NULL AND payload->'push_intent'->'desired'<>'null'::jsonb;
