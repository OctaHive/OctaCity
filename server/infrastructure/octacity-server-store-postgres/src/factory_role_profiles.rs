//! Atomic catalog publication through the ordinary scoped mutation transaction.
//!
//! Replay is resolved before mutable heads. Definitions, heads, receipt, audit
//! and outbox commit together: interruption before commit leaves no reservation;
//! after commit a new process can recover the original result with the same key.
//! Exact reads use core deserialization to revalidate content digests and bounds.
use async_trait::async_trait;
use octacity_server_domain::{EntityKind, ProjectId, Timestamp};
use octacity_server_factory::*;
use octacity_server_store::*;
use serde_json::{Value, json};
use sqlx::types::Json;

use crate::{
  PostgresStore,
  database::{classify, unavailable},
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};

#[async_trait]
impl FactoryRoleProfileStore for PostgresStore {
  async fn replay_factory_role_profiles(
    &self,
    request: ManagementMutation<ReplayFactoryRoleProfiles>,
  ) -> Result<Option<FactoryRoleProfilesOutcome>, StoreError> {
    let (request, audit) = request.into_parts();
    let identity = identity(
      &audit,
      &request.intent,
      &request.idempotency_key,
      Timestamp::from_unix_millis(0).map_err(|_| StoreError::Unavailable)?,
    )?;
    let stored: Option<(Vec<u8>, Json<Value>)> = sqlx::query_as(
      "SELECT request_digest, outcome FROM idempotency_records WHERE scope=$1 AND security_scope=$2 AND idempotency_key=$3",
    ).bind(identity.kind.scope()).bind(identity.persisted_security_scope()).bind(&identity.key)
      .fetch_optional(self.pool()).await.map_err(unavailable)?;
    match stored {
      None => Ok(None),
      Some((digest, Json(outcome))) if digest == identity.request_digest => Ok(Some(replay(outcome)?)),
      Some(_) => Err(StoreError::Conflict {
        entity: EntityKind::FactoryRole,
      }),
    }
  }

  async fn publish_factory_role_profiles(
    &self,
    request: ManagementMutation<PublishFactoryRoleProfiles>,
  ) -> Result<FactoryRoleProfilesOutcome, FactoryRoleProfilePublicationError> {
    let (request, audit) = request.into_parts();
    let identity = identity(&audit, &request.intent, &request.idempotency_key, request.published_at)?;
    let mut transaction = match crate::mutation::begin(self.pool(), &identity).await? {
      MutationStart::Fresh(transaction) => transaction,
      MutationStart::Replay(outcome) => return Ok(replay(outcome)?),
    };
    request.intent.validate()?;
    let project = request.intent.project_id;
    // One Project lock covers the bounded multi-identity publication. It also
    // serializes concurrent create/replace without order-dependent row locks.
    sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM projects WHERE id=$1 FOR UPDATE")
      .bind(project.as_uuid())
      .fetch_optional(&mut *transaction)
      .await
      .map_err(unavailable)?
      .ok_or_else(|| missing(EntityKind::Project))?;
    let role = if let Some(intent) = &request.intent.role {
      let head: Option<(uuid::Uuid, i64)> =
        sqlx::query_as("SELECT project_id, current_version FROM factory_roles WHERE id=$1")
          .bind(intent.id.as_uuid())
          .fetch_optional(&mut *transaction)
          .await
          .map_err(unavailable)?;
      let current = check_head(
        head,
        project,
        intent.expected_current_version.map(|v| v.get()),
        EntityKind::FactoryRole,
      )?;
      let role = if let Some(current) = current {
        let previous = read_role(&mut *transaction, project, intent.id, None).await?;
        previous.replace(
          FactoryRoleVersion::new(current.checked_add(1).ok_or_else(invalid)?).map_err(|_| invalid())?,
          intent.definition.clone(),
        )
      } else {
        FactoryRole::publish(intent.id, project, intent.definition.clone())
      }
      .map_err(|_| invalid())?;
      if current.is_none() {
        sqlx::query("INSERT INTO factory_roles (id, project_id, current_version) VALUES ($1,$2,1)")
          .bind(intent.id.as_uuid())
          .bind(project.as_uuid())
          .execute(&mut *transaction)
          .await
          .map_err(|e| classify(e, EntityKind::FactoryRole))?;
      } else {
        sqlx::query("UPDATE factory_roles SET current_version=$2 WHERE id=$1")
          .bind(intent.id.as_uuid())
          .bind(number(role.reference().version.get())?)
          .execute(&mut *transaction)
          .await
          .map_err(unavailable)?;
      }
      sqlx::query("INSERT INTO factory_role_versions (id, project_id, version, definition) VALUES ($1,$2,$3,$4)")
        .bind(intent.id.as_uuid())
        .bind(project.as_uuid())
        .bind(number(role.reference().version.get())?)
        .bind(Json(&role))
        .execute(&mut *transaction)
        .await
        .map_err(unavailable)?;
      Some(role)
    } else {
      None
    };
    let mut profiles = vec![];
    for intent in &request.intent.profiles {
      let head: Option<(uuid::Uuid, i64)> =
        sqlx::query_as("SELECT project_id, current_version FROM factory_role_profiles WHERE id=$1")
          .bind(intent.id.as_uuid())
          .fetch_optional(&mut *transaction)
          .await
          .map_err(unavailable)?;
      let current = check_head(
        head,
        project,
        intent.expected_current_version.map(|v| v.get()),
        EntityKind::FactoryRoleProfile,
      )?;
      let exact = read_role(&mut *transaction, project, intent.role.id, Some(intent.role.version)).await?;
      if exact.reference() != &intent.role {
        return Err(missing(EntityKind::FactoryRole).into());
      }
      let profile = if let Some(current) = current {
        let previous = read_profile(&mut *transaction, project, intent.id, None).await?;
        previous.replace(
          FactoryRoleProfileVersion::new(current.checked_add(1).ok_or_else(invalid)?).map_err(|_| invalid())?,
          &exact,
          intent.definition.clone(),
        )
      } else {
        FactoryRoleProfile::publish(intent.id, project, &exact, intent.definition.clone())
      }
      .map_err(|_| invalid())?;
      if current.is_none() {
        sqlx::query("INSERT INTO factory_role_profiles (id, project_id, current_version) VALUES ($1,$2,1)")
          .bind(intent.id.as_uuid())
          .bind(project.as_uuid())
          .execute(&mut *transaction)
          .await
          .map_err(|e| classify(e, EntityKind::FactoryRoleProfile))?;
      } else {
        sqlx::query("UPDATE factory_role_profiles SET current_version=$2 WHERE id=$1")
          .bind(intent.id.as_uuid())
          .bind(number(profile.reference().version.get())?)
          .execute(&mut *transaction)
          .await
          .map_err(unavailable)?;
      }
      sqlx::query("INSERT INTO factory_role_profile_versions (id, project_id, version, role_id, role_version, definition) VALUES ($1,$2,$3,$4,$5,$6)")
        .bind(intent.id.as_uuid()).bind(project.as_uuid()).bind(number(profile.reference().version.get())?).bind(intent.role.id.as_uuid()).bind(number(intent.role.version.get())?)
        .bind(Json(&profile)).execute(&mut *transaction).await.map_err(unavailable)?;
      profiles.push(profile);
    }
    let outcome = FactoryRoleProfilesOutcome {
      disposition: MutationDisposition::Applied,
      role,
      profiles,
      published_at: request.published_at,
    };
    crate::mutation::commit(transaction, &identity, MutationFacts::management(
      &audit, project.to_string(), json!({"profile_count": outcome.profiles.len(), "role_published": outcome.role.is_some()}),
      json!({"project_id": project, "role": outcome.role.as_ref().map(FactoryRole::reference), "profiles": outcome.profiles.iter().map(FactoryRoleProfile::reference).collect::<Vec<_>>()}),
    ), encode_outcome(&outcome)?).await?;
    Ok(outcome)
  }

  async fn factory_role(
    &self,
    project: ProjectId,
    id: FactoryRoleId,
    version: Option<FactoryRoleVersion>,
  ) -> Result<FactoryRole, StoreError> {
    read_role(self.pool(), project, id, version).await
  }
  async fn factory_role_profile(
    &self,
    project: ProjectId,
    id: FactoryRoleProfileId,
    version: Option<FactoryRoleProfileVersion>,
  ) -> Result<FactoryRoleProfile, StoreError> {
    read_profile(self.pool(), project, id, version).await
  }
}

async fn read_role<'e>(
  executor: impl sqlx::PgExecutor<'e>,
  project: ProjectId,
  id: FactoryRoleId,
  version: Option<FactoryRoleVersion>,
) -> Result<FactoryRole, StoreError> {
  let (stored_version, Json(role)): (i64, Json<FactoryRole>) = sqlx::query_as(
    "SELECT v.version, v.definition FROM factory_roles h JOIN factory_role_versions v ON v.id=h.id AND v.project_id=h.project_id WHERE h.id=$1 AND h.project_id=$2 AND v.version=COALESCE($3,h.current_version)",
  ).bind(id.as_uuid()).bind(project.as_uuid()).bind(version.map(|v| number(v.get())).transpose()?).fetch_optional(executor).await.map_err(unavailable)?.ok_or_else(|| missing(EntityKind::FactoryRole))?;
  if role.reference().id != id
    || role.reference().project_id != project
    || number(role.reference().version.get())? != stored_version
  {
    return Err(StoreError::Unavailable);
  }
  Ok(role)
}
async fn read_profile<'e>(
  executor: impl sqlx::PgExecutor<'e>,
  project: ProjectId,
  id: FactoryRoleProfileId,
  version: Option<FactoryRoleProfileVersion>,
) -> Result<FactoryRoleProfile, StoreError> {
  let (stored_version, role_id, role_version, Json(profile)): (i64, uuid::Uuid, i64, Json<FactoryRoleProfile>) = sqlx::query_as(
    "SELECT v.version, v.role_id, v.role_version, v.definition FROM factory_role_profiles h JOIN factory_role_profile_versions v ON v.id=h.id AND v.project_id=h.project_id WHERE h.id=$1 AND h.project_id=$2 AND v.version=COALESCE($3,h.current_version)",
  ).bind(id.as_uuid()).bind(project.as_uuid()).bind(version.map(|v| number(v.get())).transpose()?).fetch_optional(executor).await.map_err(unavailable)?.ok_or_else(|| missing(EntityKind::FactoryRoleProfile))?;
  if profile.reference().id != id
    || profile.reference().project_id != project
    || number(profile.reference().version.get())? != stored_version
    || profile.role().id.as_uuid() != role_id
    || number(profile.role().version.get())? != role_version
  {
    return Err(StoreError::Unavailable);
  }
  Ok(profile)
}
fn identity(
  audit: &MutationAuditContext,
  intent: &FactoryRoleProfilesIntent,
  key: &IdempotencyKey,
  time: Timestamp,
) -> Result<MutationIdentity, StoreError> {
  MutationIdentity::new_management(
    audit,
    MutationKind::PublishFactoryRoleProfiles,
    key.to_string(),
    time,
    EntityKind::FactoryRole,
    intent,
  )
}
fn replay(value: Value) -> Result<FactoryRoleProfilesOutcome, StoreError> {
  let mut outcome: FactoryRoleProfilesOutcome = decode_outcome(value)?;
  outcome.disposition = MutationDisposition::Replayed;
  Ok(outcome)
}
fn missing(entity: EntityKind) -> StoreError {
  StoreError::NotFound { entity }
}
fn invalid() -> StoreError {
  StoreError::InvalidInput {
    operation: StoreOperation::PublishFactoryRoleProfiles,
    source: StoreInputError::InvalidFactoryRoleProfiles,
  }
}
fn number(version: u64) -> Result<i64, StoreError> {
  i64::try_from(version).map_err(|_| invalid())
}

// The same precondition policy applies to both identities; definitions and
// exact role adoption remain in the provider-neutral core constructors.
fn check_head(
  head: Option<(uuid::Uuid, i64)>,
  project: ProjectId,
  expected: Option<u64>,
  entity: EntityKind,
) -> Result<Option<u64>, FactoryRoleProfilePublicationError> {
  let current = match head {
    Some((owner, _)) if owner != project.as_uuid() => return Err(missing(entity).into()),
    Some((_, version)) => Some(u64::try_from(version).map_err(|_| StoreError::Unavailable)?),
    None => None,
  };
  if current.is_some() && expected.is_none() {
    return Err(StoreError::Duplicate { entity }.into());
  }
  if current != expected {
    return Err(FactoryRoleProfilePublicationError::PreconditionFailed);
  }
  Ok(current)
}
