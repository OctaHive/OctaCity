-- Durable Dark Factory authority is additive to ordinary CI/CD state. Large
-- payloads remain in the Artifact store; these rows retain only bounded,
-- canonical metadata, digests, exact references, and current projections.

CREATE TABLE factory_configurations (
  id UUID PRIMARY KEY,
  project_id UUID NOT NULL REFERENCES projects(id),
  current_version BIGINT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT factory_configurations_positive_version CHECK (current_version > 0),
  CONSTRAINT factory_configurations_timestamp_order CHECK (updated_at >= created_at)
);

CREATE TABLE factory_configuration_versions (
  factory_configuration_id UUID NOT NULL REFERENCES factory_configurations(id),
  version BIGINT NOT NULL,
  definition_digest BYTEA NOT NULL,
  definition JSONB NOT NULL,
  enabled BOOLEAN NOT NULL,
  published_at TIMESTAMPTZ NOT NULL,
  PRIMARY KEY (factory_configuration_id, version),
  CONSTRAINT factory_configuration_versions_positive_version CHECK (version > 0),
  CONSTRAINT factory_configuration_versions_digest_shape CHECK (octet_length(definition_digest) = 32),
  CONSTRAINT factory_configuration_versions_definition_shape CHECK (
    jsonb_typeof(definition) = 'object' AND octet_length(definition::text) BETWEEN 2 AND 1048576
  )
);

ALTER TABLE factory_configurations
  ADD CONSTRAINT factory_configurations_current_version_fkey
  FOREIGN KEY (id, current_version)
  REFERENCES factory_configuration_versions(factory_configuration_id, version)
  DEFERRABLE INITIALLY DEFERRED;

CREATE TABLE factory_work_envelopes (
  id UUID PRIMARY KEY,
  project_id UUID NOT NULL REFERENCES projects(id),
  factory_configuration_id UUID NOT NULL,
  factory_configuration_version BIGINT NOT NULL,
  source_kind TEXT NOT NULL,
  security_scope_digest BYTEA NOT NULL,
  external_identity TEXT NOT NULL,
  repository_id UUID NOT NULL,
  repository_version BIGINT NOT NULL,
  exact_revision TEXT NOT NULL,
  intent_digest BYTEA NOT NULL,
  envelope_digest BYTEA NOT NULL UNIQUE,
  envelope JSONB NOT NULL,
  admitted_at TIMESTAMPTZ NOT NULL,
  FOREIGN KEY (factory_configuration_id, factory_configuration_version)
    REFERENCES factory_configuration_versions(factory_configuration_id, version),
  FOREIGN KEY (repository_id, repository_version)
    REFERENCES repository_versions(repository_id, version),
  CONSTRAINT factory_work_envelopes_source_kind_shape CHECK (
    octet_length(source_kind) BETWEEN 1 AND 128 AND source_kind ~ '^[a-z0-9][a-z0-9._-]*$'
  ),
  CONSTRAINT factory_work_envelopes_security_scope_digest_shape CHECK (octet_length(security_scope_digest) = 32),
  CONSTRAINT factory_work_envelopes_external_identity_shape CHECK (
    octet_length(external_identity) BETWEEN 1 AND 256
    AND external_identity = btrim(external_identity)
    AND external_identity !~ '[[:cntrl:]]'
  ),
  CONSTRAINT factory_work_envelopes_exact_revision_shape CHECK (
    octet_length(exact_revision) BETWEEN 1 AND 512
    AND exact_revision = btrim(exact_revision)
    AND exact_revision !~ '[[:cntrl:]]'
  ),
  CONSTRAINT factory_work_envelopes_intent_digest_shape CHECK (octet_length(intent_digest) = 32),
  CONSTRAINT factory_work_envelopes_envelope_digest_shape CHECK (octet_length(envelope_digest) = 32),
  CONSTRAINT factory_work_envelopes_document_shape CHECK (
    jsonb_typeof(envelope) = 'object' AND octet_length(envelope::text) BETWEEN 2 AND 1048576
  ),
  CONSTRAINT factory_work_envelopes_external_identity_key
    UNIQUE (source_kind, security_scope_digest, external_identity)
);

CREATE TABLE factory_runs (
  id UUID PRIMARY KEY,
  project_id UUID NOT NULL REFERENCES projects(id),
  work_envelope_id UUID NOT NULL UNIQUE REFERENCES factory_work_envelopes(id),
  factory_configuration_id UUID NOT NULL,
  factory_configuration_version BIGINT NOT NULL,
  state TEXT NOT NULL,
  version BIGINT NOT NULL,
  subject_digest BYTEA NOT NULL,
  admitted_at TIMESTAMPTZ NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL,
  FOREIGN KEY (factory_configuration_id, factory_configuration_version)
    REFERENCES factory_configuration_versions(factory_configuration_id, version),
  CONSTRAINT factory_runs_state_known CHECK (
    state IN (
      'admitted', 'implementing', 'validating', 'evaluating', 'reworking',
      'ready_for_delivery', 'delivering', 'escalated', 'rejected', 'cancelled', 'completed'
    )
  ),
  CONSTRAINT factory_runs_positive_version CHECK (version > 0),
  CONSTRAINT factory_runs_subject_digest_shape CHECK (octet_length(subject_digest) = 32),
  CONSTRAINT factory_runs_timestamp_order CHECK (updated_at >= admitted_at)
);

CREATE TABLE factory_run_claims (
  id BYTEA PRIMARY KEY,
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  owner TEXT NOT NULL,
  fence BYTEA NOT NULL,
  claimed_at TIMESTAMPTZ NOT NULL,
  expires_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT factory_run_claims_id_shape CHECK (octet_length(id) = 32),
  CONSTRAINT factory_run_claims_owner_shape CHECK (
    octet_length(owner) BETWEEN 1 AND 128 AND owner ~ '^[a-z0-9][a-z0-9._-]*$'
  ),
  CONSTRAINT factory_run_claims_fence_shape CHECK (octet_length(fence) = 32),
  CONSTRAINT factory_run_claims_expiry_order CHECK (expires_at > claimed_at),
  CONSTRAINT factory_run_claims_fence_key UNIQUE (run_id, fence)
);

CREATE TABLE factory_run_budgets (
  id BYTEA PRIMARY KEY,
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  run_version BIGINT NOT NULL,
  usage JSONB NOT NULL,
  recorded_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT factory_run_budgets_id_shape CHECK (octet_length(id) = 32),
  CONSTRAINT factory_run_budgets_positive_version CHECK (run_version > 0),
  CONSTRAINT factory_run_budgets_usage_shape CHECK (
    jsonb_typeof(usage) = 'object' AND octet_length(usage::text) BETWEEN 2 AND 65536
  ),
  CONSTRAINT factory_run_budgets_version_key UNIQUE (run_id, run_version)
);

CREATE TABLE factory_lifecycle_checkpoints (
  id BYTEA PRIMARY KEY,
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  run_version BIGINT NOT NULL,
  lifecycle JSONB NOT NULL,
  cancellation_requested BOOLEAN NOT NULL,
  recorded_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT factory_lifecycle_checkpoints_id_shape CHECK (octet_length(id) = 32),
  CONSTRAINT factory_lifecycle_checkpoints_positive_version CHECK (run_version > 0),
  CONSTRAINT factory_lifecycle_checkpoints_document_shape CHECK (
    jsonb_typeof(lifecycle) = 'object' AND octet_length(lifecycle::text) BETWEEN 2 AND 262144
  ),
  CONSTRAINT factory_lifecycle_checkpoints_version_key UNIQUE (run_id, run_version)
);

CREATE TABLE factory_stage_attempts (
  id UUID PRIMARY KEY,
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  attempt_number BIGINT NOT NULL,
  stage_kind TEXT NOT NULL,
  target_digest BYTEA NOT NULL,
  input_digest BYTEA NOT NULL,
  stage_attempt JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT factory_stage_attempts_positive_number CHECK (attempt_number > 0),
  CONSTRAINT factory_stage_attempts_kind_shape CHECK (
    octet_length(stage_kind) BETWEEN 1 AND 128 AND stage_kind ~ '^[a-z0-9][a-z0-9._-]*$'
  ),
  CONSTRAINT factory_stage_attempts_target_digest_shape CHECK (octet_length(target_digest) = 32),
  CONSTRAINT factory_stage_attempts_input_digest_shape CHECK (octet_length(input_digest) = 32),
  CONSTRAINT factory_stage_attempts_document_shape CHECK (
    jsonb_typeof(stage_attempt) = 'object' AND octet_length(stage_attempt::text) BETWEEN 2 AND 262144
  ),
  CONSTRAINT factory_stage_attempts_number_key UNIQUE (run_id, attempt_number)
);

CREATE TABLE factory_stage_attempt_completions (
  id BYTEA PRIMARY KEY,
  stage_attempt_id UUID NOT NULL UNIQUE REFERENCES factory_stage_attempts(id),
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  outcome TEXT NOT NULL,
  output_digest BYTEA NOT NULL,
  completion JSONB NOT NULL,
  completed_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT factory_stage_attempt_completions_id_shape CHECK (octet_length(id) = 32),
  CONSTRAINT factory_stage_attempt_completions_outcome_shape CHECK (
    octet_length(outcome) BETWEEN 1 AND 128 AND outcome ~ '^[a-z0-9][a-z0-9._-]*$'
  ),
  CONSTRAINT factory_stage_attempt_completions_output_digest_shape CHECK (octet_length(output_digest) = 32),
  CONSTRAINT factory_stage_attempt_completions_document_shape CHECK (
    jsonb_typeof(completion) = 'object' AND octet_length(completion::text) BETWEEN 2 AND 262144
  )
);

CREATE TABLE factory_call_nodes (
  id UUID PRIMARY KEY,
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  stage_attempt_id UUID NOT NULL REFERENCES factory_stage_attempts(id),
  parent_call_id UUID REFERENCES factory_call_nodes(id),
  call_kind TEXT NOT NULL,
  context_digest BYTEA NOT NULL,
  call_node JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT factory_call_nodes_kind_shape CHECK (
    octet_length(call_kind) BETWEEN 1 AND 128 AND call_kind ~ '^[a-z0-9][a-z0-9._-]*$'
  ),
  CONSTRAINT factory_call_nodes_context_digest_shape CHECK (octet_length(context_digest) = 32),
  CONSTRAINT factory_call_nodes_document_shape CHECK (
    jsonb_typeof(call_node) = 'object' AND octet_length(call_node::text) BETWEEN 2 AND 262144
  )
);

CREATE TABLE factory_decision_signal_requests (
  id UUID PRIMARY KEY,
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  stage_attempt_id UUID REFERENCES factory_stage_attempts(id),
  call_node_id UUID REFERENCES factory_call_nodes(id),
  purpose TEXT NOT NULL,
  policy_digest BYTEA NOT NULL,
  input_digest BYTEA NOT NULL,
  request JSONB NOT NULL,
  requested_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT factory_decision_signal_requests_purpose_shape CHECK (
    octet_length(purpose) BETWEEN 1 AND 128 AND purpose ~ '^[a-z0-9][a-z0-9._-]*$'
  ),
  CONSTRAINT factory_decision_signal_requests_policy_digest_shape CHECK (octet_length(policy_digest) = 32),
  CONSTRAINT factory_decision_signal_requests_input_digest_shape CHECK (octet_length(input_digest) = 32),
  CONSTRAINT factory_decision_signal_requests_document_shape CHECK (
    jsonb_typeof(request) = 'object' AND octet_length(request::text) BETWEEN 2 AND 262144
  )
);

CREATE TABLE factory_decision_signal_receipts (
  id UUID PRIMARY KEY,
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  request_id UUID NOT NULL UNIQUE REFERENCES factory_decision_signal_requests(id),
  provider_digest BYTEA NOT NULL,
  model_digest BYTEA NOT NULL,
  policy_digest BYTEA NOT NULL,
  input_digest BYTEA NOT NULL,
  receipt_digest BYTEA NOT NULL UNIQUE,
  receipt JSONB NOT NULL,
  received_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT factory_decision_signal_receipts_provider_digest_shape CHECK (octet_length(provider_digest) = 32),
  CONSTRAINT factory_decision_signal_receipts_model_digest_shape CHECK (octet_length(model_digest) = 32),
  CONSTRAINT factory_decision_signal_receipts_policy_digest_shape CHECK (octet_length(policy_digest) = 32),
  CONSTRAINT factory_decision_signal_receipts_input_digest_shape CHECK (octet_length(input_digest) = 32),
  CONSTRAINT factory_decision_signal_receipts_receipt_digest_shape CHECK (octet_length(receipt_digest) = 32),
  CONSTRAINT factory_decision_signal_receipts_document_shape CHECK (
    jsonb_typeof(receipt) = 'object' AND octet_length(receipt::text) BETWEEN 2 AND 262144
  )
);

CREATE TABLE factory_build_links (
  build_id UUID PRIMARY KEY REFERENCES builds(id),
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  stage_attempt_id UUID NOT NULL UNIQUE REFERENCES factory_stage_attempts(id),
  attempt_id UUID NOT NULL REFERENCES attempts(id),
  factory_configuration_id UUID NOT NULL,
  factory_configuration_version BIGINT NOT NULL,
  build_configuration_id UUID NOT NULL,
  build_configuration_version BIGINT NOT NULL,
  task_envelope_digest BYTEA NOT NULL,
  effective_policy_digest BYTEA NOT NULL,
  input_digest BYTEA NOT NULL,
  exact_revision TEXT NOT NULL,
  link JSONB NOT NULL,
  linked_at TIMESTAMPTZ NOT NULL,
  FOREIGN KEY (factory_configuration_id, factory_configuration_version)
    REFERENCES factory_configuration_versions(factory_configuration_id, version),
  FOREIGN KEY (build_configuration_id, build_configuration_version)
    REFERENCES build_configuration_versions(build_configuration_id, version),
  CONSTRAINT factory_build_links_task_envelope_digest_shape CHECK (octet_length(task_envelope_digest) = 32),
  CONSTRAINT factory_build_links_effective_policy_digest_shape CHECK (octet_length(effective_policy_digest) = 32),
  CONSTRAINT factory_build_links_input_digest_shape CHECK (octet_length(input_digest) = 32),
  CONSTRAINT factory_build_links_exact_revision_shape CHECK (
    octet_length(exact_revision) BETWEEN 1 AND 512
    AND exact_revision = btrim(exact_revision)
    AND exact_revision !~ '[[:cntrl:]]'
  ),
  CONSTRAINT factory_build_links_document_shape CHECK (
    jsonb_typeof(link) = 'object' AND octet_length(link::text) BETWEEN 2 AND 262144
  )
);

CREATE TABLE factory_build_link_jobs (
  build_id UUID NOT NULL REFERENCES factory_build_links(build_id),
  job_id UUID NOT NULL REFERENCES jobs(id),
  ordinal SMALLINT NOT NULL,
  PRIMARY KEY (build_id, job_id),
  CONSTRAINT factory_build_link_jobs_positive_ordinal CHECK (ordinal >= 0),
  CONSTRAINT factory_build_link_jobs_ordinal_key UNIQUE (build_id, ordinal)
);

CREATE TABLE factory_build_observations (
  id BYTEA PRIMARY KEY,
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  build_id UUID NOT NULL REFERENCES factory_build_links(build_id),
  attempt_id UUID NOT NULL REFERENCES attempts(id),
  build_version BIGINT NOT NULL,
  attempt_version BIGINT NOT NULL,
  state TEXT NOT NULL,
  observation JSONB NOT NULL,
  observed_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT factory_build_observations_id_shape CHECK (octet_length(id) = 32),
  CONSTRAINT factory_build_observations_positive_versions CHECK (build_version > 0 AND attempt_version > 0),
  CONSTRAINT factory_build_observations_terminal_state CHECK (state IN ('succeeded', 'failed', 'cancelled')),
  CONSTRAINT factory_build_observations_document_shape CHECK (
    jsonb_typeof(observation) = 'object' AND octet_length(observation::text) BETWEEN 2 AND 1048576
  ),
  CONSTRAINT factory_build_observations_version_key UNIQUE (build_id, build_version, attempt_version)
);

CREATE TABLE factory_changesets (
  id UUID PRIMARY KEY,
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  stage_attempt_id UUID NOT NULL REFERENCES factory_stage_attempts(id),
  base_revision TEXT NOT NULL,
  candidate_revision TEXT NOT NULL,
  changeset_digest BYTEA NOT NULL UNIQUE,
  changeset JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT factory_changesets_revision_shape CHECK (
    octet_length(base_revision) BETWEEN 1 AND 512
    AND octet_length(candidate_revision) BETWEEN 1 AND 512
    AND base_revision = btrim(base_revision)
    AND candidate_revision = btrim(candidate_revision)
    AND base_revision !~ '[[:cntrl:]]'
    AND candidate_revision !~ '[[:cntrl:]]'
  ),
  CONSTRAINT factory_changesets_digest_shape CHECK (octet_length(changeset_digest) = 32),
  CONSTRAINT factory_changesets_document_shape CHECK (
    jsonb_typeof(changeset) = 'object' AND octet_length(changeset::text) BETWEEN 2 AND 262144
  )
);

CREATE TABLE factory_evidence_manifests (
  id UUID PRIMARY KEY,
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  changeset_id UUID NOT NULL REFERENCES factory_changesets(id),
  manifest_digest BYTEA NOT NULL UNIQUE,
  manifest JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT factory_evidence_manifests_digest_shape CHECK (octet_length(manifest_digest) = 32),
  CONSTRAINT factory_evidence_manifests_document_shape CHECK (
    jsonb_typeof(manifest) = 'object' AND octet_length(manifest::text) BETWEEN 2 AND 1048576
  )
);

CREATE TABLE factory_evaluation_plans (
  id UUID PRIMARY KEY,
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  changeset_id UUID NOT NULL REFERENCES factory_changesets(id),
  evidence_manifest_id UUID NOT NULL REFERENCES factory_evidence_manifests(id),
  plan_digest BYTEA NOT NULL UNIQUE,
  plan JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT factory_evaluation_plans_digest_shape CHECK (octet_length(plan_digest) = 32),
  CONSTRAINT factory_evaluation_plans_document_shape CHECK (
    jsonb_typeof(plan) = 'object' AND octet_length(plan::text) BETWEEN 2 AND 1048576
  )
);

CREATE TABLE factory_assessments (
  id UUID PRIMARY KEY,
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  evaluation_plan_id UUID NOT NULL REFERENCES factory_evaluation_plans(id),
  evaluator TEXT NOT NULL,
  assessment_digest BYTEA NOT NULL UNIQUE,
  assessment JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT factory_assessments_evaluator_shape CHECK (
    octet_length(evaluator) BETWEEN 1 AND 128 AND evaluator ~ '^[a-z0-9][a-z0-9._-]*$'
  ),
  CONSTRAINT factory_assessments_digest_shape CHECK (octet_length(assessment_digest) = 32),
  CONSTRAINT factory_assessments_document_shape CHECK (
    jsonb_typeof(assessment) = 'object' AND octet_length(assessment::text) BETWEEN 2 AND 1048576
  ),
  CONSTRAINT factory_assessments_evaluator_key UNIQUE (evaluation_plan_id, evaluator)
);

CREATE TABLE factory_decisions (
  id UUID PRIMARY KEY,
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  evaluation_plan_id UUID NOT NULL REFERENCES factory_evaluation_plans(id),
  outcome TEXT NOT NULL,
  input_digest BYTEA NOT NULL,
  policy_digest BYTEA NOT NULL,
  decision_digest BYTEA NOT NULL UNIQUE,
  decision JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT factory_decisions_outcome_known CHECK (
    outcome IN ('accept', 'rework', 'reject', 'escalate', 'cancel')
  ),
  CONSTRAINT factory_decisions_input_digest_shape CHECK (octet_length(input_digest) = 32),
  CONSTRAINT factory_decisions_policy_digest_shape CHECK (octet_length(policy_digest) = 32),
  CONSTRAINT factory_decisions_digest_shape CHECK (octet_length(decision_digest) = 32),
  CONSTRAINT factory_decisions_document_shape CHECK (
    jsonb_typeof(decision) = 'object' AND octet_length(decision::text) BETWEEN 2 AND 1048576
  )
);

CREATE TABLE factory_decision_assessments (
  decision_id UUID NOT NULL REFERENCES factory_decisions(id),
  assessment_id UUID NOT NULL REFERENCES factory_assessments(id),
  ordinal SMALLINT NOT NULL,
  PRIMARY KEY (decision_id, assessment_id),
  CONSTRAINT factory_decision_assessments_positive_ordinal CHECK (ordinal >= 0),
  CONSTRAINT factory_decision_assessments_ordinal_key UNIQUE (decision_id, ordinal)
);

CREATE TABLE factory_escalations (
  id UUID PRIMARY KEY,
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  decision_id UUID REFERENCES factory_decisions(id),
  reason TEXT NOT NULL,
  escalation JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT factory_escalations_reason_shape CHECK (
    octet_length(reason) BETWEEN 1 AND 4096 AND reason = btrim(reason) AND reason !~ '[[:cntrl:]]'
  ),
  CONSTRAINT factory_escalations_document_shape CHECK (
    jsonb_typeof(escalation) = 'object' AND octet_length(escalation::text) BETWEEN 2 AND 262144
  )
);

CREATE TABLE factory_delivery_attempts (
  id UUID PRIMARY KEY,
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  changeset_id UUID NOT NULL REFERENCES factory_changesets(id),
  decision_id UUID NOT NULL REFERENCES factory_decisions(id),
  attempt_number BIGINT NOT NULL,
  state TEXT NOT NULL,
  operation_digest BYTEA NOT NULL UNIQUE,
  attempt JSONB NOT NULL,
  recorded_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT factory_delivery_attempts_positive_number CHECK (attempt_number > 0),
  CONSTRAINT factory_delivery_attempts_state_known CHECK (state IN ('succeeded', 'failed', 'unknown')),
  CONSTRAINT factory_delivery_attempts_operation_digest_shape CHECK (octet_length(operation_digest) = 32),
  CONSTRAINT factory_delivery_attempts_document_shape CHECK (
    jsonb_typeof(attempt) = 'object' AND octet_length(attempt::text) BETWEEN 2 AND 262144
  ),
  CONSTRAINT factory_delivery_attempts_number_key UNIQUE (run_id, attempt_number)
);

CREATE TABLE factory_reporting_attempts (
  id UUID PRIMARY KEY,
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  attempt_number BIGINT NOT NULL,
  state TEXT NOT NULL,
  operation_digest BYTEA NOT NULL UNIQUE,
  attempt JSONB NOT NULL,
  recorded_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT factory_reporting_attempts_positive_number CHECK (attempt_number > 0),
  CONSTRAINT factory_reporting_attempts_state_known CHECK (state IN ('succeeded', 'failed', 'unknown')),
  CONSTRAINT factory_reporting_attempts_operation_digest_shape CHECK (octet_length(operation_digest) = 32),
  CONSTRAINT factory_reporting_attempts_document_shape CHECK (
    jsonb_typeof(attempt) = 'object' AND octet_length(attempt::text) BETWEEN 2 AND 262144
  ),
  CONSTRAINT factory_reporting_attempts_number_key UNIQUE (run_id, attempt_number)
);

CREATE TABLE factory_audit_links (
  id BYTEA PRIMARY KEY,
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  audit_fact_id UUID NOT NULL UNIQUE REFERENCES audit_facts(id) DEFERRABLE INITIALLY DEFERRED,
  actor_kind TEXT NOT NULL,
  actor_identity_digest BYTEA,
  operation TEXT NOT NULL,
  request_identity_digest BYTEA NOT NULL,
  outcome TEXT NOT NULL,
  recorded_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT factory_audit_links_id_shape CHECK (octet_length(id) = 32),
  CONSTRAINT factory_audit_links_actor_kind_shape CHECK (
    octet_length(actor_kind) BETWEEN 1 AND 128 AND actor_kind ~ '^[a-z0-9][a-z0-9._-]*$'
  ),
  CONSTRAINT factory_audit_links_actor_digest_shape CHECK (
    actor_identity_digest IS NULL OR octet_length(actor_identity_digest) = 32
  ),
  CONSTRAINT factory_audit_links_operation_shape CHECK (
    octet_length(operation) BETWEEN 1 AND 128 AND operation ~ '^[a-z0-9][a-z0-9._-]*$'
  ),
  CONSTRAINT factory_audit_links_request_digest_shape CHECK (octet_length(request_identity_digest) = 32),
  CONSTRAINT factory_audit_links_outcome_shape CHECK (
    octet_length(outcome) BETWEEN 1 AND 128 AND outcome ~ '^[a-z0-9][a-z0-9._-]*$'
  )
);

CREATE TABLE factory_run_controls (
  id BYTEA PRIMARY KEY,
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  expected_version BIGINT NOT NULL,
  control_kind TEXT NOT NULL,
  request_identity_digest BYTEA NOT NULL,
  control JSONB NOT NULL,
  recorded_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT factory_run_controls_id_shape CHECK (octet_length(id) = 32),
  CONSTRAINT factory_run_controls_positive_version CHECK (expected_version > 0),
  CONSTRAINT factory_run_controls_kind_shape CHECK (
    octet_length(control_kind) BETWEEN 1 AND 128 AND control_kind ~ '^[a-z0-9][a-z0-9._-]*$'
  ),
  CONSTRAINT factory_run_controls_request_digest_shape CHECK (octet_length(request_identity_digest) = 32),
  CONSTRAINT factory_run_controls_document_shape CHECK (
    jsonb_typeof(control) = 'object' AND octet_length(control::text) BETWEEN 2 AND 262144
  )
);

CREATE TABLE factory_outbox_records (
  id BYTEA PRIMARY KEY,
  operation_id BYTEA NOT NULL,
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  kind TEXT NOT NULL,
  input_digest BYTEA NOT NULL,
  state TEXT NOT NULL,
  attempt INTEGER NOT NULL,
  available_at TIMESTAMPTZ NOT NULL,
  recorded_at TIMESTAMPTZ NOT NULL,
  owner TEXT,
  fence BYTEA,
  claimed_at TIMESTAMPTZ,
  claim_expires_at TIMESTAMPTZ,
  CONSTRAINT factory_outbox_records_id_shape CHECK (octet_length(id) = 32),
  CONSTRAINT factory_outbox_records_operation_id_shape CHECK (octet_length(operation_id) = 32),
  CONSTRAINT factory_outbox_records_kind_shape CHECK (
    octet_length(kind) BETWEEN 1 AND 128 AND kind ~ '^[a-z0-9][a-z0-9._-]*$'
  ),
  CONSTRAINT factory_outbox_records_input_digest_shape CHECK (octet_length(input_digest) = 32),
  CONSTRAINT factory_outbox_records_state_known CHECK (state IN ('pending', 'claimed', 'delivered', 'failed')),
  CONSTRAINT factory_outbox_records_attempt_bound CHECK (attempt BETWEEN 0 AND 65535),
  CONSTRAINT factory_outbox_records_ownership_shape CHECK (
    (state = 'pending' AND owner IS NULL AND fence IS NULL AND claimed_at IS NULL AND claim_expires_at IS NULL)
    OR
    (state IN ('claimed', 'delivered', 'failed')
      AND owner IS NOT NULL
      AND octet_length(owner) BETWEEN 1 AND 128
      AND owner ~ '^[a-z0-9][a-z0-9._-]*$'
      AND octet_length(fence) = 32
      AND claimed_at IS NOT NULL
      AND claim_expires_at > claimed_at)
  ),
  CONSTRAINT factory_outbox_records_operation_state_key UNIQUE (operation_id, attempt, state)
);

-- This is the only mutable Factory execution row. Every pointer targets an
-- immutable history row; transition code advances all pointers and the Run
-- aggregate version atomically under a fence.
CREATE TABLE factory_run_current (
  run_id UUID PRIMARY KEY REFERENCES factory_runs(id),
  run_version BIGINT NOT NULL,
  claim_id BYTEA REFERENCES factory_run_claims(id),
  budget_id BYTEA NOT NULL REFERENCES factory_run_budgets(id),
  lifecycle_checkpoint_id BYTEA NOT NULL REFERENCES factory_lifecycle_checkpoints(id),
  stage_attempt_id UUID REFERENCES factory_stage_attempts(id),
  call_node_id UUID REFERENCES factory_call_nodes(id),
  signal_request_id UUID REFERENCES factory_decision_signal_requests(id),
  signal_receipt_id UUID REFERENCES factory_decision_signal_receipts(id),
  build_id UUID REFERENCES factory_build_links(build_id),
  changeset_id UUID REFERENCES factory_changesets(id),
  evidence_manifest_id UUID REFERENCES factory_evidence_manifests(id),
  evaluation_plan_id UUID REFERENCES factory_evaluation_plans(id),
  decision_id UUID REFERENCES factory_decisions(id),
  escalation_id UUID REFERENCES factory_escalations(id),
  delivery_attempt_id UUID REFERENCES factory_delivery_attempts(id),
  reporting_attempt_id UUID REFERENCES factory_reporting_attempts(id),
  CONSTRAINT factory_run_current_positive_version CHECK (run_version > 0)
);

-- Worker selection is bounded by authoritative state. Historical rows keep
-- operation and claim lookup efficient without introducing speculative
-- diagnostic indexes that later query work must justify.
CREATE INDEX factory_runs_reconciliation_idx
  ON factory_runs (updated_at, id)
  WHERE state IN (
    'admitted', 'implementing', 'validating', 'evaluating', 'reworking',
    'ready_for_delivery', 'delivering', 'escalated'
  );

CREATE INDEX factory_configurations_discovery_idx
  ON factory_configurations (project_id, created_at DESC, id DESC);

CREATE INDEX factory_runs_project_discovery_idx
  ON factory_runs (project_id, admitted_at DESC, id DESC);

CREATE INDEX factory_runs_configuration_discovery_idx
  ON factory_runs (factory_configuration_id, admitted_at DESC, id DESC);

CREATE INDEX factory_runs_state_discovery_idx
  ON factory_runs (state, admitted_at DESC, id DESC);

CREATE INDEX factory_work_envelopes_source_discovery_idx
  ON factory_work_envelopes (source_kind, admitted_at DESC, id DESC);

CREATE INDEX factory_run_claims_history_idx
  ON factory_run_claims (run_id, claimed_at DESC, id DESC);

-- Every diagnostics endpoint constrains one Run and advances by the immutable
-- row identity. These covering order prefixes keep pages bounded without
-- indexing JSONB payloads or speculative filters.
CREATE INDEX factory_stage_attempts_diagnostics_idx ON factory_stage_attempts (run_id, id);
CREATE INDEX factory_stage_attempt_completions_diagnostics_idx ON factory_stage_attempt_completions (run_id, id);
CREATE INDEX factory_call_nodes_diagnostics_idx ON factory_call_nodes (run_id, id);
CREATE INDEX factory_decision_signal_requests_diagnostics_idx ON factory_decision_signal_requests (run_id, id);
CREATE INDEX factory_decision_signal_receipts_diagnostics_idx ON factory_decision_signal_receipts (run_id, id);
CREATE INDEX factory_build_links_diagnostics_idx ON factory_build_links (run_id, build_id);
CREATE INDEX factory_build_observations_diagnostics_idx ON factory_build_observations (run_id, id);
CREATE INDEX factory_changesets_diagnostics_idx ON factory_changesets (run_id, id);
CREATE INDEX factory_evidence_manifests_diagnostics_idx ON factory_evidence_manifests (run_id, id);
CREATE INDEX factory_evaluation_plans_diagnostics_idx ON factory_evaluation_plans (run_id, id);
CREATE INDEX factory_assessments_diagnostics_idx ON factory_assessments (run_id, id);
CREATE INDEX factory_decisions_diagnostics_idx ON factory_decisions (run_id, id);
CREATE INDEX factory_escalations_diagnostics_idx ON factory_escalations (run_id, id);
CREATE INDEX factory_delivery_attempts_diagnostics_idx ON factory_delivery_attempts (run_id, id);
CREATE INDEX factory_reporting_attempts_diagnostics_idx ON factory_reporting_attempts (run_id, id);

CREATE INDEX factory_outbox_operation_history_idx
  ON factory_outbox_records (operation_id, recorded_at DESC, id DESC);

CREATE INDEX factory_outbox_due_idx
  ON factory_outbox_records (available_at, operation_id, recorded_at, id)
  WHERE state = 'pending';
