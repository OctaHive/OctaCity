-- Persist the complete deterministic result of each atomic Job completion so
-- exact replays return the original graph transition and aggregate states.

ALTER TABLE job_completions
  ADD COLUMN skipped_job_ids UUID[] NOT NULL DEFAULT '{}',
  ADD COLUMN attempt_state TEXT,
  ADD COLUMN build_state TEXT;

UPDATE job_completions AS completion
SET attempt_state = attempt.state,
    build_state = build.state
FROM jobs AS job
JOIN attempts AS attempt ON attempt.id = job.attempt_id
JOIN builds AS build ON build.id = attempt.build_id
WHERE completion.job_id = job.id;

ALTER TABLE job_completions
  ALTER COLUMN attempt_state SET NOT NULL,
  ALTER COLUMN build_state SET NOT NULL,
  ADD COLUMN failure_class TEXT,
  ADD CONSTRAINT job_completions_attempt_state
    CHECK (attempt_state IN ('running', 'succeeded', 'failed', 'cancelled')),
  ADD CONSTRAINT job_completions_build_state
    CHECK (build_state IN ('running', 'succeeded', 'failed', 'cancelled')),
  ADD CONSTRAINT job_completions_failure_class
    CHECK (
      (kind = 'failed' AND failure_class IN ('execution', 'infrastructure'))
      OR (kind <> 'failed' AND failure_class IS NULL)
    );

ALTER TABLE job_completions
  ALTER COLUMN skipped_job_ids DROP DEFAULT;
