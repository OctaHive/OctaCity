//! Shared initial ordinary Build persistence, regardless of its initiation source.
use crate::database::{classify, unavailable};
use octacity_server_domain::{EntityKind, Timestamp, TriggerOccurrenceId};
use octacity_server_factory::FactoryDigest;
use octacity_server_store::{FactoryBuildOrigin, ImmutableBuildInput, MaterializedJob, StoreError};
use sqlx::{Postgres, Transaction, types::Json};
use std::collections::BTreeSet;
use uuid::Uuid;

pub(crate) enum BuildInsertionOrigin {
  Trigger(TriggerOccurrenceId),
  Factory {
    origin: FactoryBuildOrigin,
    materialization_digest: FactoryDigest,
    permission_digest: FactoryDigest,
  },
}

pub(crate) async fn insert_build(
  transaction: &mut Transaction<'_, Postgres>,
  build: &ImmutableBuildInput,
  origin: BuildInsertionOrigin,
  accepted_at: Timestamp,
  configuration_version: i64,
  pipeline_version: i64,
  repository_version: i64,
) -> Result<(), StoreError> {
  let (trigger, factory, materialization, permissions) = match origin {
    BuildInsertionOrigin::Trigger(trigger) => (Some(trigger), None, None, None),
    BuildInsertionOrigin::Factory {
      origin,
      materialization_digest,
      permission_digest,
    } => (
      None,
      Some(origin),
      Some(materialization_digest),
      Some(permission_digest),
    ),
  };
  sqlx::query(
    "INSERT INTO builds \
       (id, project_id, build_configuration_id, build_configuration_version, pipeline_id, pipeline_version, \
        repository_id, repository_version, trigger_occurrence_id, immutable_revision, input_snapshot, \
        effective_policy_snapshot, metadata_retention_until, log_retention_until, artifact_retention_until, \
        report_retention_until, project_job_concurrency_limit, priority, state, version, created_at, updated_at, factory_origin, factory_materialization_digest, factory_permission_digest) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, \
             to_timestamp($13::double precision / 1000.0), to_timestamp($14::double precision / 1000.0), \
             to_timestamp($15::double precision / 1000.0), to_timestamp($16::double precision / 1000.0), \
             $17, $18, 'running', 1, to_timestamp($19::double precision / 1000.0), \
             to_timestamp($19::double precision / 1000.0), $20, $21, $22)",
  )
  .bind(build.id.as_uuid())
  .bind(build.project_id.as_uuid())
  .bind(build.configuration_id.as_uuid())
  .bind(configuration_version)
  .bind(build.pipeline_id.as_uuid())
  .bind(pipeline_version)
  .bind(build.repository_id.as_uuid())
  .bind(repository_version)
  .bind(trigger.map(TriggerOccurrenceId::as_uuid))
  .bind(build.immutable_revision.as_str())
  .bind(Json(build.input_snapshot.clone()))
  .bind(Json(build.effective_policy_snapshot.clone()))
  .bind(build.retention.metadata.unix_millis())
  .bind(build.retention.logs.unix_millis())
  .bind(build.retention.artifacts.unix_millis())
  .bind(build.retention.reports.unix_millis())
  .bind(i64::from(build.project_job_concurrency_limit))
  .bind(build.priority)
  .bind(accepted_at.unix_millis())
  .bind(factory.map(Json))
  .bind(materialization.map(|digest| digest.as_bytes().to_vec()))
  .bind(permissions.map(|digest| digest.as_bytes().to_vec()))
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Build))?;

  insert_retention_work(transaction, build).await?;

  Ok(())
}
async fn insert_retention_work(
  transaction: &mut Transaction<'_, Postgres>,
  build: &ImmutableBuildInput,
) -> Result<(), StoreError> {
  const RETENTION_NAMESPACE: Uuid = Uuid::from_u128(0x55c2_366a_1b77_53bf_b710_4e58_3245_5147);
  for (component, deadline) in [
    ("metadata", build.retention.metadata),
    ("logs", build.retention.logs),
    ("artifacts", build.retention.artifacts),
    ("reports", build.retention.reports),
  ] {
    let identity = Uuid::new_v5(&RETENTION_NAMESPACE, format!("{}:{component}", build.id).as_bytes());
    sqlx::query(
      "INSERT INTO retention_work \
         (id, resource_kind, resource_identity, phase, available_at, deadline_at, attempt_count, project_id, build_id) \
       VALUES ($1, $2, $3, 'pending', to_timestamp($4::double precision / 1000.0), \
         to_timestamp($4::double precision / 1000.0), 0, $5, $6)",
    )
    .bind(identity)
    .bind(component)
    .bind(build.id.to_string())
    .bind(deadline.unix_millis())
    .bind(build.project_id.as_uuid())
    .bind(build.id.as_uuid())
    .execute(&mut **transaction)
    .await
    .map_err(|error| classify(error, EntityKind::RetentionWork))?;
  }
  Ok(())
}

pub(crate) async fn require_pools(
  transaction: &mut Transaction<'_, Postgres>,
  jobs: &[MaterializedJob],
) -> Result<(), StoreError> {
  let allowed_pools: BTreeSet<_> = jobs
    .iter()
    .flat_map(|job| job.allowed_pools.iter().map(|pool| pool.as_uuid()))
    .collect();
  let pool_ids: Vec<_> = allowed_pools.iter().copied().collect();
  for pool_id in &allowed_pools {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
      .bind(format!("octacity.agent-pool.{pool_id}"))
      .execute(&mut **transaction)
      .await
      .map_err(unavailable)?;
  }
  let pool_count: i64 = sqlx::query_scalar("SELECT COUNT(DISTINCT id) FROM pools WHERE id = ANY($1)")
    .bind(&pool_ids)
    .fetch_one(&mut **transaction)
    .await
    .map_err(unavailable)?;
  if usize::try_from(pool_count).ok() != Some(allowed_pools.len()) {
    return Err(StoreError::NotFound {
      entity: EntityKind::Pool,
    });
  }
  Ok(())
}
