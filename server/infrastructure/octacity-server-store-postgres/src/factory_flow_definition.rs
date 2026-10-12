use crate::{
  database::{classify, unavailable},
  discovery::{requested_row_limit, require_project, timestamp},
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
  read_visibility::sql_read_visibility,
};
use octacity_server_domain::{EntityKind, ProjectId};
use octacity_server_factory::{FlowDefinitionId, FlowDefinitionVersion};
use octacity_server_store::{
  FactoryFlowDefinitionMutationOutcome, FactoryFlowDefinitionPage, FlowDefinitionListVisibility,
  FlowDefinitionPagePosition, ManagementMutation, MutationDisposition, PublishFactoryFlowDefinition,
  PublishedFactoryFlowDefinition, ReadVisibilityKind, ReplaceFactoryConfigurationError, ReplayFactoryFlowDefinition,
  StoreError,
};
use serde_json::{Value, json};
use sqlx::{PgPool, types::Json};

fn identity(
  request: &ReplayFactoryFlowDefinition,
  audit: &octacity_server_store::MutationAuditContext,
) -> Result<MutationIdentity, StoreError> {
  // Publication time is server-owned and cannot change replay identity.
  MutationIdentity::new_management(
    audit,
    MutationKind::PublishFactoryFlowDefinition,
    request.idempotency_key.to_string(),
    octacity_server_domain::Timestamp::from_unix_millis(0).map_err(|_| StoreError::Unavailable)?,
    EntityKind::FlowDefinition,
    request,
  )
}
fn replay(value: Value) -> Result<FactoryFlowDefinitionMutationOutcome, StoreError> {
  let mut outcome: FactoryFlowDefinitionMutationOutcome = decode_outcome(value)?;
  outcome.disposition = MutationDisposition::Replayed;
  Ok(outcome)
}
pub(crate) async fn replay_mutation(
  pool: &PgPool,
  request: ManagementMutation<ReplayFactoryFlowDefinition>,
) -> Result<Option<FactoryFlowDefinitionMutationOutcome>, StoreError> {
  let (request, audit) = request.into_parts();
  let identity = identity(&request, &audit)?;
  let saved: Option<(Vec<u8>, Json<Value>)> = sqlx::query_as("SELECT request_digest, outcome FROM idempotency_records WHERE scope = $1 AND security_scope = $2 AND idempotency_key = $3")
    .bind(identity.kind.scope()).bind(identity.persisted_security_scope()).bind(&identity.key).fetch_optional(pool).await.map_err(unavailable)?;
  let Some((digest, Json(outcome))) = saved else {
    return Ok(None);
  };
  if digest != identity.request_digest {
    return Err(StoreError::Conflict {
      entity: EntityKind::FlowDefinition,
    });
  }
  replay(outcome).map(Some)
}
pub(crate) async fn publish(
  pool: &PgPool,
  request: ManagementMutation<PublishFactoryFlowDefinition>,
) -> Result<FactoryFlowDefinitionMutationOutcome, ReplaceFactoryConfigurationError> {
  let (request, audit) = request.into_parts();
  let mut identity = identity(&request.replay_probe(), &audit)?;
  identity.occurred_at = request.publication.published_at;
  let mut tx = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Replay(value) => return Ok(replay(value)?),
    MutationStart::Fresh(tx) => tx,
  };
  let publication = request.publication;
  let reference = publication.flow.closure.root();
  // Reconstruct at the persistence boundary, including canonical digest, resource and graph checks.
  if octacity_server_factory::FactoryFlowConfiguration::new(
    publication.flow.closure.clone(),
    publication.flow.limits.clone(),
    publication.flow.data_schemas.clone(),
    publication.flow.pools.clone(),
  )
  .is_err()
  {
    return Err(
      StoreError::Conflict {
        entity: EntityKind::FlowDefinition,
      }
      .into(),
    );
  }
  let version = i64::try_from(reference.version().get()).map_err(|_| StoreError::Unavailable)?;
  if let Some(expected) = request.expected_current_version {
    let current: Option<(uuid::Uuid, i64, i64)> = sqlx::query_as("SELECT project_id, current_version, FLOOR(EXTRACT(EPOCH FROM updated_at)*1000)::BIGINT FROM factory_flow_definitions WHERE id=$1 FOR UPDATE")
      .bind(reference.id().as_uuid()).fetch_optional(&mut *tx).await.map_err(unavailable)?;
    let (project, baseline, updated) = current.ok_or(StoreError::NotFound {
      entity: EntityKind::FlowDefinition,
    })?;
    if project != publication.project_id.as_uuid()
      || baseline as u64 != expected.get()
      || reference.version().get() != expected.get().saturating_add(1)
      || publication.published_at.unix_millis() < updated
    {
      return Err(ReplaceFactoryConfigurationError::PreconditionFailed);
    }
    sqlx::query("UPDATE factory_flow_definitions SET current_version=$2, updated_at=to_timestamp($3::double precision/1000.0) WHERE id=$1")
      .bind(reference.id().as_uuid()).bind(version).bind(publication.published_at.unix_millis()).execute(&mut *tx).await.map_err(unavailable)?;
  } else {
    if version != 1 {
      return Err(
        StoreError::Conflict {
          entity: EntityKind::FlowDefinition,
        }
        .into(),
      );
    }
    sqlx::query("INSERT INTO factory_flow_definitions (id,project_id,current_version,created_at,updated_at) VALUES ($1,$2,1,to_timestamp($3::double precision/1000.0),to_timestamp($3::double precision/1000.0))")
      .bind(reference.id().as_uuid()).bind(publication.project_id.as_uuid()).bind(publication.published_at.unix_millis()).execute(&mut *tx).await.map_err(|error|classify(error,EntityKind::FlowDefinition))?;
  }
  for definition in publication.flow.closure.definitions() {
    let exact = definition.reference();
    sqlx::query("INSERT INTO factory_flow_definition_versions (id,version,definition_digest,definition) VALUES ($1,$2,$3,$4) ON CONFLICT (id,version) DO NOTHING")
      .bind(exact.id().as_uuid()).bind(i64::try_from(exact.version().get()).map_err(|_|StoreError::Unavailable)?).bind(exact.digest().as_bytes().as_slice()).bind(Json(definition)).execute(&mut *tx).await.map_err(unavailable)?;
    let retained: Vec<u8> =
      sqlx::query_scalar("SELECT definition_digest FROM factory_flow_definition_versions WHERE id=$1 AND version=$2")
        .bind(exact.id().as_uuid())
        .bind(i64::try_from(exact.version().get()).map_err(|_| StoreError::Unavailable)?)
        .fetch_one(&mut *tx)
        .await
        .map_err(unavailable)?;
    if retained.as_slice() != exact.digest().as_bytes() {
      return Err(
        StoreError::Conflict {
          entity: EntityKind::FlowDefinition,
        }
        .into(),
      );
    }
  }
  sqlx::query("INSERT INTO factory_flow_publications (id,version,publication) VALUES ($1,$2,$3)")
    .bind(reference.id().as_uuid())
    .bind(version)
    .bind(Json(&publication))
    .execute(&mut *tx)
    .await
    .map_err(unavailable)?;
  let facts = json!({"project_id":publication.project_id,"version":version,"digest":reference.digest()});
  let outcome = FactoryFlowDefinitionMutationOutcome {
    disposition: MutationDisposition::Applied,
    publication,
  };
  crate::mutation::commit(
    tx,
    &identity,
    MutationFacts::management(&audit, reference.id().to_string(), facts.clone(), facts),
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}
pub(crate) async fn read(
  pool: &PgPool,
  id: FlowDefinitionId,
  version: Option<FlowDefinitionVersion>,
) -> Result<PublishedFactoryFlowDefinition, StoreError> {
  let row: Option<Json<PublishedFactoryFlowDefinition>> = sqlx::query_scalar("SELECT published.publication FROM factory_flow_definitions identity JOIN factory_flow_publications published ON published.id=identity.id AND published.version=COALESCE($2,identity.current_version) WHERE identity.id=$1")
    .bind(id.as_uuid()).bind(version.map(|v|i64::try_from(v.get()).map_err(|_|StoreError::Unavailable)).transpose()?).fetch_optional(pool).await.map_err(unavailable)?;
  row.map(|Json(value)| value).ok_or(StoreError::NotFound {
    entity: EntityKind::FlowDefinition,
  })
}
pub(crate) async fn list(
  pool: &PgPool,
  project: ProjectId,
  after: Option<FlowDefinitionPagePosition>,
  limit: u16,
  visibility: FlowDefinitionListVisibility,
) -> Result<FactoryFlowDefinitionPage, StoreError> {
  if limit == 0 || limit > octacity_server_store::MAX_FACTORY_CONFIGURATION_PAGE_SIZE {
    return Err(StoreError::Conflict {
      entity: EntityKind::FlowDefinition,
    });
  }
  if visibility.kind() == ReadVisibilityKind::None {
    return Ok(FactoryFlowDefinitionPage {
      total: 0,
      items: vec![],
      next_cursor: None,
    });
  }
  require_project(pool, project).await?;
  let mut tx = pool.begin().await.map_err(unavailable)?;
  sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
    .execute(&mut *tx)
    .await
    .map_err(unavailable)?;
  let visibility = sql_read_visibility(visibility.view(), |id| id.as_uuid());
  let total: i64 = sqlx::query_scalar(
    "SELECT COUNT(*) FROM factory_flow_definitions WHERE project_id=$1 AND ($2 OR id=ANY($3::uuid[]))",
  )
  .bind(project.as_uuid())
  .bind(visibility.all)
  .bind(&visibility.identities)
  .fetch_one(&mut *tx)
  .await
  .map_err(unavailable)?;
  let mut rows:Vec<(Json<PublishedFactoryFlowDefinition>,i64)>=sqlx::query_as("SELECT published.publication,FLOOR(EXTRACT(EPOCH FROM identity.created_at)*1000)::BIGINT FROM factory_flow_definitions identity JOIN factory_flow_publications published ON published.id=identity.id AND published.version=identity.current_version WHERE identity.project_id=$1 AND ($2 OR identity.id=ANY($3::uuid[])) AND ($4::bigint IS NULL OR (identity.created_at,identity.id)<(to_timestamp($4::double precision/1000.0),$5::uuid)) ORDER BY identity.created_at DESC,identity.id DESC LIMIT $6")
    .bind(project.as_uuid()).bind(visibility.all).bind(&visibility.identities).bind(after.map(|p|p.created_at.unix_millis())).bind(after.map(|p|p.id.as_uuid())).bind(requested_row_limit(limit)).fetch_all(&mut *tx).await.map_err(unavailable)?;
  tx.commit().await.map_err(unavailable)?;
  let more = rows.len() > usize::from(limit);
  rows.truncate(usize::from(limit));
  let next_cursor = if more {
    rows
      .last()
      .map(|(Json(publication), time)| {
        Ok(FlowDefinitionPagePosition {
          created_at: timestamp(*time)?,
          id: publication.flow.closure.root().id(),
        })
      })
      .transpose()?
  } else {
    None
  };
  Ok(FactoryFlowDefinitionPage {
    total: u64::try_from(total).map_err(|_| StoreError::Unavailable)?,
    items: rows.into_iter().map(|(Json(p), _)| p).collect(),
    next_cursor,
  })
}
