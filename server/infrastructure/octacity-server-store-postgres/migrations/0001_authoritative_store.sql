-- OctaCity authoritative control-plane schema.
--
-- This first forward migration establishes storage groups and foreign-key
-- shape. Cross-row uniqueness, immutable snapshot, fencing, acyclicity and
-- claim indexes are added by the dedicated invariant migration in task 2.3.

CREATE TABLE projects (
  id UUID PRIMARY KEY,
  parent_id UUID REFERENCES projects(id),
  name TEXT NOT NULL,
  version BIGINT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE project_policy_versions (
  project_id UUID NOT NULL REFERENCES projects(id),
  version BIGINT NOT NULL,
  policy JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  PRIMARY KEY (project_id, version)
);

CREATE TABLE repositories (
  id UUID NOT NULL,
  version BIGINT NOT NULL,
  project_id UUID NOT NULL REFERENCES projects(id),
  name TEXT NOT NULL,
  vcs_integration_id UUID NOT NULL,
  repository_locator TEXT NOT NULL,
  selection_policy JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  PRIMARY KEY (id, version)
);

CREATE TABLE pipelines (
  id UUID PRIMARY KEY,
  project_id UUID NOT NULL REFERENCES projects(id),
  name TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE pipeline_versions (
  pipeline_id UUID NOT NULL REFERENCES pipelines(id),
  version BIGINT NOT NULL,
  dag_snapshot JSONB NOT NULL,
  published_at TIMESTAMPTZ NOT NULL,
  PRIMARY KEY (pipeline_id, version)
);

CREATE TABLE build_configurations (
  id UUID NOT NULL,
  version BIGINT NOT NULL,
  project_id UUID NOT NULL REFERENCES projects(id),
  name TEXT NOT NULL,
  enabled BOOLEAN NOT NULL,
  repository_id UUID NOT NULL,
  repository_version BIGINT NOT NULL,
  pipeline_id UUID NOT NULL,
  pipeline_version BIGINT NOT NULL,
  configuration_snapshot JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  PRIMARY KEY (id, version),
  FOREIGN KEY (repository_id, repository_version) REFERENCES repositories(id, version),
  FOREIGN KEY (pipeline_id, pipeline_version) REFERENCES pipeline_versions(pipeline_id, version)
);

CREATE TABLE triggers (
  id UUID NOT NULL,
  version BIGINT NOT NULL,
  build_configuration_id UUID NOT NULL,
  build_configuration_version BIGINT NOT NULL,
  kind TEXT NOT NULL,
  enabled BOOLEAN NOT NULL,
  definition JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  PRIMARY KEY (id, version),
  FOREIGN KEY (build_configuration_id, build_configuration_version)
    REFERENCES build_configurations(id, version)
);

CREATE TABLE schedules (
  trigger_id UUID NOT NULL,
  trigger_version BIGINT NOT NULL,
  expression TEXT NOT NULL,
  timezone TEXT NOT NULL,
  next_occurrence_at TIMESTAMPTZ NOT NULL,
  missed_run_policy JSONB NOT NULL,
  claim_owner TEXT,
  claim_expires_at TIMESTAMPTZ,
  PRIMARY KEY (trigger_id, trigger_version),
  FOREIGN KEY (trigger_id, trigger_version) REFERENCES triggers(id, version)
);

CREATE TABLE trigger_occurrences (
  id UUID PRIMARY KEY,
  trigger_id UUID NOT NULL,
  trigger_version BIGINT NOT NULL,
  deduplication_identity TEXT NOT NULL,
  cause JSONB NOT NULL,
  source_time TIMESTAMPTZ NOT NULL,
  state TEXT NOT NULL,
  build_id UUID,
  created_at TIMESTAMPTZ NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL,
  FOREIGN KEY (trigger_id, trigger_version) REFERENCES triggers(id, version)
);

CREATE TABLE pools (
  id UUID NOT NULL,
  version BIGINT NOT NULL,
  name TEXT NOT NULL,
  enabled BOOLEAN NOT NULL,
  drain_state TEXT NOT NULL,
  admission_policy JSONB NOT NULL,
  concurrency_limit BIGINT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  PRIMARY KEY (id, version)
);

CREATE TABLE agents (
  id UUID PRIMARY KEY,
  name TEXT NOT NULL,
  pool_id UUID NOT NULL,
  pool_version BIGINT NOT NULL,
  state TEXT NOT NULL,
  inventory JSONB NOT NULL,
  version BIGINT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL,
  FOREIGN KEY (pool_id, pool_version) REFERENCES pools(id, version)
);

CREATE TABLE agent_registrations (
  id UUID PRIMARY KEY,
  agent_id UUID NOT NULL REFERENCES agents(id),
  epoch BIGINT NOT NULL,
  credential_hash BYTEA NOT NULL,
  inventory JSONB NOT NULL,
  registered_at TIMESTAMPTZ NOT NULL,
  expires_at TIMESTAMPTZ NOT NULL,
  revoked_at TIMESTAMPTZ
);

CREATE TABLE builds (
  id UUID PRIMARY KEY,
  project_id UUID NOT NULL REFERENCES projects(id),
  build_configuration_id UUID NOT NULL,
  build_configuration_version BIGINT NOT NULL,
  pipeline_id UUID NOT NULL,
  pipeline_version BIGINT NOT NULL,
  repository_id UUID NOT NULL,
  repository_version BIGINT NOT NULL,
  trigger_occurrence_id UUID NOT NULL REFERENCES trigger_occurrences(id),
  immutable_revision TEXT NOT NULL,
  input_snapshot JSONB NOT NULL,
  effective_policy_snapshot JSONB NOT NULL,
  state TEXT NOT NULL,
  version BIGINT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL,
  FOREIGN KEY (build_configuration_id, build_configuration_version)
    REFERENCES build_configurations(id, version),
  FOREIGN KEY (pipeline_id, pipeline_version) REFERENCES pipeline_versions(pipeline_id, version),
  FOREIGN KEY (repository_id, repository_version) REFERENCES repositories(id, version)
);

ALTER TABLE trigger_occurrences
  ADD CONSTRAINT trigger_occurrences_build_fk FOREIGN KEY (build_id) REFERENCES builds(id);

CREATE TABLE attempts (
  id UUID PRIMARY KEY,
  build_id UUID NOT NULL REFERENCES builds(id),
  attempt_number BIGINT NOT NULL,
  state TEXT NOT NULL,
  version BIGINT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE jobs (
  id UUID PRIMARY KEY,
  attempt_id UUID NOT NULL REFERENCES attempts(id),
  pipeline_node_id TEXT NOT NULL,
  state TEXT NOT NULL,
  job_snapshot JSONB NOT NULL,
  allowed_pool_ids UUID[] NOT NULL,
  requirements JSONB NOT NULL,
  version BIGINT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE job_dependencies (
  attempt_id UUID NOT NULL REFERENCES attempts(id),
  job_id UUID NOT NULL REFERENCES jobs(id),
  dependency_job_id UUID NOT NULL REFERENCES jobs(id),
  dependency_policy JSONB NOT NULL,
  PRIMARY KEY (job_id, dependency_job_id)
);

CREATE TABLE ready_queue_entries (
  job_id UUID PRIMARY KEY REFERENCES jobs(id),
  priority BIGINT NOT NULL,
  enqueue_order BIGINT NOT NULL,
  enqueued_at TIMESTAMPTZ NOT NULL,
  project_id UUID NOT NULL REFERENCES projects(id),
  build_configuration_id UUID NOT NULL,
  build_configuration_version BIGINT NOT NULL,
  allowed_pool_ids UUID[] NOT NULL,
  requirements JSONB NOT NULL,
  claim_owner TEXT,
  claim_expires_at TIMESTAMPTZ,
  FOREIGN KEY (build_configuration_id, build_configuration_version)
    REFERENCES build_configurations(id, version)
);

CREATE TABLE leases (
  id UUID PRIMARY KEY,
  job_id UUID NOT NULL REFERENCES jobs(id),
  pool_id UUID NOT NULL,
  pool_version BIGINT NOT NULL,
  registration_id UUID NOT NULL REFERENCES agent_registrations(id),
  fence_hash BYTEA NOT NULL,
  state TEXT NOT NULL,
  version BIGINT NOT NULL,
  leased_at TIMESTAMPTZ NOT NULL,
  expires_at TIMESTAMPTZ NOT NULL,
  completed_at TIMESTAMPTZ,
  FOREIGN KEY (pool_id, pool_version) REFERENCES pools(id, version)
);

CREATE TABLE job_events (
  job_id UUID NOT NULL REFERENCES jobs(id),
  sequence BIGINT NOT NULL,
  lease_id UUID NOT NULL REFERENCES leases(id),
  event_kind TEXT NOT NULL,
  event_time TIMESTAMPTZ NOT NULL,
  payload JSONB NOT NULL,
  event_digest BYTEA NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  PRIMARY KEY (job_id, sequence)
);

CREATE TABLE log_chunk_manifests (
  id UUID PRIMARY KEY,
  build_id UUID NOT NULL REFERENCES builds(id),
  attempt_id UUID NOT NULL REFERENCES attempts(id),
  job_id UUID NOT NULL REFERENCES jobs(id),
  stream TEXT NOT NULL,
  first_sequence BIGINT NOT NULL,
  last_sequence BIGINT NOT NULL,
  object_identity TEXT NOT NULL,
  byte_length BIGINT NOT NULL,
  sha256 BYTEA NOT NULL,
  visible BOOLEAN NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  deleted_at TIMESTAMPTZ
);

CREATE TABLE log_search_documents (
  id UUID PRIMARY KEY,
  chunk_id UUID NOT NULL REFERENCES log_chunk_manifests(id),
  build_id UUID NOT NULL REFERENCES builds(id),
  attempt_id UUID NOT NULL REFERENCES attempts(id),
  job_id UUID NOT NULL REFERENCES jobs(id),
  stream TEXT NOT NULL,
  first_sequence BIGINT NOT NULL,
  last_sequence BIGINT NOT NULL,
  normalized_text TEXT NOT NULL,
  indexed_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE log_indexing_work (
  id UUID PRIMARY KEY,
  project_id UUID NOT NULL REFERENCES projects(id),
  position BIGINT NOT NULL,
  build_id UUID NOT NULL REFERENCES builds(id),
  chunk_id UUID REFERENCES log_chunk_manifests(id),
  operation TEXT NOT NULL,
  state TEXT NOT NULL,
  attempt_count BIGINT NOT NULL,
  available_at TIMESTAMPTZ NOT NULL,
  claim_owner TEXT,
  claim_expires_at TIMESTAMPTZ,
  last_error_code TEXT,
  created_at TIMESTAMPTZ NOT NULL,
  completed_at TIMESTAMPTZ,
  CONSTRAINT log_indexing_work_positive_position CHECK (position > 0),
  CONSTRAINT log_indexing_work_project_position_unique UNIQUE (project_id, position),
  CONSTRAINT log_indexing_work_operation_shape
    CHECK ((operation IN ('index', 'rebuild') AND chunk_id IS NOT NULL) OR (operation = 'delete_build' AND chunk_id IS NULL))
);

CREATE TABLE log_search_build_tombstones (
  project_id UUID NOT NULL REFERENCES projects(id),
  build_id UUID NOT NULL REFERENCES builds(id),
  deleted_at_position BIGINT NOT NULL,
  PRIMARY KEY (project_id, build_id),
  CONSTRAINT log_search_tombstones_positive_position CHECK (deleted_at_position > 0),
  CONSTRAINT log_search_tombstones_project_position_unique UNIQUE (project_id, deleted_at_position)
);

CREATE TABLE artifact_uploads (
  id UUID PRIMARY KEY,
  artifact_id UUID NOT NULL,
  job_id UUID NOT NULL REFERENCES jobs(id),
  lease_id UUID NOT NULL REFERENCES leases(id),
  idempotency_key TEXT NOT NULL,
  logical_name TEXT NOT NULL,
  media_type TEXT NOT NULL,
  report_format TEXT,
  expected_size BIGINT NOT NULL,
  expected_sha256 BYTEA NOT NULL,
  object_identity TEXT NOT NULL,
  object_generation TEXT,
  state TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  expires_at TIMESTAMPTZ NOT NULL,
  completed_at TIMESTAMPTZ
);

CREATE TABLE artifacts (
  id UUID PRIMARY KEY,
  build_id UUID NOT NULL REFERENCES builds(id),
  attempt_id UUID NOT NULL REFERENCES attempts(id),
  job_id UUID NOT NULL REFERENCES jobs(id),
  logical_name TEXT NOT NULL,
  media_type TEXT NOT NULL,
  report_format TEXT,
  byte_length BIGINT NOT NULL,
  sha256 BYTEA NOT NULL,
  object_identity TEXT NOT NULL,
  object_generation TEXT NOT NULL,
  state TEXT NOT NULL,
  retention_until TIMESTAMPTZ,
  published_at TIMESTAMPTZ,
  deleted_at TIMESTAMPTZ
);

ALTER TABLE artifact_uploads
  ADD CONSTRAINT artifact_uploads_artifact_fk FOREIGN KEY (artifact_id) REFERENCES artifacts(id) DEFERRABLE INITIALLY DEFERRED;

CREATE TABLE cache_sessions (
  id UUID PRIMARY KEY,
  project_id UUID NOT NULL REFERENCES projects(id),
  build_id UUID NOT NULL REFERENCES builds(id),
  job_id UUID NOT NULL REFERENCES jobs(id),
  registration_id UUID NOT NULL REFERENCES agent_registrations(id),
  lease_id UUID NOT NULL REFERENCES leases(id),
  namespace TEXT NOT NULL,
  permissions JSONB NOT NULL,
  credential_hash BYTEA NOT NULL,
  quota_bytes BIGINT NOT NULL,
  state TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  expires_at TIMESTAMPTZ NOT NULL,
  revoked_at TIMESTAMPTZ
);

CREATE TABLE idempotency_records (
  scope TEXT NOT NULL,
  idempotency_key TEXT NOT NULL,
  request_digest BYTEA NOT NULL,
  outcome JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  expires_at TIMESTAMPTZ,
  PRIMARY KEY (scope, idempotency_key)
);

CREATE TABLE audit_facts (
  id UUID PRIMARY KEY,
  actor_kind TEXT NOT NULL,
  actor_identity TEXT,
  operation TEXT NOT NULL,
  target_kind TEXT NOT NULL,
  target_identity TEXT NOT NULL,
  request_identity TEXT,
  idempotency_key TEXT,
  outcome TEXT NOT NULL,
  safe_metadata JSONB NOT NULL,
  occurred_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE outbox_entries (
  id UUID PRIMARY KEY,
  topic TEXT NOT NULL,
  aggregate_kind TEXT NOT NULL,
  aggregate_identity TEXT NOT NULL,
  payload JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  available_at TIMESTAMPTZ NOT NULL,
  attempt_count BIGINT NOT NULL,
  claim_owner TEXT,
  claim_expires_at TIMESTAMPTZ,
  published_at TIMESTAMPTZ
);

CREATE TABLE adapter_retries (
  id UUID PRIMARY KEY,
  adapter_kind TEXT NOT NULL,
  operation TEXT NOT NULL,
  idempotency_identity TEXT NOT NULL,
  request JSONB NOT NULL,
  failure_code TEXT,
  attempt_count BIGINT NOT NULL,
  available_at TIMESTAMPTZ NOT NULL,
  claim_owner TEXT,
  claim_expires_at TIMESTAMPTZ,
  completed_at TIMESTAMPTZ
);

CREATE TABLE retention_work (
  id UUID PRIMARY KEY,
  resource_kind TEXT NOT NULL,
  resource_identity TEXT NOT NULL,
  phase TEXT NOT NULL,
  available_at TIMESTAMPTZ NOT NULL,
  attempt_count BIGINT NOT NULL,
  claim_owner TEXT,
  claim_expires_at TIMESTAMPTZ,
  completed_at TIMESTAMPTZ
);

CREATE TABLE worker_claims (
  work_kind TEXT NOT NULL,
  work_identity TEXT NOT NULL,
  owner TEXT NOT NULL,
  claimed_at TIMESTAMPTZ NOT NULL,
  expires_at TIMESTAMPTZ NOT NULL,
  PRIMARY KEY (work_kind, work_identity)
);
