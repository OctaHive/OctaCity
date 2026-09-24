use octacity_server_domain::{EntityKind, Timestamp};
use octacity_server_store::StoreError;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use sqlx::{PgPool, Postgres, Transaction, types::Json};
use uuid::Uuid;

const PENDING_OUTCOME: &str = "pending";
const STORED_OUTCOME_SCHEMA_VERSION: u16 = 1;

pub(crate) struct MutationIdentity {
  pub(crate) kind: MutationKind,
  pub(crate) key: String,
  pub(crate) request_digest: [u8; 32],
  pub(crate) occurred_at: Timestamp,
  pub(crate) conflict_entity: EntityKind,
  pub(crate) request_identity: String,
}

impl MutationIdentity {
  pub(crate) fn new<T: Serialize>(
    kind: MutationKind,
    key: String,
    occurred_at: Timestamp,
    conflict_entity: EntityKind,
    request: &T,
  ) -> Result<Self, StoreError> {
    let encoded = serde_json::to_vec(request).map_err(|_| StoreError::Unavailable)?;
    let mut digest = Sha256::new();
    digest.update(kind.digest_version());
    digest.update(encoded);
    let request_identity = format!("{}:{}", kind.scope(), key);
    Ok(Self {
      kind,
      key,
      request_digest: digest.finalize().into(),
      occurred_at,
      conflict_entity,
      request_identity,
    })
  }

  pub(crate) fn new_with_request_identity<T: Serialize>(
    kind: MutationKind,
    key: String,
    request_identity: String,
    occurred_at: Timestamp,
    conflict_entity: EntityKind,
    request: &T,
  ) -> Result<Self, StoreError> {
    let mut identity = Self::new(kind, key, occurred_at, conflict_entity, request)?;
    identity.request_identity = request_identity;
    Ok(identity)
  }

  pub(crate) fn with_digest(
    kind: MutationKind,
    key: String,
    occurred_at: Timestamp,
    conflict_entity: EntityKind,
    request_digest: [u8; 32],
  ) -> Self {
    let request_identity = format!("{}:{}", kind.scope(), key);
    Self {
      kind,
      key,
      request_digest,
      occurred_at,
      conflict_entity,
      request_identity,
    }
  }
}

pub(crate) struct MutationFacts {
  pub(crate) actor_kind: &'static str,
  pub(crate) actor_identity: Option<String>,
  pub(crate) target_identity: String,
  pub(crate) safe_metadata: Value,
  pub(crate) outbox_payload: Value,
}

#[derive(Clone, Copy)]
pub(crate) enum MutationKind {
  CreateProject,
  RenameProject,
  MoveProject,
  DeleteProject,
  PublishProjectPolicy,
  CreateAgentPool,
  PublishAgentPoolVersion,
  DeleteAgentPool,
  ReassignAgentPool,
  DrainAgent,
  CreatePipeline,
  PublishPipelineVersion,
  CreateRepository,
  PublishRepositoryVersion,
  CreateBuildConfiguration,
  PublishBuildConfigurationVersion,
  CreateTriggerDefinition,
  PublishInternalTriggerVersion,
  CreateUnmanagedWebhook,
  CreateManagedWebhook,
  CompleteManagedWebhookCreate,
  ObserveManagedWebhook,
  RotateManagedWebhook,
  DeleteManagedWebhook,
  CreateSchedule,
  AcceptTrigger,
  SuppressTrigger,
  ClaimReadyJob,
  RenewLease,
  RecoverExpiredLease,
  AppendJobEvents,
  CompleteJob,
  CancelBuild,
  RetryBuild,
  IssueAgentEnrollment,
  RegisterAgent,
  RevokeAgentEnrollment,
  RevokeAgentRegistration,
  PlaceBuildResultHold,
  ReleaseBuildResultHold,
}

impl MutationKind {
  const fn metadata(self) -> MutationMetadata {
    macro_rules! metadata {
      ($digest:literal, $scope:literal, $target:literal, $topic:literal) => {
        MutationMetadata {
          digest_version: $digest,
          scope: $scope,
          target_kind: $target,
          outbox_topic: $topic,
        }
      };
    }
    match self {
      Self::CreateProject => metadata!(
        b"octacity.create-project.v1\0",
        "create-project",
        "project",
        "project.created"
      ),
      Self::RenameProject => metadata!(
        b"octacity.rename-project.v1\0",
        "rename-project",
        "project",
        "project.renamed"
      ),
      Self::MoveProject => metadata!(
        b"octacity.move-project.v1\0",
        "move-project",
        "project",
        "project.moved"
      ),
      Self::DeleteProject => metadata!(
        b"octacity.delete-project.v1\0",
        "delete-project",
        "project",
        "project.deleted"
      ),
      Self::PublishProjectPolicy => metadata!(
        b"octacity.publish-project-policy.v1\0",
        "publish-project-policy",
        "project_policy",
        "project-policy.version-published"
      ),
      Self::CreateAgentPool => metadata!(
        b"octacity.create-agent-pool.v1\0",
        "create-agent-pool",
        "pool",
        "agent-pool.created"
      ),
      Self::PublishAgentPoolVersion => metadata!(
        b"octacity.publish-agent-pool-version.v1\0",
        "publish-agent-pool-version",
        "pool",
        "agent-pool.version-published"
      ),
      Self::DeleteAgentPool => metadata!(
        b"octacity.delete-agent-pool.v1\0",
        "delete-agent-pool",
        "pool",
        "agent-pool.deleted"
      ),
      Self::ReassignAgentPool => metadata!(
        b"octacity.reassign-agent-pool.v1\0",
        "reassign-agent-pool",
        "agent",
        "agent.pool-reassigned"
      ),
      Self::DrainAgent => metadata!(
        b"octacity.drain-agent.v1\0",
        "drain-agent",
        "agent",
        "agent.drain-requested"
      ),
      Self::CreatePipeline => metadata!(
        b"octacity.create-pipeline.v1\0",
        "create-pipeline",
        "pipeline",
        "pipeline.created"
      ),
      Self::PublishPipelineVersion => metadata!(
        b"octacity.publish-pipeline-version.v1\0",
        "publish-pipeline-version",
        "pipeline",
        "pipeline.version-published"
      ),
      Self::CreateRepository => metadata!(
        b"octacity.create-repository.v1\0",
        "create-repository",
        "repository",
        "repository.created"
      ),
      Self::PublishRepositoryVersion => metadata!(
        b"octacity.publish-repository-version.v1\0",
        "publish-repository-version",
        "repository",
        "repository.version-published"
      ),
      Self::CreateBuildConfiguration => metadata!(
        b"octacity.create-build-configuration.v1\0",
        "create-build-configuration",
        "build_configuration",
        "build-configuration.created"
      ),
      Self::PublishBuildConfigurationVersion => metadata!(
        b"octacity.publish-build-configuration-version.v1\0",
        "publish-build-configuration-version",
        "build_configuration",
        "build-configuration.version-published"
      ),
      Self::CreateTriggerDefinition => metadata!(
        b"octacity.create-trigger-definition.v1\0",
        "create-trigger-definition",
        "trigger",
        "trigger.created"
      ),
      Self::PublishInternalTriggerVersion => metadata!(
        b"octacity.publish-internal-trigger-version.v1\0",
        "publish-internal-trigger-version",
        "trigger",
        "internal-trigger.version-published"
      ),
      Self::CreateUnmanagedWebhook => metadata!(
        b"octacity.create-unmanaged-webhook.v1\0",
        "create-unmanaged-webhook",
        "integration",
        "webhook-integration.created"
      ),
      Self::CreateManagedWebhook => metadata!(
        b"octacity.create-managed-webhook.v1\0",
        "create-managed-webhook",
        "integration",
        "webhook-integration.managed-created"
      ),
      Self::CompleteManagedWebhookCreate => metadata!(
        b"octacity.complete-managed-webhook-create.v1\0",
        "complete-managed-webhook-create",
        "integration",
        "webhook-integration.registration-created"
      ),
      Self::ObserveManagedWebhook => metadata!(
        b"octacity.observe-managed-webhook.v1\0",
        "observe-managed-webhook",
        "integration",
        "webhook-integration.registration-observed"
      ),
      Self::RotateManagedWebhook => metadata!(
        b"octacity.rotate-managed-webhook.v1\0",
        "rotate-managed-webhook",
        "integration",
        "webhook-integration.registration-rotated"
      ),
      Self::DeleteManagedWebhook => metadata!(
        b"octacity.delete-managed-webhook.v1\0",
        "delete-managed-webhook",
        "integration",
        "webhook-integration.registration-deleted"
      ),
      Self::CreateSchedule => metadata!(
        b"octacity.create-schedule.v1\0",
        "create-schedule",
        "trigger",
        "schedule.created"
      ),
      Self::AcceptTrigger => metadata!(
        b"octacity.accept-trigger.v2\0",
        "accept-trigger",
        "build",
        "build.accepted"
      ),
      Self::SuppressTrigger => metadata!(
        b"octacity.suppress-trigger.v1\0",
        "suppress-trigger",
        "trigger",
        "trigger.suppressed"
      ),
      Self::ClaimReadyJob => metadata!(
        b"octacity.claim-ready-job.v2\0",
        "claim-ready-job",
        "job",
        "job.claimed"
      ),
      Self::RenewLease => metadata!(b"octacity.renew-lease.v1\0", "renew-lease", "lease", "lease.heartbeat"),
      Self::RecoverExpiredLease => metadata!(
        b"octacity.recover-expired-lease.v1\0",
        "recover-expired-lease",
        "lease",
        "lease.expired-recovered"
      ),
      Self::AppendJobEvents => metadata!(
        b"octacity.append-job-events.v2\0",
        "append-job-events",
        "job",
        "job.events-appended"
      ),
      Self::CompleteJob => metadata!(b"octacity.complete-job.v1\0", "complete-job", "job", "job.completed"),
      Self::CancelBuild => metadata!(
        b"octacity.cancel-build.v1\0",
        "cancel-build",
        "build",
        "build.cancellation-requested"
      ),
      Self::RetryBuild => metadata!(b"octacity.retry-build.v1\0", "retry-build", "build", "build.retried"),
      Self::IssueAgentEnrollment => metadata!(
        b"octacity.issue-agent-enrollment.v2\0",
        "issue-agent-enrollment",
        "agent_enrollment_credential",
        "agent.enrollment-issued"
      ),
      Self::RegisterAgent => metadata!(
        b"octacity.register-agent.v2\0",
        "register-agent",
        "agent_registration",
        "agent.registered"
      ),
      Self::RevokeAgentEnrollment => metadata!(
        b"octacity.revoke-agent-enrollment.v1\0",
        "revoke-agent-enrollment",
        "agent_enrollment_credential",
        "agent.enrollment-revoked"
      ),
      Self::RevokeAgentRegistration => metadata!(
        b"octacity.revoke-agent-registration.v1\0",
        "revoke-agent-registration",
        "agent_registration",
        "agent.registration-revoked"
      ),
      Self::PlaceBuildResultHold => metadata!(
        b"octacity.place-build-result-hold.v1\0",
        "place-build-result-hold",
        "retention_hold",
        "build-result.retention-hold-placed"
      ),
      Self::ReleaseBuildResultHold => metadata!(
        b"octacity.release-build-result-hold.v1\0",
        "release-build-result-hold",
        "retention_hold",
        "build-result.retention-hold-released"
      ),
    }
  }

  const fn digest_version(self) -> &'static [u8] {
    self.metadata().digest_version
  }

  pub(crate) const fn scope(self) -> &'static str {
    self.metadata().scope
  }

  const fn target_kind(self) -> &'static str {
    self.metadata().target_kind
  }

  pub(crate) const fn outbox_topic(self) -> &'static str {
    self.metadata().outbox_topic
  }
}

struct MutationMetadata {
  digest_version: &'static [u8],
  scope: &'static str,
  target_kind: &'static str,
  outbox_topic: &'static str,
}

#[derive(Deserialize, Serialize)]
struct StoredOutcome<T> {
  schema_version: u16,
  value: T,
}

pub(crate) fn encode_outcome<T: Serialize>(outcome: &T) -> Result<Value, StoreError> {
  serde_json::to_value(StoredOutcome {
    schema_version: STORED_OUTCOME_SCHEMA_VERSION,
    value: outcome,
  })
  .map_err(|_| StoreError::Unavailable)
}

pub(crate) fn decode_outcome<T: DeserializeOwned>(outcome: Value) -> Result<T, StoreError> {
  let stored: StoredOutcome<T> = serde_json::from_value(outcome).map_err(|_| StoreError::Unavailable)?;
  if stored.schema_version != STORED_OUTCOME_SCHEMA_VERSION {
    return Err(StoreError::Unavailable);
  }
  Ok(stored.value)
}

pub(crate) enum MutationStart<'a> {
  Fresh(Transaction<'a, Postgres>),
  Replay(Value),
}

pub(crate) async fn begin<'a>(pool: &'a PgPool, identity: &MutationIdentity) -> Result<MutationStart<'a>, StoreError> {
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let inserted = sqlx::query(
    "INSERT INTO idempotency_records \
       (scope, idempotency_key, request_digest, outcome, created_at) \
     VALUES ($1, $2, $3, $4, to_timestamp($5::double precision / 1000.0)) \
     ON CONFLICT DO NOTHING",
  )
  .bind(identity.kind.scope())
  .bind(&identity.key)
  .bind(identity.request_digest.as_slice())
  .bind(Json(json!({"status": PENDING_OUTCOME})))
  .bind(identity.occurred_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(unavailable)?;
  if inserted.rows_affected() == 1 {
    return Ok(MutationStart::Fresh(transaction));
  }

  let (request_digest, Json(outcome)): (Vec<u8>, Json<Value>) = sqlx::query_as(
    "SELECT request_digest, outcome FROM idempotency_records \
     WHERE scope = $1 AND idempotency_key = $2 FOR UPDATE",
  )
  .bind(identity.kind.scope())
  .bind(&identity.key)
  .fetch_one(&mut *transaction)
  .await
  .map_err(unavailable)?;
  if request_digest != identity.request_digest {
    return Err(StoreError::Conflict {
      entity: identity.conflict_entity,
    });
  }
  if outcome.get("status").and_then(Value::as_str) == Some(PENDING_OUTCOME) {
    return Err(StoreError::Unavailable);
  }
  transaction.commit().await.map_err(unavailable)?;
  Ok(MutationStart::Replay(outcome))
}

pub(crate) async fn commit(
  mut transaction: Transaction<'_, Postgres>,
  identity: &MutationIdentity,
  facts: MutationFacts,
  outcome: Value,
) -> Result<(), StoreError> {
  sqlx::query(
    "INSERT INTO audit_facts \
       (id, actor_kind, actor_identity, operation, target_kind, target_identity, request_identity, \
        idempotency_key, outcome, safe_metadata, occurred_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'accepted', $9, \
             to_timestamp($10::double precision / 1000.0))",
  )
  .bind(stable_record_id("audit", identity))
  .bind(facts.actor_kind)
  .bind(facts.actor_identity)
  .bind(identity.kind.scope())
  .bind(identity.kind.target_kind())
  .bind(&facts.target_identity)
  .bind(&identity.request_identity)
  .bind(&identity.key)
  .bind(Json(facts.safe_metadata))
  .bind(identity.occurred_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(unavailable)?;

  sqlx::query(
    "INSERT INTO outbox_entries \
       (id, topic, aggregate_kind, aggregate_identity, payload, created_at, available_at, attempt_count) \
     VALUES ($1, $2, $3, $4, $5, to_timestamp($6::double precision / 1000.0), \
             to_timestamp($6::double precision / 1000.0), 0)",
  )
  .bind(stable_record_id("outbox", identity))
  .bind(identity.kind.outbox_topic())
  .bind(identity.kind.target_kind())
  .bind(&facts.target_identity)
  .bind(Json(facts.outbox_payload))
  .bind(identity.occurred_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(unavailable)?;

  let updated = sqlx::query("UPDATE idempotency_records SET outcome = $1 WHERE scope = $2 AND idempotency_key = $3")
    .bind(Json(outcome))
    .bind(identity.kind.scope())
    .bind(&identity.key)
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?;
  if updated.rows_affected() != 1 {
    return Err(StoreError::Unavailable);
  }
  transaction.commit().await.map_err(unavailable)
}

fn stable_record_id(record_kind: &str, identity: &MutationIdentity) -> Uuid {
  let mut digest = Sha256::new();
  digest.update(b"octacity.store-mutation-record.v1\0");
  digest.update(record_kind.as_bytes());
  digest.update([0]);
  digest.update(identity.kind.scope().as_bytes());
  digest.update([0]);
  digest.update(identity.key.as_bytes());
  let mut bytes: [u8; 16] = digest.finalize()[..16]
    .try_into()
    .expect("SHA-256 prefix has fixed length");
  bytes[6] = (bytes[6] & 0x0f) | 0x50;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  Uuid::from_bytes(bytes)
}

fn unavailable(_: sqlx::Error) -> StoreError {
  StoreError::Unavailable
}
