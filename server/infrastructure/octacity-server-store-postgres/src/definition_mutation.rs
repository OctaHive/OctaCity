use octacity_server_domain::{EntityKind, ProjectPolicyVersion};
use octacity_server_store::{
  CreateTriggerDefinition, MutationDisposition, ProjectPolicyMutationOutcome, PublishProjectPolicy, StoreError,
  StoreOperation, TriggerDefinitionMutationOutcome,
};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::types::Json;

use crate::{
  database::{classify, number, unavailable},
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};

#[derive(Serialize)]
struct PolicyFingerprint<'a> {
  project_id: octacity_server_domain::ProjectId,
  expected_current_version: Option<ProjectPolicyVersion>,
  policy: &'a Value,
}

#[derive(Serialize)]
struct TriggerFingerprint<'a> {
  version: octacity_server_domain::TriggerVersion,
  configuration_id: octacity_server_domain::BuildConfigurationId,
  configuration_version: octacity_server_domain::BuildConfigurationVersion,
  kind: octacity_server_store::TriggerKind,
  enabled: bool,
  definition: &'a Value,
}

pub(crate) async fn publish_policy(
  pool: &sqlx::PgPool,
  request: PublishProjectPolicy,
) -> Result<ProjectPolicyMutationOutcome, StoreError> {
  request.validate()?;
  let identity = MutationIdentity::new(
    MutationKind::PublishProjectPolicy,
    request.idempotency_key.to_string(),
    request.published_at,
    EntityKind::Project,
    &PolicyFingerprint {
      project_id: request.project_id,
      expected_current_version: request.expected_current_version,
      policy: &request.policy,
    },
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return replay_policy(outcome),
  };

  let project_exists = sqlx::query_scalar::<_, bool>("SELECT TRUE FROM projects WHERE id = $1 FOR UPDATE")
    .bind(request.project_id.as_uuid())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(unavailable)?
    .unwrap_or(false);
  if !project_exists {
    return Err(StoreError::NotFound {
      entity: EntityKind::Project,
    });
  }
  let current: Option<i64> =
    sqlx::query_scalar("SELECT MAX(version) FROM project_policy_versions WHERE project_id = $1")
      .bind(request.project_id.as_uuid())
      .fetch_one(&mut *transaction)
      .await
      .map_err(unavailable)?;
  let current = match current {
    Some(value) => Some(
      u64::try_from(value)
        .ok()
        .and_then(|value| ProjectPolicyVersion::new(value).ok())
        .ok_or(StoreError::Unavailable)?,
    ),
    None => None,
  };
  if current != request.expected_current_version {
    return Err(StoreError::Conflict {
      entity: EntityKind::Project,
    });
  }
  let version = match current {
    Some(version) => version.next().map_err(|_| StoreError::Unavailable)?,
    None => ProjectPolicyVersion::INITIAL,
  };
  sqlx::query(
    "INSERT INTO project_policy_versions (project_id, version, policy, created_at) \
     VALUES ($1, $2, $3, to_timestamp($4::double precision / 1000.0))",
  )
  .bind(request.project_id.as_uuid())
  .bind(number(version.get(), StoreOperation::PublishProjectPolicy)?)
  .bind(Json(request.policy))
  .bind(request.published_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Project))?;
  let outcome = ProjectPolicyMutationOutcome {
    disposition: MutationDisposition::Applied,
    project_id: request.project_id,
    version,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    MutationFacts {
      actor_kind: "management_api",
      actor_identity: None,
      target_identity: request.project_id.to_string(),
      safe_metadata: json!({"version": version.get()}),
      outbox_payload: json!({"project_id": request.project_id, "version": version}),
    },
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

pub(crate) async fn create_trigger(
  pool: &sqlx::PgPool,
  request: CreateTriggerDefinition,
) -> Result<TriggerDefinitionMutationOutcome, StoreError> {
  request.validate()?;
  let identity = MutationIdentity::new(
    MutationKind::CreateTriggerDefinition,
    request.idempotency_key.to_string(),
    request.created_at,
    EntityKind::Trigger,
    &TriggerFingerprint {
      version: request.version,
      configuration_id: request.configuration_id,
      configuration_version: request.configuration_version,
      kind: request.kind,
      enabled: request.enabled,
      definition: &request.definition,
    },
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return replay_trigger(outcome),
  };
  sqlx::query(
    "INSERT INTO triggers \
       (id, version, build_configuration_id, build_configuration_version, kind, enabled, definition, created_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, to_timestamp($8::double precision / 1000.0))",
  )
  .bind(request.id.as_uuid())
  .bind(number(request.version.get(), StoreOperation::CreateTriggerDefinition)?)
  .bind(request.configuration_id.as_uuid())
  .bind(number(
    request.configuration_version.get(),
    StoreOperation::CreateTriggerDefinition,
  )?)
  .bind(request.kind.as_str())
  .bind(request.enabled)
  .bind(Json(request.definition))
  .bind(request.created_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Trigger))?;
  let outcome = TriggerDefinitionMutationOutcome {
    disposition: MutationDisposition::Applied,
    trigger_id: request.id,
    version: request.version,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    MutationFacts {
      actor_kind: "management_api",
      actor_identity: None,
      target_identity: request.id.to_string(),
      safe_metadata: json!({
        "version": request.version.get(),
        "kind": request.kind.as_str(),
        "enabled": request.enabled,
      }),
      outbox_payload: json!({"trigger_id": request.id, "version": request.version}),
    },
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

fn replay_policy(outcome: Value) -> Result<ProjectPolicyMutationOutcome, StoreError> {
  let mut outcome: ProjectPolicyMutationOutcome = decode_outcome(outcome)?;
  outcome.disposition = MutationDisposition::Replayed;
  Ok(outcome)
}

fn replay_trigger(outcome: Value) -> Result<TriggerDefinitionMutationOutcome, StoreError> {
  let mut outcome: TriggerDefinitionMutationOutcome = decode_outcome(outcome)?;
  outcome.disposition = MutationDisposition::Replayed;
  Ok(outcome)
}
