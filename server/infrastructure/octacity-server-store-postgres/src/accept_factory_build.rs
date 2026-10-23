//! Common node admission reuses ordinary Build storage and signed ready queues.

use crate::database::{number, unavailable};
use octacity_server_domain::{AttemptId, BuildId, EntityKind, JobId};
use octacity_server_factory::{FactoryDigest, FactoryKey};
use octacity_server_job::JobSpecSigner;
use octacity_server_store::{
  AcceptFactoryBuild, AcceptedFactoryBuild, AuditActorKind, FactoryAuditFact, FactoryBuildOrigin, StoreError,
  StoreOperation,
};
use sqlx::{PgPool, Postgres, Transaction};

type AcceptanceRow = (uuid::Uuid, uuid::Uuid, Vec<uuid::Uuid>, Vec<u8>, Vec<u8>);

async fn lookup<'e, E: sqlx::Executor<'e, Database = Postgres>>(
  executor: E,
  operation: FactoryDigest,
) -> Result<Option<(AcceptedFactoryBuild, Vec<u8>)>, StoreError> {
  let row: Option<AcceptanceRow> = sqlx::query_as(
    "SELECT build.id, attempt.id, ARRAY(SELECT job.id FROM jobs job WHERE job.attempt_id=attempt.id ORDER BY job.id), \
       build.factory_permission_digest, build.factory_materialization_digest \
     FROM builds build JOIN attempts attempt ON attempt.build_id=build.id AND attempt.attempt_number=1 \
     WHERE build.factory_origin IS NOT NULL AND build.factory_origin->>'operation_id'=$1",
  )
  .bind(operation.to_string())
  .fetch_optional(executor)
  .await
  .map_err(unavailable)?;
  row
    .map(|(build, attempt, jobs, permissions, digest)| {
      Ok((
        AcceptedFactoryBuild {
          build_id: BuildId::from_uuid(build).map_err(|_| StoreError::Unavailable)?,
          attempt_id: AttemptId::from_uuid(attempt).map_err(|_| StoreError::Unavailable)?,
          job_ids: jobs
            .into_iter()
            .map(|id| JobId::from_uuid(id).map_err(|_| StoreError::Unavailable))
            .collect::<Result<_, _>>()?,
          effective_policy_digest: FactoryDigest::from_bytes(
            permissions.try_into().map_err(|_| StoreError::Unavailable)?,
          ),
        },
        digest,
      ))
    })
    .transpose()
}

pub(crate) async fn read(pool: &PgPool, operation: FactoryDigest) -> Result<Option<AcceptedFactoryBuild>, StoreError> {
  Ok(lookup(pool, operation).await?.map(|(accepted, _)| accepted))
}

pub(crate) async fn accept(
  pool: &PgPool,
  signer: &JobSpecSigner,
  request: AcceptFactoryBuild,
) -> Result<AcceptedFactoryBuild, StoreError> {
  request.validate()?;
  let operation = request.intent.operation_id().map_err(|_| conflict())?;
  // Owner/fence/time are mutable dispatch authority, never logical operation identity.
  let bytes = serde_json::to_vec(&(&request.intent, &request.build, request.attempt_id, &request.jobs))
    .map_err(|_| StoreError::Unavailable)?;
  let materialization = FactoryDigest::sha256("octacity.factory.build-materialization.v1", &[&bytes]);
  if let Some((accepted, retained)) = lookup(pool, operation).await? {
    return identical(accepted, retained, materialization);
  }
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  crate::factory_run::verify_build_dispatch(&mut transaction, &request).await?;
  // The Run lock serializes competing owners of this exact persisted operation.
  if let Some((accepted, retained)) = lookup(&mut *transaction, operation).await? {
    return identical(accepted, retained, materialization);
  }
  request.validate_fresh()?;
  require_references(&mut transaction, &request).await?;
  crate::factory_role_package::require_capacity(&mut transaction, &request).await?;
  let configuration = number(
    request.build.configuration_version.get(),
    StoreOperation::AcceptFactoryBuild,
  )?;
  let pipeline = number(request.build.pipeline_version.get(), StoreOperation::AcceptFactoryBuild)?;
  let repository = number(
    request.build.repository_version.get(),
    StoreOperation::AcceptFactoryBuild,
  )?;
  crate::build_materialization::insert_build(
    &mut transaction,
    &request.build,
    crate::build_materialization::BuildInsertionOrigin::Factory {
      origin: FactoryBuildOrigin::from_intent(&request.intent).map_err(|_| conflict())?,
      materialization_digest: materialization,
      permission_digest: request.intent.permissions().digest(),
    },
    request.accepted_at,
    configuration,
    pipeline,
    repository,
  )
  .await?;
  crate::attempt_materialization::insert_attempt(
    &mut transaction,
    request.attempt_id,
    request.build.id,
    1,
    None,
    octacity_server_orchestrator::AttemptState::Running,
    request.accepted_at,
  )
  .await?;
  crate::attempt_materialization::insert_jobs(&mut transaction, request.attempt_id, &request.jobs, request.accepted_at)
    .await?;
  crate::attempt_materialization::insert_dependencies(&mut transaction, request.attempt_id, &request.jobs).await?;
  crate::attempt_materialization::enqueue_roots(
    &mut transaction,
    signer,
    request.attempt_id,
    &request.jobs,
    request.accepted_at,
  )
  .await?;
  crate::factory_run::insert_audit(
    &mut transaction,
    &FactoryAuditFact::new(
      request.intent.input().factory_run_id(),
      AuditActorKind::Worker,
      Some(FactoryDigest::sha256(
        "octacity.factory.audit-worker.v1",
        &[request.ownership.owner().as_str().as_bytes()],
      )),
      FactoryKey::new("factory.build.accepted").map_err(|_| StoreError::Unavailable)?,
      operation,
      FactoryKey::new("accepted").map_err(|_| StoreError::Unavailable)?,
      request.accepted_at,
    ),
  )
  .await?;
  transaction.commit().await.map_err(unavailable)?;
  Ok(AcceptedFactoryBuild {
    build_id: request.build.id,
    attempt_id: request.attempt_id,
    job_ids: request.jobs.iter().map(|job| job.id).collect(),
    effective_policy_digest: request.intent.permissions().digest(),
  })
}

fn identical(
  accepted: AcceptedFactoryBuild,
  retained: Vec<u8>,
  expected: FactoryDigest,
) -> Result<AcceptedFactoryBuild, StoreError> {
  if retained != expected.as_bytes() {
    return Err(conflict());
  }
  Ok(accepted)
}
fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Build,
  }
}

async fn require_references(
  tx: &mut Transaction<'_, Postgres>,
  request: &AcceptFactoryBuild,
) -> Result<(), StoreError> {
  let build = &request.build;
  let exists: bool = sqlx::query_scalar(
    "SELECT EXISTS (SELECT 1 FROM build_configuration_versions version JOIN build_configurations configuration \
       ON configuration.id=version.build_configuration_id WHERE configuration.id=$1 AND configuration.project_id=$2 \
       AND version.version=$3 AND version.enabled AND version.repository_id=$4 AND version.repository_version=$5 \
       AND version.pipeline_id=$6 AND version.pipeline_version=$7 AND $8::uuid[] <@ version.allowed_pool_ids)",
  )
  .bind(build.configuration_id.as_uuid())
  .bind(build.project_id.as_uuid())
  .bind(number(
    build.configuration_version.get(),
    StoreOperation::AcceptFactoryBuild,
  )?)
  .bind(build.repository_id.as_uuid())
  .bind(number(
    build.repository_version.get(),
    StoreOperation::AcceptFactoryBuild,
  )?)
  .bind(build.pipeline_id.as_uuid())
  .bind(number(
    build.pipeline_version.get(),
    StoreOperation::AcceptFactoryBuild,
  )?)
  .bind(
    request
      .jobs
      .iter()
      .flat_map(|job| job.allowed_pools.iter().map(|pool| pool.as_uuid()))
      .collect::<Vec<_>>(),
  )
  .fetch_one(&mut **tx)
  .await
  .map_err(unavailable)?;
  if !exists {
    return Err(StoreError::NotFound {
      entity: EntityKind::Configuration,
    });
  }
  crate::build_materialization::require_pools(tx, &request.jobs).await
}
