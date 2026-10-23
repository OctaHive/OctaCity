//! Exact private package recovery under the existing node fence and retention owner.
use crate::{PostgresStore, database::unavailable};
use octacity_server_domain::{EntityKind, Timestamp};
use octacity_server_factory::{FactoryClaimOwnership, FactoryDigest, FactoryKey, FlowBuildIntent};
use octacity_server_store::{AuditActorKind, FactoryAuditFact, RetainedFactoryRoleInvocation, StoreError};
use sqlx::{Postgres, types::Json};

fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Build,
  }
}
async fn read<'e, E: sqlx::Executor<'e, Database = Postgres>>(
  executor: E,
  operation: FactoryDigest,
) -> Result<Option<RetainedFactoryRoleInvocation>, StoreError> {
  let row: Option<(Json<RetainedFactoryRoleInvocation>, Vec<u8>)> =
    sqlx::query_as("SELECT document,digest FROM factory_role_execution_packages WHERE operation_id=$1")
      .bind(operation.as_bytes().as_slice())
      .fetch_optional(executor)
      .await
      .map_err(unavailable)?;
  row
    .map(|(Json(document), expected)| {
      if digest(&document)?.as_bytes().as_slice() != expected {
        return Err(StoreError::Unavailable);
      }
      Ok(document)
    })
    .transpose()
}
fn digest(document: &RetainedFactoryRoleInvocation) -> Result<FactoryDigest, StoreError> {
  let bytes = serde_json::to_vec(document).map_err(|_| StoreError::Unavailable)?;
  Ok(FactoryDigest::sha256("octacity.factory.role-invocation.v1", &[&bytes]))
}
pub(crate) async fn recover(
  store: &PostgresStore,
  operation: FactoryDigest,
) -> Result<Option<RetainedFactoryRoleInvocation>, StoreError> {
  read(store.pool(), operation).await
}
pub(crate) async fn retain(
  store: &PostgresStore,
  intent: &FlowBuildIntent,
  ownership: &FactoryClaimOwnership,
  document: RetainedFactoryRoleInvocation,
  now: Timestamp,
) -> Result<(), StoreError> {
  document.verify_intent(intent)?;
  let operation = intent.operation_id().map_err(|_| conflict())?;
  if let Some(previous) = read(store.pool(), operation).await? {
    return if previous == document { Ok(()) } else { Err(conflict()) };
  }
  let mut tx = store.pool().begin().await.map_err(unavailable)?;
  crate::factory_run::verify_node_dispatch(&mut tx, intent, ownership, now).await?;
  if let Some(previous) = read(&mut *tx, operation).await? {
    return if previous == document { Ok(()) } else { Err(conflict()) };
  }
  let selected = document.package.inputs();
  let role = selected.role.reference();
  let profile = selected.profile.reference();
  if crate::factory_role_profiles::read_role(&mut *tx, role.project_id, role.id, Some(role.version)).await?
    != selected.role
    || crate::factory_role_profiles::read_profile(&mut *tx, profile.project_id, profile.id, Some(profile.version))
      .await?
      != selected.profile
  {
    return Err(conflict());
  }
  let digest = digest(&document)?;
  sqlx::query("INSERT INTO factory_role_execution_packages(operation_id,run_id,role_id,role_version,profile_id,profile_version,document,digest) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
    .bind(operation.as_bytes().as_slice()).bind(intent.input().factory_run_id().as_uuid())
    .bind(role.id.as_uuid()).bind(i64::try_from(role.version.get()).map_err(|_| conflict())?).bind(profile.id.as_uuid()).bind(i64::try_from(profile.version.get()).map_err(|_| conflict())?).bind(Json(document)).bind(digest.as_bytes().as_slice())
    .execute(&mut *tx).await.map_err(unavailable)?;
  crate::factory_run::insert_audit(
    &mut tx,
    &FactoryAuditFact::new(
      intent.input().factory_run_id(),
      AuditActorKind::Worker,
      None,
      FactoryKey::new("factory.role_package_retained").map_err(|_| conflict())?,
      digest,
      FactoryKey::new("accepted").map_err(|_| conflict())?,
      now,
    ),
  )
  .await?;
  tx.commit().await.map_err(unavailable)
}
