//! Managed publication shares the existing Source transaction and management mutation journal.
use crate::{
  PostgresStore,
  database::unavailable,
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};
use octacity_server_domain::{EntityKind, IntegrationId, Timestamp};
use octacity_server_factory::WorkSourceInput;
use octacity_server_store::*;
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction, types::Json};
const KIND: MutationKind = MutationKind::PublishWorkSourceConfiguration;
fn replay(value: Value) -> Result<WorkSourceConfigurationOutcome, StoreError> {
  let mut outcome: WorkSourceConfigurationOutcome = decode_outcome(value)?;
  outcome.disposition = MutationDisposition::Replayed;
  Ok(outcome)
}
#[async_trait::async_trait]
impl FactorySourceConfigurationStore for PostgresStore {
  async fn list_work_source_configurations(
    &self,
    request: ListWorkSourceConfigurations,
  ) -> Result<WorkSourceConfigurationPage, StoreError> {
    request.validate()?;
    crate::discovery::require_project(self.pool(), request.project_id).await?;
    let configurations =
      crate::read_visibility::sql_read_visibility(request.visibility.configurations.view(), |id| id.as_uuid());
    let repositories =
      crate::read_visibility::sql_read_visibility(request.visibility.repositories.view(), |id| id.as_uuid());
    let connections =
      crate::read_visibility::sql_read_visibility(request.visibility.connections.view(), |id| id.as_uuid());
    let after = request.after.map(|target| target.identity());
    let after_version = request.after.map(|target| match target {
      WorkSourceScanTarget::Input { version, .. } => version.get(),
      WorkSourceScanTarget::Group { version, .. } => version.get(),
    });
    let rows: Vec<(String, uuid::Uuid, i64, Json<Value>)> = sqlx::query_as(
      "WITH inputs AS (SELECT DISTINCT ON(id) id,version,document FROM factory_source_inputs WHERE project_id=$1 ORDER BY id,version DESC), \
       groups AS (SELECT DISTINCT ON(id) id,version,document FROM factory_source_groups WHERE project_id=$1 ORDER BY id,version DESC), \
       heads AS (SELECT 'input'::text kind,id,version,document FROM inputs WHERE $11::uuid IS NULL UNION ALL SELECT 'group'::text kind,id,version,document FROM groups WHERE $11::uuid IS NULL UNION ALL SELECT 'input'::text kind,id,version,document FROM factory_source_inputs WHERE project_id=$1 AND $11::uuid IS NOT NULL AND (document#>>'{definition,connection,id}')::uuid=$11) \
       SELECT kind,id,version,document FROM heads h WHERE NOT EXISTS ( \
         SELECT 1 FROM jsonb_array_elements(CASE WHEN h.kind='input' THEN jsonb_build_array(h.document) ELSE h.document->'members' END) member \
         WHERE NOT (($2 OR (member#>>'{definition,target,id}')::uuid=ANY($3::uuid[])) \
           AND ($4 OR (member#>>'{definition,repository,id}')::uuid=ANY($5::uuid[])) \
           AND ((member#>>'{definition,connection,id}') IS NULL OR $6 OR (member#>>'{definition,connection,id}')::uuid=ANY($7::uuid[])))) \
       AND ($8::text IS NULL OR (kind,id,CASE WHEN $11::uuid IS NULL THEN 0 ELSE version END)>($8,$9::uuid,$12::bigint)) ORDER BY kind,id,version LIMIT $10"
    ).bind(request.project_id.as_uuid()).bind(configurations.all).bind(configurations.identities)
      .bind(repositories.all).bind(repositories.identities).bind(connections.all).bind(connections.identities)
      .bind(after.map(|a| a.0)).bind(after.map(|a| a.1)).bind(i64::from(request.limit)+1)
      .bind(request.connection_dependency.map(|id| id.as_uuid()))
      .bind(if request.connection_dependency.is_some() { after_version.map(|v| i64::try_from(v).map_err(|_| StoreError::Unavailable)).transpose()? } else { Some(0) })
      .fetch_all(self.pool()).await.map_err(unavailable)?;
    let mut items = vec![];
    for (kind, id, version, Json(document)) in rows {
      let record = match kind.as_str() {
        "input" => {
          WorkSourceConfigurationRecord::Input(serde_json::from_value(document).map_err(|_| StoreError::Unavailable)?)
        }
        "group" => {
          WorkSourceConfigurationRecord::Group(serde_json::from_value(document).map_err(|_| StoreError::Unavailable)?)
        }
        _ => return Err(StoreError::Unavailable),
      };
      let target = record.target();
      let retained_version = match target {
        WorkSourceScanTarget::Input { version, .. } => version.get(),
        WorkSourceScanTarget::Group { version, .. } => version.get(),
      };
      if target.identity() != (kind.as_str(), id)
        || version <= 0
        || retained_version != version as u64
        || record.inputs().iter().any(|i| i.project_id() != request.project_id)
      {
        return Err(StoreError::Unavailable);
      }
      items.push(record);
    }
    let more = items.len() > usize::from(request.limit);
    items.truncate(usize::from(request.limit));
    let next_cursor = more.then(|| items.last().map(|r| r.target())).flatten();
    Ok(WorkSourceConfigurationPage { items, next_cursor })
  }
  async fn replay_work_source_configuration(
    &self,
    request: ManagementMutation<ReplayWorkSourceConfiguration>,
  ) -> Result<Option<WorkSourceConfigurationOutcome>, StoreError> {
    let (request, audit) = request.into_parts();
    let identity = MutationIdentity::new_management(
      &audit,
      KIND,
      request.idempotency_key.to_string(),
      Timestamp::from_unix_millis(0).map_err(|_| StoreError::Unavailable)?,
      EntityKind::Project,
      &request.intent,
    )?;
    let row: Option<(Vec<u8>, Json<Value>)> = sqlx::query_as("SELECT request_digest, outcome FROM idempotency_records WHERE scope=$1 AND security_scope=$2 AND idempotency_key=$3")
      .bind(KIND.scope()).bind(identity.persisted_security_scope()).bind(&identity.key).fetch_optional(self.pool()).await.map_err(unavailable)?;
    let Some((digest, Json(value))) = row else {
      return Ok(None);
    };
    if digest != identity.request_digest {
      return Err(source_conflict());
    }
    Ok(Some(replay(value)?))
  }
  async fn publish_work_source_configuration(
    &self,
    request: ManagementMutation<PublishWorkSourceConfiguration>,
  ) -> Result<WorkSourceConfigurationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    request.validate()?;
    let identity = MutationIdentity::new_management(
      &audit,
      KIND,
      request.idempotency_key.to_string(),
      request.published_at,
      EntityKind::Project,
      &request.intent,
    )?;
    let mut tx = match crate::mutation::begin(self.pool(), &identity).await? {
      MutationStart::Replay(value) => return replay(value),
      MutationStart::Fresh(tx) => tx,
    };
    crate::factory_source::publish_in(&mut tx, request.publication.clone()).await?;
    let outcome = request.outcome();
    let facts = MutationFacts::management(
      &audit,
      request.intent.project_id.to_string(),
      json!({"inputs":outcome.publication.inputs.len(),"group":outcome.publication.group.is_some()}),
      json!({"project_id":request.intent.project_id,"inputs":outcome.publication.inputs.iter().map(|c| c.input.id()).collect::<Vec<_>>(),"group":outcome.publication.group.as_ref().map(|c| c.group.id())}),
    );
    crate::mutation::commit(tx, &identity, facts, encode_outcome(&outcome)?).await?;
    Ok(outcome)
  }
}

/// Lock connections before the Source Project lock, matching grant publication's
/// connection-before-Project/FK order. Sorted identities avoid cross-input cycles.
pub(crate) async fn lock_connections(
  tx: &mut Transaction<'_, Postgres>,
  request: &PublishWorkSources,
) -> Result<(), StoreError> {
  let ids = request
    .inputs
    .iter()
    .map(|c| &c.input)
    .chain(request.group.iter().flat_map(|c| c.group.members()))
    .filter_map(|i| i.definition().connection.as_ref().map(|c| c.id))
    .collect::<std::collections::BTreeSet<IntegrationId>>();
  for id in ids {
    let current: Option<i64> =
      sqlx::query_scalar("SELECT current_version FROM integration_connections WHERE id=$1 FOR SHARE")
        .bind(id.as_uuid())
        .fetch_optional(&mut **tx)
        .await
        .map_err(unavailable)?;
    for input in request
      .inputs
      .iter()
      .map(|c| &c.input)
      .chain(request.group.iter().flat_map(|c| c.group.members()))
    {
      if let Some(connection) = &input.definition().connection
        && connection.id == id
        && current.map(|v| v as u64) != Some(connection.version.get())
      {
        return Err(source_conflict());
      }
    }
  }
  Ok(())
}
pub(crate) async fn validate_managed(
  tx: &mut Transaction<'_, Postgres>,
  input: &WorkSourceInput,
) -> Result<(), StoreError> {
  let Some(connection) = &input.definition().connection else {
    return Ok(());
  };
  let metadata = crate::integration_connection::read(&mut **tx, connection.id, Some(connection.version)).await?;
  let access = crate::integration_project_access::read(&mut **tx, connection.id, input.project_id())
    .await?
    .ok_or_else(source_conflict)?;
  let administration = crate::integration_administration::read_state(tx, connection.id).await?;
  validate_managed_work_source(input, &metadata, &access, &administration)
}
