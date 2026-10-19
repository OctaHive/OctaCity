-- Keep immutable cleanup identities for delayed writes and interrupted acknowledgements.
ALTER TABLE project_text_input_objects
  ADD COLUMN deletion_started_at_millis BIGINT,
  ADD COLUMN deleted_at_millis BIGINT,
  ADD COLUMN recheck_at_millis BIGINT;
CREATE INDEX project_input_cleanup ON project_text_input_objects(recheck_at_millis,preparation_id);
