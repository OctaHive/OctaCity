use octacity_server_domain::{EntityKind, ProjectId, Timestamp};
use octacity_server_factory::{
  FactoryConfiguration, FactoryConfigurationId, FactoryConfigurationVersion, PinnedFlowDefinitionClosure,
};
use octacity_server_store::{
  CreateFactoryConfiguration, FactoryConfigurationMutationIntent, FactoryConfigurationMutationOutcome,
  ManagementMutation, MutationAuditContext, MutationDisposition, PublishedFactoryConfiguration,
  ReplaceFactoryConfiguration, ReplaceFactoryConfigurationError, ReplayFactoryConfigurationMutation, StoreError,
  StoreInputError, StoreOperation,
};
use serde_json::{Value, json};
use sqlx::{FromRow, PgPool, Postgres, Transaction, types::Json};

use crate::{
  database::{classify, unavailable},
  discovery::{positive, timestamp},
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};

#[derive(FromRow)]
struct ConfigurationRow {
  project_id: uuid::Uuid,
  version: i64,
  definition_digest: Vec<u8>,
  definition: Json<FactoryConfiguration>,
  enabled: bool,
  published_at_millis: i64,
}

pub(crate) async fn replay_mutation(
  pool: &PgPool,
  request: ManagementMutation<ReplayFactoryConfigurationMutation>,
) -> Result<Option<FactoryConfigurationMutationOutcome>, StoreError> {
  let (request, audit) = request.into_parts();
  let kind = mutation_kind(&request.intent);
  let identity = MutationIdentity::new_management(
    &audit,
    kind,
    request.idempotency_key.to_string(),
    Timestamp::from_unix_millis(0).map_err(|_| StoreError::Unavailable)?,
    EntityKind::FactoryConfiguration,
    &request.intent,
  )?;
  let stored = sqlx::query_as::<_, (Vec<u8>, Json<Value>)>(
    "SELECT request_digest, outcome FROM idempotency_records \
     WHERE scope = $1 AND security_scope = $2 AND idempotency_key = $3",
  )
  .bind(identity.kind.scope())
  .bind(identity.persisted_security_scope())
  .bind(&identity.key)
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?;
  let Some((digest, Json(outcome))) = stored else {
    return Ok(None);
  };
  if digest != identity.request_digest {
    return Err(conflict());
  }
  Ok(Some(replay(outcome)?))
}

pub(crate) async fn create(
  pool: &PgPool,
  request: ManagementMutation<CreateFactoryConfiguration>,
) -> Result<FactoryConfigurationMutationOutcome, StoreError> {
  let (request, audit) = request.into_parts();
  validate_intent(
    &request.intent,
    &request.configuration,
    None,
    StoreOperation::CreateFactoryConfiguration,
  )?;
  let reference = request.configuration.reference();
  let identity = MutationIdentity::new_management(
    &audit,
    MutationKind::CreateFactoryConfiguration,
    request.idempotency_key.to_string(),
    request.published_at,
    EntityKind::FactoryConfiguration,
    &request.intent,
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return replay(outcome),
  };
  require_project(&mut transaction, reference.project_id()).await?;
  if reference.version() != FactoryConfigurationVersion::INITIAL {
    return Err(conflict());
  }
  sqlx::query(
    "INSERT INTO factory_configurations (id, project_id, current_version, created_at, updated_at) \
     VALUES ($1, $2, $3, to_timestamp($4::double precision / 1000.0), \
             to_timestamp($4::double precision / 1000.0))",
  )
  .bind(reference.id().as_uuid())
  .bind(reference.project_id().as_uuid())
  .bind(version_number(reference.version())?)
  .bind(request.published_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(|error| classify(error, EntityKind::FactoryConfiguration))?;
  insert_version(&mut transaction, &request.configuration, request.published_at).await?;
  let published = PublishedFactoryConfiguration {
    configuration: request.configuration,
    published_at: request.published_at,
  };
  let outcome = FactoryConfigurationMutationOutcome {
    disposition: MutationDisposition::Applied,
    configuration: published,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    facts(&audit, MutationKind::CreateFactoryConfiguration, &outcome.configuration),
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

pub(crate) async fn replace(
  pool: &PgPool,
  request: ManagementMutation<ReplaceFactoryConfiguration>,
) -> Result<FactoryConfigurationMutationOutcome, ReplaceFactoryConfigurationError> {
  let (request, audit) = request.into_parts();
  validate_intent(
    &request.intent,
    &request.configuration,
    Some(request.expected_current_version),
    StoreOperation::ReplaceFactoryConfiguration,
  )?;
  let reference = request.configuration.reference();
  let identity = MutationIdentity::new_management(
    &audit,
    MutationKind::ReplaceFactoryConfiguration,
    request.idempotency_key.to_string(),
    request.published_at,
    EntityKind::FactoryConfiguration,
    &request.intent,
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return Ok(replay(outcome)?),
  };
  let current: Option<(uuid::Uuid, i64, i64)> = sqlx::query_as(
    "SELECT project_id, current_version, FLOOR(EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT \
     FROM factory_configurations WHERE id = $1 FOR UPDATE",
  )
  .bind(reference.id().as_uuid())
  .fetch_optional(&mut *transaction)
  .await
  .map_err(unavailable)?;
  let Some((project_id, current_version, current_published_at)) = current else {
    return Err(
      StoreError::NotFound {
        entity: EntityKind::FactoryConfiguration,
      }
      .into(),
    );
  };
  if project_id != reference.project_id().as_uuid()
    || current_version != version_number(request.expected_current_version)?
    || request.published_at.unix_millis() < current_published_at
  {
    return Err(ReplaceFactoryConfigurationError::PreconditionFailed);
  }
  let expected_next = request
    .expected_current_version
    .get()
    .checked_add(1)
    .ok_or(StoreError::Unavailable)?;
  if reference.version().get() != expected_next {
    return Err(conflict().into());
  }
  insert_version(&mut transaction, &request.configuration, request.published_at).await?;
  let updated = sqlx::query(
    "UPDATE factory_configurations SET current_version = $1, \
       updated_at = to_timestamp($2::double precision / 1000.0) \
     WHERE id = $3 AND current_version = $4",
  )
  .bind(version_number(reference.version())?)
  .bind(request.published_at.unix_millis())
  .bind(reference.id().as_uuid())
  .bind(version_number(request.expected_current_version)?)
  .execute(&mut *transaction)
  .await
  .map_err(unavailable)?;
  if updated.rows_affected() != 1 {
    return Err(ReplaceFactoryConfigurationError::PreconditionFailed);
  }
  let published = PublishedFactoryConfiguration {
    configuration: request.configuration,
    published_at: request.published_at,
  };
  let outcome = FactoryConfigurationMutationOutcome {
    disposition: MutationDisposition::Applied,
    configuration: published,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    facts(
      &audit,
      MutationKind::ReplaceFactoryConfiguration,
      &outcome.configuration,
    ),
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

pub(crate) async fn read(
  pool: &PgPool,
  id: FactoryConfigurationId,
  version: FactoryConfigurationVersion,
) -> Result<PublishedFactoryConfiguration, StoreError> {
  let row = sqlx::query_as::<_, ConfigurationRow>(
    "SELECT configuration.project_id, version.version, version.definition_digest, version.definition, \
            version.enabled, FLOOR(EXTRACT(EPOCH FROM version.published_at) * 1000)::BIGINT AS published_at_millis \
     FROM factory_configuration_versions AS version \
     JOIN factory_configurations AS configuration ON configuration.id = version.factory_configuration_id \
     WHERE version.factory_configuration_id = $1 AND version.version = $2",
  )
  .bind(id.as_uuid())
  .bind(version_number(version)?)
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::FactoryConfiguration,
  })?;
  decode_row(id, row)
}

pub(crate) async fn read_current(
  pool: &PgPool,
  id: FactoryConfigurationId,
) -> Result<PublishedFactoryConfiguration, StoreError> {
  let row = sqlx::query_as::<_, ConfigurationRow>(
    "SELECT configuration.project_id, version.version, version.definition_digest, version.definition, \
            version.enabled, FLOOR(EXTRACT(EPOCH FROM version.published_at) * 1000)::BIGINT AS published_at_millis \
     FROM factory_configurations AS configuration \
     JOIN factory_configuration_versions AS version \
       ON version.factory_configuration_id = configuration.id \
      AND version.version = configuration.current_version \
     WHERE configuration.id = $1",
  )
  .bind(id.as_uuid())
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::FactoryConfiguration,
  })?;
  decode_row(id, row)
}

async fn insert_version(
  transaction: &mut Transaction<'_, Postgres>,
  configuration: &FactoryConfiguration,
  published_at: Timestamp,
) -> Result<(), StoreError> {
  let reference = configuration.reference();
  sqlx::query(
    "INSERT INTO factory_configuration_versions \
       (factory_configuration_id, version, definition_digest, definition, enabled, published_at) \
     VALUES ($1, $2, $3, $4, $5, to_timestamp($6::double precision / 1000.0))",
  )
  .bind(reference.id().as_uuid())
  .bind(version_number(reference.version())?)
  .bind(reference.definition_digest().as_bytes().as_slice())
  .bind(Json(configuration))
  .bind(configuration.is_enabled())
  .bind(published_at.unix_millis())
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::FactoryConfiguration))?;
  let closure =
    PinnedFlowDefinitionClosure::from_stage_projection(configuration).map_err(|_| StoreError::Unavailable)?;
  for (ordinal, definition) in closure.definitions().iter().enumerate() {
    let reference = definition.reference();
    sqlx::query(
      "INSERT INTO factory_flow_definition_versions (id, version, definition_digest, definition) \
       VALUES ($1, $2, $3, $4) ON CONFLICT (id, version) DO NOTHING",
    )
    .bind(reference.id().as_uuid())
    .bind(i64::try_from(reference.version().get()).map_err(|_| StoreError::Unavailable)?)
    .bind(reference.digest().as_bytes().as_slice())
    .bind(Json(definition))
    .execute(&mut **transaction)
    .await
    .map_err(unavailable)?;
    let persisted_digest: Vec<u8> = sqlx::query_scalar(
      "SELECT definition_digest FROM factory_flow_definition_versions WHERE id = $1 AND version = $2",
    )
    .bind(reference.id().as_uuid())
    .bind(i64::try_from(reference.version().get()).map_err(|_| StoreError::Unavailable)?)
    .fetch_one(&mut **transaction)
    .await
    .map_err(unavailable)?;
    if persisted_digest.as_slice() != reference.digest().as_bytes() {
      return Err(StoreError::Conflict {
        entity: EntityKind::FactoryConfiguration,
      });
    }
    sqlx::query(
      "INSERT INTO factory_configuration_flow_definitions \
         (factory_configuration_id, factory_configuration_version, flow_definition_id, \
          flow_definition_version, ordinal, is_root) \
       VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(configuration.reference().id().as_uuid())
    .bind(version_number(configuration.reference().version())?)
    .bind(reference.id().as_uuid())
    .bind(i64::try_from(reference.version().get()).map_err(|_| StoreError::Unavailable)?)
    .bind(i16::try_from(ordinal).map_err(|_| StoreError::Unavailable)?)
    .bind(reference == closure.root())
    .execute(&mut **transaction)
    .await
    .map_err(unavailable)?;
  }
  Ok(())
}

fn decode_row(id: FactoryConfigurationId, row: ConfigurationRow) -> Result<PublishedFactoryConfiguration, StoreError> {
  let version = FactoryConfigurationVersion::new(positive(row.version)?).map_err(|_| StoreError::Unavailable)?;
  let project_id = ProjectId::from_uuid(row.project_id).map_err(|_| StoreError::Unavailable)?;
  let definition_digest: [u8; 32] = row.definition_digest.try_into().map_err(|_| StoreError::Unavailable)?;
  let reference = row.definition.reference();
  if reference.id() != id
    || reference.version() != version
    || reference.project_id() != project_id
    || reference.definition_digest().as_bytes() != definition_digest
    || row.definition.is_enabled() != row.enabled
  {
    return Err(StoreError::Unavailable);
  }
  Ok(PublishedFactoryConfiguration {
    configuration: row.definition.0,
    published_at: timestamp(row.published_at_millis)?,
  })
}

async fn require_project(transaction: &mut Transaction<'_, Postgres>, project_id: ProjectId) -> Result<(), StoreError> {
  let found = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM projects WHERE id = $1)")
    .bind(project_id.as_uuid())
    .fetch_one(&mut **transaction)
    .await
    .map_err(unavailable)?;
  found.then_some(()).ok_or(StoreError::NotFound {
    entity: EntityKind::Project,
  })
}

fn validate_intent(
  intent: &FactoryConfigurationMutationIntent,
  configuration: &FactoryConfiguration,
  expected_current_version: Option<FactoryConfigurationVersion>,
  operation: StoreOperation,
) -> Result<(), StoreError> {
  intent
    .matches_configuration(configuration, expected_current_version)
    .then_some(())
    .ok_or(StoreError::InvalidInput {
      operation,
      source: StoreInputError::InvalidFactoryConfiguration,
    })
}

fn facts(
  audit: &MutationAuditContext,
  kind: MutationKind,
  configuration: &PublishedFactoryConfiguration,
) -> MutationFacts {
  let reference = configuration.configuration.reference();
  MutationFacts::management(
    audit,
    reference.id().to_string(),
    json!({
      "enabled": configuration.configuration.is_enabled(),
      "project_id": reference.project_id(),
      "version": reference.version().get(),
    }),
    json!({
      "event": kind.outbox_topic(),
      "factory_configuration_id": reference.id(),
      "project_id": reference.project_id(),
      "version": reference.version().get(),
    }),
  )
}

fn replay(value: Value) -> Result<FactoryConfigurationMutationOutcome, StoreError> {
  let mut outcome: FactoryConfigurationMutationOutcome = decode_outcome(value)?;
  outcome.disposition = MutationDisposition::Replayed;
  Ok(outcome)
}

const fn mutation_kind(intent: &FactoryConfigurationMutationIntent) -> MutationKind {
  match intent {
    FactoryConfigurationMutationIntent::Create { .. } => MutationKind::CreateFactoryConfiguration,
    FactoryConfigurationMutationIntent::Replace { .. } => MutationKind::ReplaceFactoryConfiguration,
  }
}

fn version_number(version: FactoryConfigurationVersion) -> Result<i64, StoreError> {
  i64::try_from(version.get()).map_err(|_| StoreError::Unavailable)
}

const fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::FactoryConfiguration,
  }
}
