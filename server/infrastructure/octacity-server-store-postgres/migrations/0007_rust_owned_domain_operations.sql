-- Move domain behavior out of historical database routines and establish the
-- resource/version table shape used by the Rust-owned publication operations.

DROP TRIGGER projects_reject_cycle ON projects;
DROP TRIGGER pipeline_versions_reject_mutation ON pipeline_versions;
DROP TRIGGER attempts_require_next_number ON attempts;
DROP TRIGGER attempts_reject_identity_update ON attempts;
DROP TRIGGER attempts_reject_delete ON attempts;
DROP TRIGGER ready_queue_guard_current_ownership ON ready_queue_entries;
DROP TRIGGER leases_guard_current_ownership ON leases;
DROP TRIGGER job_events_require_next_sequence ON job_events;
DROP TRIGGER job_events_reject_mutation ON job_events;
DROP TRIGGER artifacts_preserve_published_identity ON artifacts;
DROP TRIGGER audit_facts_are_immutable ON audit_facts;
DROP TRIGGER log_indexing_work_require_next_position ON log_indexing_work;
DROP TRIGGER log_indexing_work_preserve_position_identity ON log_indexing_work;

DROP FUNCTION octacity_reject_project_cycle();
DROP FUNCTION octacity_reject_pipeline_snapshot_mutation();
DROP FUNCTION octacity_require_next_attempt_number();
DROP FUNCTION octacity_reject_attempt_identity_mutation();
DROP FUNCTION octacity_guard_ready_job_ownership();
DROP FUNCTION octacity_guard_leased_job_ownership();
DROP FUNCTION octacity_require_next_event_sequence();
DROP FUNCTION octacity_reject_event_mutation();
DROP FUNCTION octacity_preserve_published_artifact_identity();
DROP FUNCTION reject_audit_fact_mutation();
DROP FUNCTION octacity_require_next_log_index_position();
DROP FUNCTION octacity_preserve_log_index_position_identity();

ALTER TABLE repositories RENAME TO repository_versions;
ALTER TABLE repository_versions RENAME COLUMN id TO repository_id;
ALTER TABLE repository_versions RENAME COLUMN created_at TO published_at;
ALTER TABLE repository_versions RENAME CONSTRAINT repositories_pkey TO repository_versions_pkey;

CREATE TABLE repositories (
  id UUID PRIMARY KEY,
  project_id UUID NOT NULL REFERENCES projects(id),
  name TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL
);

INSERT INTO repositories (id, project_id, name, created_at)
SELECT DISTINCT ON (repository_id) repository_id, project_id, name, published_at
  FROM repository_versions
  ORDER BY repository_id, version;

ALTER TABLE repository_versions
  DROP COLUMN project_id,
  DROP COLUMN name,
  ADD CONSTRAINT repository_versions_repository_id_fkey
    FOREIGN KEY (repository_id) REFERENCES repositories(id);

ALTER TABLE build_configurations RENAME TO build_configuration_versions;
ALTER TABLE build_configuration_versions RENAME COLUMN id TO build_configuration_id;
ALTER TABLE build_configuration_versions RENAME COLUMN created_at TO published_at;
ALTER TABLE build_configuration_versions
  RENAME CONSTRAINT build_configurations_pkey TO build_configuration_versions_pkey;

CREATE TABLE build_configurations (
  id UUID PRIMARY KEY,
  project_id UUID NOT NULL REFERENCES projects(id),
  name TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL
);

INSERT INTO build_configurations (id, project_id, name, created_at)
SELECT DISTINCT ON (build_configuration_id) build_configuration_id, project_id, name, published_at
  FROM build_configuration_versions
  ORDER BY build_configuration_id, version;

ALTER TABLE build_configuration_versions
  DROP COLUMN project_id,
  DROP COLUMN name,
  ADD CONSTRAINT build_configuration_versions_configuration_id_fkey
    FOREIGN KEY (build_configuration_id) REFERENCES build_configurations(id);

-- Transactional invariants for the hierarchical Project aggregate.

ALTER TABLE projects
  ADD CONSTRAINT projects_positive_version CHECK (version > 0),
  ADD CONSTRAINT projects_timestamps_ordered CHECK (updated_at >= created_at),
  ADD CONSTRAINT projects_sibling_name_key UNIQUE NULLS NOT DISTINCT (parent_id, name);
