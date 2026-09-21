-- Scheduling and recovery predicates must not depend on the JSON spelling of
-- immutable API snapshots. Backfill typed columns once, then keep them in sync
-- at the publication and Build-acceptance boundaries.

ALTER TABLE build_configuration_versions
  ADD COLUMN job_concurrency_limit BIGINT,
  ADD COLUMN allowed_pool_ids UUID[],
  ADD COLUMN retry_max_attempts BIGINT,
  ADD COLUMN retries_infrastructure BOOLEAN;

UPDATE build_configuration_versions
SET job_concurrency_limit = (configuration_snapshot ->> 'job_concurrency_limit')::BIGINT,
    allowed_pool_ids = ARRAY(
      SELECT jsonb_array_elements_text(configuration_snapshot -> 'allowed_pools')::UUID
    ),
    retry_max_attempts = (configuration_snapshot -> 'retry' ->> 'max_attempts')::BIGINT,
    retries_infrastructure = (configuration_snapshot -> 'retry' -> 'retry_on') ? 'infrastructure_failure';

ALTER TABLE build_configuration_versions
  ALTER COLUMN job_concurrency_limit SET NOT NULL,
  ALTER COLUMN allowed_pool_ids SET NOT NULL,
  ALTER COLUMN retry_max_attempts SET NOT NULL,
  ALTER COLUMN retries_infrastructure SET NOT NULL,
  ADD CONSTRAINT build_configuration_versions_positive_job_concurrency
    CHECK (job_concurrency_limit > 0),
  ADD CONSTRAINT build_configuration_versions_nonempty_allowed_pools
    CHECK (cardinality(allowed_pool_ids) > 0),
  ADD CONSTRAINT build_configuration_versions_positive_retry_attempts
    CHECK (retry_max_attempts > 0);

ALTER TABLE builds
  ADD COLUMN project_job_concurrency_limit BIGINT;

UPDATE builds
SET project_job_concurrency_limit =
  (effective_policy_snapshot #>> '{project,policy,concurrency,active_jobs}')::BIGINT;

ALTER TABLE builds
  ALTER COLUMN project_job_concurrency_limit SET NOT NULL,
  ADD CONSTRAINT builds_positive_project_job_concurrency
    CHECK (project_job_concurrency_limit > 0);
