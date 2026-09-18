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
    Ok(Self {
      kind,
      key,
      request_digest: digest.finalize().into(),
      occurred_at,
      conflict_entity,
    })
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
  CreatePipeline,
  PublishPipelineVersion,
  CreateRepository,
  PublishRepositoryVersion,
  CreateBuildConfiguration,
  PublishBuildConfigurationVersion,
  AcceptTrigger,
  ClaimReadyJob,
  AppendJobEvents,
  CompleteJob,
  IssueAgentEnrollment,
  RegisterAgent,
  RevokeAgentEnrollment,
  RevokeAgentRegistration,
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
      Self::AcceptTrigger => metadata!(
        b"octacity.accept-trigger.v2\0",
        "accept-trigger",
        "build",
        "build.accepted"
      ),
      Self::ClaimReadyJob => metadata!(
        b"octacity.claim-ready-job.v2\0",
        "claim-ready-job",
        "job",
        "job.claimed"
      ),
      Self::AppendJobEvents => metadata!(
        b"octacity.append-job-events.v1\0",
        "append-job-events",
        "job",
        "job.events-appended"
      ),
      Self::CompleteJob => metadata!(b"octacity.complete-job.v1\0", "complete-job", "job", "job.completed"),
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
    }
  }

  const fn digest_version(self) -> &'static [u8] {
    self.metadata().digest_version
  }

  const fn scope(self) -> &'static str {
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
  .bind(format!("{}:{}", identity.kind.scope(), identity.key))
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
