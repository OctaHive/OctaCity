-- Every Job retains stable execution intent. Blocked Jobs have no signed
-- envelope; an envelope is issued when a Job becomes ready and may remain as
-- immutable execution history after the Job leaves an active state.
ALTER TABLE jobs
  ADD COLUMN job_spec_template JSONB NOT NULL,
  ADD COLUMN dependency_policy JSONB NOT NULL,
  ADD COLUMN signed_job_spec JSONB;

-- Fan-in policy belongs to the dependent Job, not to each incoming edge.
ALTER TABLE job_dependencies
  DROP COLUMN dependency_policy;
