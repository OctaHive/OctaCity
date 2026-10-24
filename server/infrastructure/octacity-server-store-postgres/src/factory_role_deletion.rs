//! Deletion shares the publication Project lock, scoped replay, audit and outbox transaction.
use crate::{
  PostgresStore,
  database::unavailable,
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};
use octacity_server_domain::EntityKind;
use octacity_server_store::*;
use serde_json::json;

pub(crate) async fn delete(
  store: &PostgresStore,
  request: ManagementMutation<DeleteFactoryRole>,
) -> Result<DeleteFactoryRoleOutcome, FactoryRoleProfilePublicationError> {
  let (request, audit) = request.into_parts();
  let identity = MutationIdentity::new_management(
    &audit,
    MutationKind::DeleteFactoryRole,
    request.idempotency_key.to_string(),
    request.deleted_at,
    EntityKind::FactoryRole,
    &(request.project_id, request.id, request.expected_current_version),
  )?;
  let mut tx = match crate::mutation::begin(store.pool(), &identity).await? {
    MutationStart::Fresh(tx) => tx,
    MutationStart::Replay(value) => {
      let mut outcome: DeleteFactoryRoleOutcome = decode_outcome(value)?;
      outcome.disposition = MutationDisposition::Replayed;
      return Ok(outcome);
    }
  };
  sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM projects WHERE id=$1 FOR UPDATE")
    .bind(request.project_id.as_uuid())
    .fetch_optional(&mut *tx)
    .await
    .map_err(unavailable)?
    .ok_or(StoreError::NotFound {
      entity: EntityKind::Project,
    })?;
  let role = crate::factory_role_profiles::read_role(&mut *tx, request.project_id, request.id, None).await?;
  if role.reference().version != request.expected_current_version {
    return Err(FactoryRoleProfilePublicationError::PreconditionFailed);
  }
  sqlx::query("UPDATE factory_roles SET deleted=TRUE WHERE id=$1 AND project_id=$2")
    .bind(request.id.as_uuid())
    .bind(request.project_id.as_uuid())
    .execute(&mut *tx)
    .await
    .map_err(unavailable)?;
  let outcome = DeleteFactoryRoleOutcome {
    disposition: MutationDisposition::Applied,
    role: role.reference().clone(),
    deleted_at: request.deleted_at,
  };
  crate::mutation::commit(
    tx,
    &identity,
    MutationFacts::management(
      &audit,
      request.project_id.to_string(),
      json!({"role_id": request.id}),
      json!({"project_id": request.project_id, "role": outcome.role}),
    ),
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

/// Serializes new manual/automatic selection with role deletion without touching retained versions.
pub(crate) async fn require_roles_available(
  tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  project: octacity_server_domain::ProjectId,
  roles: &std::collections::BTreeSet<octacity_server_factory::FactoryRoleId>,
) -> Result<(), StoreError> {
  if roles.is_empty() {
    return Ok(());
  }
  sqlx::query("SELECT id FROM projects WHERE id=$1 FOR UPDATE")
    .bind(project.as_uuid())
    .fetch_one(&mut **tx)
    .await
    .map_err(unavailable)?;
  let ids: Vec<_> = roles.iter().map(|id| id.as_uuid()).collect();
  let deleted: bool =
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM factory_roles WHERE project_id=$1 AND id=ANY($2) AND deleted)")
      .bind(project.as_uuid())
      .bind(ids)
      .fetch_one(&mut **tx)
      .await
      .map_err(unavailable)?;
  if deleted {
    return Err(StoreError::Conflict {
      entity: EntityKind::FactoryRoleProfile,
    });
  }
  Ok(())
}
