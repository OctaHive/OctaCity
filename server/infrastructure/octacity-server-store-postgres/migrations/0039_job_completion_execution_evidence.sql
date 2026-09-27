ALTER TABLE job_completions
  ADD COLUMN execution JSONB;

ALTER TABLE job_completions
  ADD CONSTRAINT job_completions_execution_object
  CHECK (execution IS NULL OR jsonb_typeof(execution) = 'object');
