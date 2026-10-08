-- Durable, replayable model-call context. The authoritative bytes referenced
-- by a manifest remain in the Artifact Store; PostgreSQL retains the bounded
-- canonical records and exact dependency graph needed after process loss.

ALTER TABLE factory_stage_attempts
  ADD CONSTRAINT factory_stage_attempts_id_run_unique UNIQUE (id, run_id);

ALTER TABLE factory_call_nodes
  ADD CONSTRAINT factory_call_nodes_id_run_unique UNIQUE (id, run_id),
  ADD CONSTRAINT factory_call_nodes_stage_run_fk
    FOREIGN KEY (stage_attempt_id, run_id) REFERENCES factory_stage_attempts(id, run_id),
  ADD CONSTRAINT factory_call_nodes_parent_run_fk
    FOREIGN KEY (parent_call_id, run_id) REFERENCES factory_call_nodes(id, run_id);

CREATE TABLE factory_stage_handoffs (
  id UUID PRIMARY KEY,
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  stage_attempt_id UUID NOT NULL UNIQUE REFERENCES factory_stage_attempts(id),
  content_digest BYTEA NOT NULL,
  handoff JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT factory_stage_handoffs_digest_shape CHECK (octet_length(content_digest) = 32),
  CONSTRAINT factory_stage_handoffs_stage_run_fk
    FOREIGN KEY (stage_attempt_id, run_id) REFERENCES factory_stage_attempts(id, run_id),
  CONSTRAINT factory_stage_handoffs_document_shape CHECK (
    jsonb_typeof(handoff) = 'object' AND octet_length(handoff::text) BETWEEN 2 AND 262144
  )
);

CREATE TABLE factory_context_manifests (
  id UUID PRIMARY KEY,
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  stage_attempt_id UUID NOT NULL REFERENCES factory_stage_attempts(id),
  content_digest BYTEA NOT NULL,
  policy_digest BYTEA NOT NULL,
  manifest JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT factory_context_manifests_content_digest_shape CHECK (octet_length(content_digest) = 32),
  CONSTRAINT factory_context_manifests_policy_digest_shape CHECK (octet_length(policy_digest) = 32),
  CONSTRAINT factory_context_manifests_id_run_unique UNIQUE (id, run_id),
  CONSTRAINT factory_context_manifests_stage_run_fk
    FOREIGN KEY (stage_attempt_id, run_id) REFERENCES factory_stage_attempts(id, run_id),
  CONSTRAINT factory_context_manifests_document_shape CHECK (
    jsonb_typeof(manifest) = 'object' AND octet_length(manifest::text) BETWEEN 2 AND 262144
  )
);

ALTER TABLE factory_call_nodes
  ADD COLUMN context_manifest_id UUID NOT NULL REFERENCES factory_context_manifests(id),
  ADD COLUMN depth SMALLINT NOT NULL,
  ADD CONSTRAINT factory_call_nodes_context_run_fk
    FOREIGN KEY (context_manifest_id, run_id) REFERENCES factory_context_manifests(id, run_id);

ALTER TABLE factory_call_nodes
  ADD CONSTRAINT factory_call_nodes_depth_shape CHECK (depth BETWEEN 1 AND 32);

CREATE TABLE factory_call_stage_dependencies (
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  call_id UUID NOT NULL REFERENCES factory_call_nodes(id),
  stage_attempt_id UUID NOT NULL REFERENCES factory_stage_attempts(id),
  PRIMARY KEY (call_id, stage_attempt_id),
  CONSTRAINT factory_call_stage_dependencies_call_run_fk
    FOREIGN KEY (call_id, run_id) REFERENCES factory_call_nodes(id, run_id),
  CONSTRAINT factory_call_stage_dependencies_stage_run_fk
    FOREIGN KEY (stage_attempt_id, run_id) REFERENCES factory_stage_attempts(id, run_id)
);

CREATE TABLE factory_call_dependencies (
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  call_id UUID NOT NULL REFERENCES factory_call_nodes(id),
  dependency_call_id UUID NOT NULL REFERENCES factory_call_nodes(id),
  PRIMARY KEY (call_id, dependency_call_id),
  CONSTRAINT factory_call_dependencies_call_run_fk
    FOREIGN KEY (call_id, run_id) REFERENCES factory_call_nodes(id, run_id),
  CONSTRAINT factory_call_dependencies_dependency_run_fk
    FOREIGN KEY (dependency_call_id, run_id) REFERENCES factory_call_nodes(id, run_id),
  CONSTRAINT factory_call_dependencies_not_self CHECK (call_id <> dependency_call_id)
);

CREATE TABLE factory_call_completions (
  call_id UUID PRIMARY KEY REFERENCES factory_call_nodes(id),
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  terminal TEXT NOT NULL,
  content_digest BYTEA NOT NULL,
  completion JSONB NOT NULL,
  completed_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT factory_call_completions_call_run_fk
    FOREIGN KEY (call_id, run_id) REFERENCES factory_call_nodes(id, run_id),
  CONSTRAINT factory_call_completions_terminal_shape CHECK (
    terminal IN ('succeeded', 'timed_out', 'cancelled', 'provider_unavailable',
                 'output_overflow', 'missing_deliverable', 'invalid_json',
                 'invalid_schema', 'integrity_failure', 'budget_exceeded',
                 'execution_failed', 'infrastructure_failed')
  ),
  CONSTRAINT factory_call_completions_digest_shape CHECK (octet_length(content_digest) = 32),
  CONSTRAINT factory_call_completions_document_shape CHECK (
    jsonb_typeof(completion) = 'object' AND octet_length(completion::text) BETWEEN 2 AND 262144
  )
);

ALTER TABLE factory_artifact_references
  DROP CONSTRAINT factory_artifact_references_role_known,
  ADD CONSTRAINT factory_artifact_references_role_known CHECK (
    role IN ('task', 'acceptance', 'specification', 'build_output', 'changeset_bundle',
             'changeset_manifest', 'evidence', 'stage_handoff', 'call_context', 'call_output')
  );

CREATE INDEX factory_stage_handoffs_diagnostics_idx ON factory_stage_handoffs (run_id, id);
CREATE INDEX factory_context_manifests_diagnostics_idx ON factory_context_manifests (run_id, id);
CREATE INDEX factory_call_completions_diagnostics_idx ON factory_call_completions (run_id, call_id);

-- Schemas 47-49 ship in the same server release. The immediately preceding
-- binary supports schema 46 and therefore has no Factory history to backfill.
