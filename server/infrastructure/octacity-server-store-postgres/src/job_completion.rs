use std::collections::BTreeSet;

use octacity_protocol::{AgentInventory, EXECUTION_CONTRACT_V2, ExecutionEvidenceV2};
use octacity_server_domain::{EntityKind, JobId, PoolId};
use octacity_server_job::{JobFailureClass, JobRuntimePolicy, JobSpecSigner, JobSpecTemplate};
use octacity_server_orchestrator::{AttemptState, BuildState};
use octacity_server_store::{
  AuditActorKind, CompletionDisposition, EventSequence, JobCompletion, JobCompletionKind, MutationDisposition,
  StoreError, StoreInputError, StoreOperation, complete_job_state,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, PgPool, Postgres, Transaction, types::Json};
use uuid::Uuid;

use crate::{
  database::{classify, job_ids, number, unavailable},
  lease,
  mutation::{
    AdditionalAuditFact, MutationFacts, MutationIdentity, MutationKind, MutationStart, NonManagementActor,
    append_additional_audit_fact, decode_outcome, encode_outcome,
  },
  state::{attempt_state, build_state, job_state, parse_attempt_state, parse_build_state, parse_job_state},
};

pub(crate) mod orchestration;

pub(crate) async fn execute(
  pool: &PgPool,
  signer: &JobSpecSigner,
  request: JobCompletion,
) -> Result<CompletionDisposition, StoreError> {
  execute_with_origin(pool, signer, request, CompletionOrigin::Agent).await
}

pub(crate) async fn fail_assignment(
  pool: &PgPool,
  signer: &JobSpecSigner,
  request: JobCompletion,
) -> Result<CompletionDisposition, StoreError> {
  request.validate_assignment_failure()?;
  execute_with_origin(pool, signer, request, CompletionOrigin::AssignmentDelivery).await
}

#[derive(Clone, Copy)]
enum CompletionOrigin {
  Agent,
  AssignmentDelivery,
}

async fn execute_with_origin(
  pool: &PgPool,
  signer: &JobSpecSigner,
  request: JobCompletion,
  origin: CompletionOrigin,
) -> Result<CompletionDisposition, StoreError> {
  let digest_input = json!({
    "agent_id": request.lease.agent_id,
    "final_sequence": request.final_sequence.map(EventSequence::get),
    "fence": request.lease.fence.expose(),
    "failure_class": failure_class(request.kind),
    "execution": request.execution,
    "kind": completion_kind(request.kind),
    "lease_id": request.lease.lease_id,
    "registration_epoch": request.lease.registration_epoch.get(),
  });
  let identity = MutationIdentity::new_non_management(
    match origin {
      CompletionOrigin::Agent => MutationKind::CompleteJob,
      CompletionOrigin::AssignmentDelivery => MutationKind::FailLeaseAssignment,
    },
    request.completion_id.to_string(),
    request.completed_at,
    EntityKind::Job,
    &digest_input,
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return replay(outcome),
  };
  let (attempt_id, build_id) = locate_graph(&mut transaction, request.lease.lease_id.as_uuid())
    .await?
    .ok_or(StoreError::Fenced {
      lease: request.lease.lease_id,
    })?;
  let attention_build_id = octacity_server_domain::BuildId::from_uuid(build_id).map_err(|_| StoreError::Unavailable)?;
  lock_build_and_attempt(&mut transaction, build_id, attempt_id).await?;
  let lease = lease::load(&mut transaction, request.lease, StoreOperation::CompleteJob).await?;
  if lease.attempt_id != attempt_id {
    return Err(StoreError::Unavailable);
  }
  require_valid_execution_evidence(&mut transaction, &lease, &request).await?;
  let required = request.final_sequence.map_or(0, EventSequence::get);
  let required_db = number(required, StoreOperation::CompleteJob)?;
  let kind = completion_kind(request.kind);

  let existing: Option<CompletionRow> = sqlx::query_as(
    "SELECT completion_id, lease_id, final_sequence, kind, failure_class, execution, ready_job_ids, skipped_job_ids, attempt_state, build_state \
     FROM job_completions WHERE job_id = $1 FOR UPDATE",
  )
  .bind(lease.job_id)
  .fetch_optional(&mut *transaction)
  .await
  .map_err(unavailable)?;
  if let Some(existing) = existing {
    if existing.matches(&request, required_db, kind) {
      let ready_jobs = job_ids(existing.ready_job_ids)?;
      let ready_pools = ready_pools(&mut transaction, &ready_jobs).await?;
      let outcome = CompletionDisposition {
        disposition: MutationDisposition::Replayed,
        job_id: JobId::from_uuid(lease.job_id).map_err(|_| StoreError::Unavailable)?,
        failure_class: parse_failure_class(existing.failure_class.as_deref())?,
        ready_jobs,
        ready_pools,
        skipped_jobs: job_ids(existing.skipped_job_ids)?,
        attempt_state: parse_attempt_state(&existing.attempt_state)?,
        build_state: parse_build_state(&existing.build_state)?,
      };
      // Assignment delivery and Agent completion intentionally use distinct
      // mutation namespaces. If one path already committed this completion,
      // the other path must replay the canonical Job outcome without
      // publishing a second idempotency, audit, or outbox record.
      return Ok(outcome);
    }
    return Err(StoreError::Conflict {
      entity: EntityKind::Job,
    });
  }

  lease::require_current(&lease, request.lease, request.completed_at)?;
  require_durable_cursor(&mut transaction, lease.job_id, required).await?;
  persist_terminal_state(&mut transaction, &request, lease.job_id, kind).await?;
  let applied = orchestration::reconcile_and_apply(
    &mut transaction,
    signer,
    lease.attempt_id,
    build_id,
    request.completed_at,
  )
  .await?;
  persist_completion(
    &mut transaction,
    CompletionWrite {
      request: &request,
      job_id: lease.job_id,
      final_sequence: required_db,
      kind,
      ready: &applied.ready_jobs,
      skipped: &applied.skipped_jobs,
      attempt_state: applied.attempt_state,
      build_state: applied.build_state,
    },
  )
  .await?;

  let ready_jobs = job_ids(applied.ready_jobs)?;
  let ready_pools = ready_pools(&mut transaction, &ready_jobs).await?;
  let outcome = CompletionDisposition {
    disposition: MutationDisposition::Applied,
    job_id: JobId::from_uuid(lease.job_id).map_err(|_| StoreError::Unavailable)?,
    failure_class: request.kind.failure_class(),
    ready_jobs,
    ready_pools,
    skipped_jobs: job_ids(applied.skipped_jobs)?,
    attempt_state: applied.attempt_state,
    build_state: applied.build_state,
  };
  append_orchestrator_fact(&mut transaction, &identity, attempt_id, &outcome).await?;
  crate::mutation::commit(
    transaction,
    &identity,
    facts(&request, &outcome, attention_build_id, origin),
    encode_outcome(&StoredOutcome::from(&outcome))?,
  )
  .await?;
  Ok(outcome)
}

#[derive(FromRow)]
struct CompletionExecutionContext {
  job_spec_template: Json<JobSpecTemplate>,
  inventory: Json<AgentInventory>,
  execution_contract_version: i16,
}

async fn require_valid_execution_evidence(
  transaction: &mut Transaction<'_, Postgres>,
  lease: &lease::LeaseRow,
  request: &JobCompletion,
) -> Result<(), StoreError> {
  let context: CompletionExecutionContext = sqlx::query_as(
    "SELECT job.job_spec_template, registration.inventory, registration.execution_contract_version \
     FROM jobs AS job \
     JOIN agent_registrations AS registration ON registration.id = $2 \
     WHERE job.id = $1",
  )
  .bind(lease.job_id)
  .bind(lease.registration_id)
  .fetch_one(&mut **transaction)
  .await
  .map_err(unavailable)?;
  let runtime = context.job_spec_template.0.placement_policy().runtime;
  if execution_evidence_is_valid(
    request.kind,
    runtime,
    &context.inventory.0,
    context.execution_contract_version,
    request.execution.as_ref(),
  ) {
    Ok(())
  } else {
    Err(StoreError::InvalidInput {
      operation: StoreOperation::CompleteJob,
      source: StoreInputError::InvalidExecutionEvidence,
    })
  }
}

fn execution_evidence_is_valid(
  kind: JobCompletionKind,
  runtime: &JobRuntimePolicy,
  inventory: &AgentInventory,
  execution_contract_version: i16,
  evidence: Option<&ExecutionEvidenceV2>,
) -> bool {
  match (runtime, evidence) {
    (JobRuntimePolicy::Legacy(_), None) => true,
    (JobRuntimePolicy::Legacy(_), Some(_)) => false,
    (JobRuntimePolicy::Current(_), None) => kind != JobCompletionKind::Succeeded,
    (JobRuntimePolicy::Current(runtime), Some(evidence)) => {
      execution_contract_version >= i16::try_from(EXECUTION_CONTRACT_V2).expect("v2 fits PostgreSQL SMALLINT")
        && evidence.target == runtime.target
        && inventory
          .executions
          .iter()
          .any(|capability| capability.provider == evidence.provider && capability.satisfies(&evidence.target))
    }
  }
}

async fn append_orchestrator_fact(
  transaction: &mut Transaction<'_, Postgres>,
  identity: &MutationIdentity,
  attempt_id: Uuid,
  outcome: &CompletionDisposition,
) -> Result<(), StoreError> {
  append_additional_audit_fact(
    transaction,
    identity,
    AdditionalAuditFact {
      actor_kind: AuditActorKind::Orchestrator,
      actor_identity: None,
      operation: "reconcile-job-completion",
      target_kind: "attempt",
      target_identity: attempt_id.to_string(),
      safe_metadata: json!({
        "job_id": outcome.job_id,
        "ready_job_count": outcome.ready_jobs.len(),
        "skipped_job_count": outcome.skipped_jobs.len(),
        "attempt_state": attempt_state(outcome.attempt_state),
        "build_state": build_state(outcome.build_state),
      }),
    },
  )
  .await
}

async fn require_durable_cursor(
  transaction: &mut Transaction<'_, Postgres>,
  job_id: Uuid,
  required: u64,
) -> Result<(), StoreError> {
  let durable_through: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(sequence), 0) FROM job_events WHERE job_id = $1")
    .bind(job_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(unavailable)?;
  let durable_through = u64::try_from(durable_through).map_err(|_| StoreError::Unavailable)?;
  if required > durable_through {
    return Err(StoreError::EventsMissing {
      job: JobId::from_uuid(job_id).map_err(|_| StoreError::Unavailable)?,
      durable_through,
      required,
    });
  }
  if required < durable_through {
    return Err(StoreError::Conflict {
      entity: EntityKind::Job,
    });
  }
  Ok(())
}

async fn persist_terminal_state(
  transaction: &mut Transaction<'_, Postgres>,
  request: &JobCompletion,
  job_id: Uuid,
  kind: &str,
) -> Result<(), StoreError> {
  let current: String = sqlx::query_scalar("SELECT state FROM jobs WHERE id = $1 FOR UPDATE")
    .bind(job_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(unavailable)?;
  let terminal = complete_job_state(parse_job_state(&current)?, request.kind)?;
  if job_state(terminal) != kind {
    return Err(StoreError::Unavailable);
  }
  sqlx::query(
    "UPDATE leases SET state = 'completed', version = version + 1, \
       completed_at = to_timestamp($1::double precision / 1000.0) WHERE id = $2",
  )
  .bind(request.completed_at.unix_millis())
  .bind(request.lease.lease_id.as_uuid())
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Lease))?;
  sqlx::query(
    "UPDATE jobs SET state = $1, version = version + 1, \
       updated_at = to_timestamp($2::double precision / 1000.0) WHERE id = $3",
  )
  .bind(job_state(terminal))
  .bind(request.completed_at.unix_millis())
  .bind(job_id)
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Job))?;
  Ok(())
}

async fn locate_graph(
  transaction: &mut Transaction<'_, Postgres>,
  lease_id: Uuid,
) -> Result<Option<(Uuid, Uuid)>, StoreError> {
  sqlx::query_as(
    "SELECT job.attempt_id, attempt.build_id FROM leases AS lease \
     JOIN jobs AS job ON job.id = lease.job_id \
     JOIN attempts AS attempt ON attempt.id = job.attempt_id \
     WHERE lease.id = $1",
  )
  .bind(lease_id)
  .fetch_optional(&mut **transaction)
  .await
  .map_err(unavailable)
}

async fn lock_build_and_attempt(
  transaction: &mut Transaction<'_, Postgres>,
  build_id: Uuid,
  attempt_id: Uuid,
) -> Result<(), StoreError> {
  sqlx::query_scalar::<_, Uuid>("SELECT id FROM builds WHERE id = $1 FOR UPDATE")
    .bind(build_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(unavailable)?;
  let locked_build: Uuid = sqlx::query_scalar("SELECT build_id FROM attempts WHERE id = $1 FOR UPDATE")
    .bind(attempt_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(unavailable)?;
  if locked_build != build_id {
    return Err(StoreError::Unavailable);
  }
  Ok(())
}

struct CompletionWrite<'a> {
  request: &'a JobCompletion,
  job_id: Uuid,
  final_sequence: i64,
  kind: &'a str,
  ready: &'a [Uuid],
  skipped: &'a [Uuid],
  attempt_state: AttemptState,
  build_state: BuildState,
}

async fn persist_completion(
  transaction: &mut Transaction<'_, Postgres>,
  write: CompletionWrite<'_>,
) -> Result<(), StoreError> {
  sqlx::query(
    "INSERT INTO job_completions \
       (job_id, completion_id, lease_id, final_sequence, kind, failure_class, execution, ready_job_ids, skipped_job_ids, \
        attempt_state, build_state, completed_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, to_timestamp($12::double precision / 1000.0))",
  )
  .bind(write.job_id)
  .bind(write.request.completion_id.as_str())
  .bind(write.request.lease.lease_id.as_uuid())
  .bind(write.final_sequence)
  .bind(write.kind)
  .bind(failure_class(write.request.kind))
  .bind(write.request.execution.as_ref().map(Json))
  .bind(write.ready)
  .bind(write.skipped)
  .bind(attempt_state(write.attempt_state))
  .bind(build_state(write.build_state))
  .bind(write.request.completed_at.unix_millis())
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Job))?;
  Ok(())
}

#[derive(FromRow)]
struct CompletionRow {
  completion_id: String,
  lease_id: Uuid,
  final_sequence: i64,
  kind: String,
  failure_class: Option<String>,
  execution: Option<Json<ExecutionEvidenceV2>>,
  ready_job_ids: Vec<Uuid>,
  skipped_job_ids: Vec<Uuid>,
  attempt_state: String,
  build_state: String,
}

impl CompletionRow {
  fn matches(&self, request: &JobCompletion, final_sequence: i64, kind: &str) -> bool {
    self.completion_id == request.completion_id.as_str()
      && self.lease_id == request.lease.lease_id.as_uuid()
      && self.final_sequence == final_sequence
      && self.kind == kind
      && self.failure_class.as_deref() == failure_class(request.kind)
      && self.execution.as_ref().map(|execution| &execution.0) == request.execution.as_ref()
  }
}

#[derive(Deserialize, Serialize)]
struct StoredOutcome {
  job_id: JobId,
  failure_class: Option<JobFailureClass>,
  ready_jobs: Vec<JobId>,
  #[serde(default)]
  ready_pools: BTreeSet<PoolId>,
  skipped_jobs: Vec<JobId>,
  attempt_state: AttemptState,
  build_state: BuildState,
}

impl From<&CompletionDisposition> for StoredOutcome {
  fn from(outcome: &CompletionDisposition) -> Self {
    Self {
      job_id: outcome.job_id,
      failure_class: outcome.failure_class,
      ready_jobs: outcome.ready_jobs.clone(),
      ready_pools: outcome.ready_pools.clone(),
      skipped_jobs: outcome.skipped_jobs.clone(),
      attempt_state: outcome.attempt_state,
      build_state: outcome.build_state,
    }
  }
}

fn replay(value: Value) -> Result<CompletionDisposition, StoreError> {
  let stored: StoredOutcome = decode_outcome(value)?;
  Ok(CompletionDisposition {
    disposition: MutationDisposition::Replayed,
    job_id: stored.job_id,
    failure_class: stored.failure_class,
    ready_jobs: stored.ready_jobs,
    ready_pools: stored.ready_pools,
    skipped_jobs: stored.skipped_jobs,
    attempt_state: stored.attempt_state,
    build_state: stored.build_state,
  })
}

async fn ready_pools(
  transaction: &mut Transaction<'_, Postgres>,
  ready_jobs: &[JobId],
) -> Result<BTreeSet<PoolId>, StoreError> {
  if ready_jobs.is_empty() {
    return Ok(BTreeSet::new());
  }
  let job_ids: Vec<_> = ready_jobs.iter().map(|job_id| job_id.as_uuid()).collect();
  let rows: Vec<Uuid> = sqlx::query_scalar(
    "SELECT DISTINCT unnest(allowed_pool_ids) AS pool_id FROM jobs WHERE id = ANY($1::uuid[]) ORDER BY pool_id",
  )
  .bind(job_ids)
  .fetch_all(&mut **transaction)
  .await
  .map_err(unavailable)?;
  rows
    .into_iter()
    .map(|pool_id| PoolId::from_uuid(pool_id).map_err(|_| StoreError::Unavailable))
    .collect()
}

fn facts(
  request: &JobCompletion,
  outcome: &CompletionDisposition,
  build_id: octacity_server_domain::BuildId,
  origin: CompletionOrigin,
) -> MutationFacts {
  let facts = MutationFacts::non_management(
    match origin {
      CompletionOrigin::Agent => NonManagementActor::Agent(request.lease.agent_id.to_string()),
      CompletionOrigin::AssignmentDelivery => NonManagementActor::Worker("lease-assignment-delivery".to_owned()),
    },
    outcome.job_id.to_string(),
    json!({
      "final_sequence": request.final_sequence.map(EventSequence::get),
      "kind": completion_kind(request.kind),
      "failure_class": failure_class(request.kind),
      "execution": request.execution,
      "lease_id": request.lease.lease_id,
      "completion_id": request.completion_id,
      "ready_job_count": outcome.ready_jobs.len(),
      "skipped_job_count": outcome.skipped_jobs.len(),
      "attempt_state": attempt_state(outcome.attempt_state),
      "build_state": build_state(outcome.build_state),
    }),
    json!({
      "job_id": outcome.job_id,
      "kind": completion_kind(request.kind),
      "failure_class": failure_class(request.kind),
      "execution": request.execution,
      "ready_jobs": outcome.ready_jobs,
      "skipped_jobs": outcome.skipped_jobs,
      "attempt_state": attempt_state(outcome.attempt_state),
      "build_state": build_state(outcome.build_state),
      "schema_version": 2,
    }),
  );
  if outcome.disposition == MutationDisposition::Applied && outcome.build_state == BuildState::Failed {
    facts.with_attention(crate::operator_attention::TargetAttentionChange::build_failed(build_id))
  } else {
    facts
  }
}

const fn completion_kind(kind: JobCompletionKind) -> &'static str {
  match kind {
    JobCompletionKind::Succeeded => "succeeded",
    JobCompletionKind::Failed(_) => "failed",
    JobCompletionKind::TimedOut => "timed_out",
    JobCompletionKind::Cancelled => "cancelled",
  }
}

const fn failure_class(kind: JobCompletionKind) -> Option<&'static str> {
  match kind {
    JobCompletionKind::Failed(JobFailureClass::Execution) => Some("execution"),
    JobCompletionKind::Failed(JobFailureClass::Infrastructure) => Some("infrastructure"),
    JobCompletionKind::TimedOut => Some("execution"),
    JobCompletionKind::Succeeded | JobCompletionKind::Cancelled => None,
  }
}

fn parse_failure_class(value: Option<&str>) -> Result<Option<JobFailureClass>, StoreError> {
  match value {
    None => Ok(None),
    Some("execution") => Ok(Some(JobFailureClass::Execution)),
    Some("infrastructure") => Ok(Some(JobFailureClass::Infrastructure)),
    Some(_) => Err(StoreError::Unavailable),
  }
}

#[cfg(test)]
mod tests {
  use std::collections::BTreeSet;

  use octacity_protocol::{
    ExecutionCapabilityV2, ExecutionContractRange, ExecutionMode, ExecutionProviderId, ExecutionTargetV2,
    NetworkPolicy, PlatformArchitecture, PlatformOs, PlatformSpec, RuntimeSpecV2, guarantees_for,
  };
  use octacity_server_domain::AgentId;

  use super::*;

  #[test]
  fn completion_evidence_must_match_signed_intent_and_registered_provider() {
    let platform = PlatformSpec {
      os: PlatformOs::Linux,
      architecture: PlatformArchitecture::Amd64,
    };
    let provider = ExecutionProviderId::new("host").unwrap();
    let target = ExecutionTargetV2 {
      mode: ExecutionMode::Host,
      host_platform: platform,
      target_platform: platform,
      required_guarantees: guarantees_for(ExecutionMode::Host),
      immutable_image: None,
    };
    let runtime = JobRuntimePolicy::Current(RuntimeSpecV2 {
      target: target.clone(),
      cpu_millis: 1,
      memory_bytes: 1,
      writable_disk_bytes: 1,
      timeout_seconds: 1,
      network: NetworkPolicy::Unrestricted,
      workload_identity_profile: None,
    });
    let evidence = ExecutionEvidenceV2 {
      provider: provider.clone(),
      target,
    };
    let agent_id = AgentId::from_uuid(uuid::Uuid::from_u128(1)).unwrap();
    let mut inventory = octacity_server_store::testing::compatible_inventory(agent_id);
    inventory.execution_contract = ExecutionContractRange { min: 1, max: 2 };
    inventory.executions = vec![ExecutionCapabilityV2 {
      provider,
      mode: ExecutionMode::Host,
      host_platform: platform,
      target_platform: platform,
      guarantees: BTreeSet::new(),
      immutable_images: false,
    }];

    assert!(execution_evidence_is_valid(
      JobCompletionKind::Succeeded,
      &runtime,
      &inventory,
      2,
      Some(&evidence),
    ));
    assert!(!execution_evidence_is_valid(
      JobCompletionKind::Succeeded,
      &runtime,
      &inventory,
      2,
      None,
    ));
    assert!(execution_evidence_is_valid(
      JobCompletionKind::Failed(JobFailureClass::Infrastructure),
      &runtime,
      &inventory,
      2,
      None,
    ));
    inventory.executions.clear();
    assert!(!execution_evidence_is_valid(
      JobCompletionKind::Succeeded,
      &runtime,
      &inventory,
      2,
      Some(&evidence),
    ));
  }
}
