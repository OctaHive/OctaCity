use std::str::FromStr;

use octacity_server_domain::EntityKind;
use octacity_server_factory::FactoryDigest;
use octacity_server_store::{
  FactoryRunDiagnosticKind, FactoryRunDiagnosticPage, FactoryRunDiagnosticRecord, ListFactoryRunDiagnostics,
  StoreError, StoreInputError, StoreOperation,
};
use serde::de::DeserializeOwned;
use sqlx::{PgPool, types::Json};

use crate::database::unavailable;

pub(crate) async fn list_diagnostics(
  pool: &PgPool,
  request: ListFactoryRunDiagnostics,
) -> Result<FactoryRunDiagnosticPage, StoreError> {
  request.validate()?;
  let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM factory_runs WHERE id = $1 AND visible)")
    .bind(request.run_id.as_uuid())
    .fetch_one(pool)
    .await
    .map_err(unavailable)?;
  if !exists {
    return Err(StoreError::NotFound {
      entity: EntityKind::FactoryRun,
    });
  }
  let limit = i64::from(request.limit.get()) + 1;
  let records = match request.kind {
    FactoryRunDiagnosticKind::EffectDelivery => digest_rows(pool,
      "WITH latest AS (SELECT DISTINCT ON (operation_id) * FROM factory_outbox_records WHERE run_id=$1 \
       ORDER BY operation_id, attempt DESC, CASE state WHEN 'pending' THEN 0 WHEN 'claimed' THEN 1 ELSE 2 END DESC, recorded_at DESC, id DESC) \
       SELECT jsonb_build_object('id',encode(operation_id,'hex'),'record_id',encode(id,'hex'),'kind',kind, \
         'input_digest',encode(input_digest,'hex'),'state',state,'failure',to_jsonb(failure),'attempt',attempt, \
         'available_at',FLOOR(EXTRACT(EPOCH FROM available_at)*1000)::bigint, \
         'recorded_at',FLOOR(EXTRACT(EPOCH FROM recorded_at)*1000)::bigint) FROM latest \
       WHERE ($2::bytea IS NULL OR operation_id>$2) ORDER BY operation_id LIMIT $3",
      &request, limit, |value| FactoryRunDiagnosticRecord::EffectDelivery(Box::new(value)),
    ).await?,
    FactoryRunDiagnosticKind::ProfileSelection => digest_rows(pool,
      "SELECT to_jsonb(encode(id,'hex')) FROM factory_flow_profile_selections WHERE run_id=$1 AND ($2::bytea IS NULL OR id>$2) ORDER BY id LIMIT $3",
      &request, limit, FactoryRunDiagnosticRecord::ProfileSelection,
    ).await?,
    FactoryRunDiagnosticKind::NodeStop => digest_rows(pool,
      "SELECT convert_from(document,'UTF8')::jsonb FROM factory_node_stops WHERE run_id = $1 AND ($2::bytea IS NULL OR id > $2) ORDER BY id LIMIT $3",
      &request, limit, |value| FactoryRunDiagnosticRecord::NodeStop(Box::new(value)),
    ).await?,

    FactoryRunDiagnosticKind::Work => uuid_rows(pool,
      "SELECT work.envelope FROM factory_runs run JOIN factory_work_envelopes work ON work.id = run.work_envelope_id \
       WHERE run.id = $1 AND ($2::uuid IS NULL OR work.id > $2) ORDER BY work.id LIMIT $3",
      &request, limit, |value| FactoryRunDiagnosticRecord::Work(Box::new(value)),
    ).await?,
    FactoryRunDiagnosticKind::Call => call_rows(pool, &request, limit).await?,
    FactoryRunDiagnosticKind::DomainFact(_) | FactoryRunDiagnosticKind::CandidatePublication => {
      let after = digest_cursor(&request)?;
      sqlx::query_scalar::<_, Json<octacity_server_store::FactoryDomainDiagnostic>>(
        "SELECT jsonb_build_object('id', encode(record.id,'hex'), 'node_attempt_id', record.node_attempt_id, \
          'flow_run_id', node.flow_run_id, 'cycle_id', node.workflow_cycle_id, 'input_digest', encode(record.input_digest,'hex'), \
          'observed_at', record.observed_at_millis, 'facts', record.discovery_facts) \
         FROM factory_flow_records record JOIN factory_node_attempts node ON node.id = record.node_attempt_id AND node.run_id = record.run_id \
         WHERE record.run_id = $1 AND ($2::bytea IS NULL OR record.id > $2) AND record.discovery_facts ->> 'kind' = ANY($4) \
         ORDER BY record.id LIMIT $3"
      ).bind(request.run_id.as_uuid()).bind(after).bind(limit).bind(request.kind.discovery_kinds().iter().map(|kind| kind.as_str()).collect::<Vec<_>>()).fetch_all(pool).await.map_err(unavailable)?
        .into_iter().map(|Json(row)| FactoryRunDiagnosticRecord::DomainFact(Box::new(row))).collect()
    }

    FactoryRunDiagnosticKind::BuildExecution => digest_rows(pool,
      "SELECT jsonb_build_object('id', encode(id,'hex'), 'operation_id', encode(operation_id,'hex'), \
        'node_attempt_id', node_attempt_id, 'build_id', build_id, 'attempt_id', attempt_id, \
        'jobs', convert_from(document,'UTF8')::jsonb -> 'jobs') \
       FROM factory_flow_build_executions WHERE run_id = $1 AND ($2::bytea IS NULL OR id > $2) ORDER BY id LIMIT $3",
      &request, limit, |value| FactoryRunDiagnosticRecord::BuildExecution(Box::new(value)),
    ).await?,
    FactoryRunDiagnosticKind::Budget => digest_rows(pool,
      "SELECT jsonb_build_object('id', encode(id,'hex'), 'run_version', run_version, 'usage', usage, \
        'recorded_at', FLOOR(EXTRACT(EPOCH FROM recorded_at)*1000)::BIGINT) \
       FROM factory_run_budgets WHERE run_id = $1 AND ($2::bytea IS NULL OR id > $2) ORDER BY id LIMIT $3",
      &request, limit, |value| FactoryRunDiagnosticRecord::Budget(Box::new(value)),
    ).await?,
    FactoryRunDiagnosticKind::WorkflowCycle => uuid_rows(pool,
      "SELECT jsonb_build_object('id', id, 'flow_run_id', flow_run_id, 'number', cycle_number, \
         'predecessor', predecessor_id, 'source_attempt_id', source_attempt_id) \
       FROM factory_workflow_cycles WHERE factory_run_id = $1 AND ($2::uuid IS NULL OR id > $2) ORDER BY id LIMIT $3",
      &request, limit, |value| FactoryRunDiagnosticRecord::WorkflowCycle(Box::new(value)),
    ).await?,
    FactoryRunDiagnosticKind::FlowRun => uuid_rows(pool,
      "SELECT jsonb_build_object('id', flow.id, 'factory_run_id', flow.factory_run_id, \
         'definition', jsonb_build_object('id', definition.id, 'version', definition.version, \
           'digest', encode(definition.definition_digest, 'hex')), \
         'parent', CASE WHEN flow.parent_flow_run_id IS NULL THEN NULL ELSE \
           jsonb_build_object('flow_run_id', flow.parent_flow_run_id, 'node_attempt_id', flow.parent_node_attempt_id) END) \
       FROM factory_flow_runs flow JOIN factory_flow_definition_versions definition \
         ON definition.id = flow.flow_definition_id AND definition.version = flow.flow_definition_version \
       WHERE flow.factory_run_id = $1 AND ($2::uuid IS NULL OR flow.id > $2) ORDER BY flow.id LIMIT $3",
      &request, limit, |value| FactoryRunDiagnosticRecord::FlowRun(Box::new(value)),
    ).await?,
    FactoryRunDiagnosticKind::Claim => {
      let after = digest_cursor(&request)?;
      sqlx::query_as::<_, (Vec<u8>, uuid::Uuid, String, Vec<u8>, i64, i64)>(
        "SELECT id, run_id, owner, fence, FLOOR(EXTRACT(EPOCH FROM claimed_at)*1000)::BIGINT, \
          FLOOR(EXTRACT(EPOCH FROM expires_at)*1000)::BIGINT FROM factory_run_claims \
          WHERE run_id=$1 AND ($2::BYTEA IS NULL OR id>$2) ORDER BY id LIMIT $3"
      ).bind(request.run_id.as_uuid()).bind(after).bind(limit).fetch_all(pool).await.map_err(unavailable)?
        .into_iter().map(|(id, run, owner, fence, claimed, expires)| {
          let claim = super::decode_claim((run, owner, fence, claimed, expires))?;
          if claim.id.as_bytes().as_slice() != id { return Err(StoreError::Unavailable); }
          Ok(FactoryRunDiagnosticRecord::Claim(Box::new(claim)))
        }).collect::<Result<Vec<_>, _>>()?
    }
    FactoryRunDiagnosticKind::Audit => {
      let after = digest_cursor(&request)?;
      sqlx::query_as::<_, super::FactoryAuditRow>(
        "SELECT id,run_id,actor_kind,actor_identity_digest,operation,request_identity_digest,outcome, \
          FLOOR(EXTRACT(EPOCH FROM recorded_at)*1000)::BIGINT AS recorded_at_millis FROM factory_audit_links \
          WHERE run_id=$1 AND ($2::BYTEA IS NULL OR id>$2) ORDER BY id LIMIT $3"
      ).bind(request.run_id.as_uuid()).bind(after).bind(limit).fetch_all(pool).await.map_err(unavailable)?
        .into_iter().map(|row| super::decode_audit_row(row).map(|row| FactoryRunDiagnosticRecord::Audit(Box::new(row))))
        .collect::<Result<Vec<_>, _>>()?
    }
    FactoryRunDiagnosticKind::NodeAttempt => uuid_rows(pool, "SELECT node_attempt FROM factory_node_attempts WHERE run_id = $1 AND ($2::UUID IS NULL OR id > $2) ORDER BY id LIMIT $3", &request, limit, |value| FactoryRunDiagnosticRecord::NodeAttempt(Box::new(value))).await?,
    FactoryRunDiagnosticKind::NodeAttemptCompletion => digest_rows(pool, "SELECT completion FROM factory_node_attempt_completions WHERE run_id = $1 AND ($2::BYTEA IS NULL OR id > $2) ORDER BY id LIMIT $3", &request, limit, |value| FactoryRunDiagnosticRecord::NodeAttemptCompletion(Box::new(value))).await?,

  };
  page(records, usize::from(request.limit.get()))
}

/// Reads only the bounded invocations selected by identity. Protected Context and
/// permissions are reconstructed internally to hash their canonical contracts;
/// only the explicit profile allowlist crosses the application boundary.
async fn call_rows(
  pool: &PgPool,
  request: &ListFactoryRunDiagnostics,
  limit: i64,
) -> Result<Vec<FactoryRunDiagnosticRecord>, StoreError> {
  use octacity_server_factory::{ContextManifest, FactoryPermissionSet, FlowBuildProfile, NodeAttempt};
  let after = request
    .after
    .as_ref()
    .map(|key| uuid::Uuid::from_str(key.as_str()))
    .transpose()
    .map_err(|_| invalid_cursor())?;
  let rows = sqlx::query_as::<_, (Json<NodeAttempt>, Option<Vec<u8>>, Option<Json<FlowBuildProfile>>, Option<Json<ContextManifest>>, Option<Json<FactoryPermissionSet>>, Option<Json<octacity_server_factory::FactoryRoleProfileRef>>)>(
    "SELECT node.node_attempt, intent.operation_id, data.document -> 'profile', data.document -> 'input' -> 'context', data.document -> 'permissions', data.document -> 'selection' -> 'profile' \
     FROM (SELECT id, run_id, node_attempt FROM factory_node_attempts WHERE run_id = $1 \
       AND ($2::uuid IS NULL OR id > $2) AND node_attempt -> 'execution' ->> 'kind' = 'external' ORDER BY id LIMIT $3) node \
     LEFT JOIN factory_flow_build_intents intent ON intent.run_id = node.run_id AND intent.node_attempt_id = node.id \
     LEFT JOIN LATERAL (SELECT convert_from(intent.document,'UTF8')::jsonb AS document) data ON TRUE ORDER BY node.id"
  ).bind(request.run_id.as_uuid()).bind(after).bind(limit).fetch_all(pool).await.map_err(unavailable)?;
  rows
    .into_iter()
    .map(
      |(Json(node), operation, profile, context, permissions, selected_profile)| {
        let profile = match (operation, profile, context, permissions) {
          (Some(operation), Some(Json(profile)), Some(Json(context)), Some(Json(permissions))) => {
            Some(Box::new(octacity_server_store::FactoryCallProfileDiagnostic {
              selected_profile: selected_profile.map(|Json(profile)| profile),
              operation_id: super::digest(&operation)?,
              context_id: context.id(),
              context_digest: context.digest().map_err(|_| StoreError::Unavailable)?,
              tool: profile.tool,
              plugin: profile.plugin,
              model_or_tool: profile.model_or_tool,
              task_digest: profile.task_digest,
              permission_digest: permissions.digest(),
              build_configuration: profile.build_configuration,
            }))
          }
          (None, None, None, None) => None,
          _ => return Err(StoreError::Unavailable),
        };
        Ok(FactoryRunDiagnosticRecord::Call {
          node: Box::new(node),
          profile,
        })
      },
    )
    .collect()
}

async fn uuid_rows<T>(
  pool: &PgPool,
  query: &'static str,
  request: &ListFactoryRunDiagnostics,
  limit: i64,
  wrap: fn(T) -> FactoryRunDiagnosticRecord,
) -> Result<Vec<FactoryRunDiagnosticRecord>, StoreError>
where
  T: DeserializeOwned + Send + Unpin + 'static,
{
  let after = request
    .after
    .as_ref()
    .map(|value| uuid::Uuid::from_str(value.as_str()))
    .transpose()
    .map_err(|_| invalid_cursor())?;
  sqlx::query_scalar::<_, Json<T>>(query)
    .bind(request.run_id.as_uuid())
    .bind(after)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(unavailable)
    .map(|rows| rows.into_iter().map(|row| wrap(row.0)).collect())
}

async fn digest_rows<T>(
  pool: &PgPool,
  query: &'static str,
  request: &ListFactoryRunDiagnostics,
  limit: i64,
  wrap: fn(T) -> FactoryRunDiagnosticRecord,
) -> Result<Vec<FactoryRunDiagnosticRecord>, StoreError>
where
  T: DeserializeOwned + Send + Unpin + 'static,
{
  let after = digest_cursor(request)?;
  sqlx::query_scalar::<_, Json<T>>(query)
    .bind(request.run_id.as_uuid())
    .bind(after)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(unavailable)
    .map(|rows| rows.into_iter().map(|row| wrap(row.0)).collect())
}

fn digest_cursor(request: &ListFactoryRunDiagnostics) -> Result<Option<Vec<u8>>, StoreError> {
  request
    .after
    .as_ref()
    .map(|value| {
      FactoryDigest::from_lower_hex(value.as_str())
        .map(|digest| digest.as_bytes().to_vec())
        .map_err(|_| invalid_cursor())
    })
    .transpose()
}

fn page(mut records: Vec<FactoryRunDiagnosticRecord>, limit: usize) -> Result<FactoryRunDiagnosticPage, StoreError> {
  let has_next = records.len() > limit;
  records.truncate(limit);
  let next = has_next.then(|| records.last().expect("positive diagnostic page limit").cursor());
  if records.windows(2).any(|pair| pair[0].cursor() >= pair[1].cursor()) {
    return Err(StoreError::Unavailable);
  }
  Ok(FactoryRunDiagnosticPage { items: records, next })
}

fn invalid_cursor() -> StoreError {
  StoreError::InvalidInput {
    operation: StoreOperation::ListFactoryRunDiagnostics,
    source: StoreInputError::InvalidFactoryRunDiagnosticCursor,
  }
}
