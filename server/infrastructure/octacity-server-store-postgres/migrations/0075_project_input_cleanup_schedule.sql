-- Choose due preparations through an ordered index, including retained rechecks.
ALTER TABLE project_text_preparations
  ADD COLUMN cleanup_at_millis BIGINT,
  ADD COLUMN run_id UUID GENERATED ALWAYS AS ((document->'admission'->'run'->>'id')::uuid) STORED;
UPDATE project_text_preparations p SET cleanup_at_millis = COALESCE(
  (SELECT MIN(COALESCE(o.recheck_at_millis,(p.document->>'expires_at')::bigint))
    FROM project_text_input_objects o WHERE o.preparation_id=p.id),
  (p.document->>'expires_at')::bigint);
ALTER TABLE project_text_preparations ALTER COLUMN cleanup_at_millis SET NOT NULL;
CREATE INDEX project_text_cleanup_due ON project_text_preparations(cleanup_at_millis,id);
CREATE INDEX project_text_preparation_run ON project_text_preparations(run_id) WHERE run_id IS NOT NULL;
DROP INDEX project_input_cleanup;
