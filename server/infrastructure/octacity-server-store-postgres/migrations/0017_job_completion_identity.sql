ALTER TABLE job_completions
  ADD COLUMN completion_id TEXT;

UPDATE job_completions
SET completion_id = 'legacy-' || lease_id::text;

ALTER TABLE job_completions
  ALTER COLUMN completion_id SET NOT NULL,
  ADD CONSTRAINT job_completions_completion_id_nonempty
    CHECK (length(completion_id) BETWEEN 1 AND 128),
  ADD CONSTRAINT job_completions_completion_id_unique UNIQUE (completion_id);
