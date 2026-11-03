//! Exact Project policy history through the shared management mutation transaction.
use crate::{
  PostgresStore,
  database::unavailable,
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};
use octacity_server_domain::{EntityKind, ProjectId};
use octacity_server_store::*;
use serde_json::json;
use sqlx::types::Json;

#[async_trait::async_trait]
impl FactoryProjectPolicyStore for PostgresStore {
  async fn replay_factory_project_policy(
    &self,
    request: ManagementMutation<ReplayFactoryProjectPolicy>,
  ) -> Result<Option<FactoryProjectPolicyOutcome>, StoreError> {
    let (request, audit) = request.into_parts();
    let identity = MutationIdentity::new_management(
      &audit,
      MutationKind::PublishFactoryProjectPolicy,
      request.idempotency_key.to_string(),
      octacity_server_domain::Timestamp::from_unix_millis(0).map_err(|_| StoreError::Unavailable)?,
      octacity_server_domain::EntityKind::Project,
      &request.intent,
    )?;
    let row: Option<(Vec<u8>, Json<serde_json::Value>)> = sqlx::query_as(
      "SELECT request_digest, outcome FROM idempotency_records WHERE scope=$1 AND security_scope=$2 AND idempotency_key=$3"
    ).bind(identity.kind.scope()).bind(identity.persisted_security_scope()).bind(&identity.key).fetch_optional(self.pool()).await.map_err(unavailable)?;
    match row {
      None => Ok(None),
      Some((digest, Json(value))) if digest == identity.request_digest => {
        let mut outcome: FactoryProjectPolicyOutcome = decode_outcome(value)?;
        outcome.disposition = MutationDisposition::Replayed;
        Ok(Some(outcome))
      }
      Some(_) => Err(StoreError::Conflict {
        entity: EntityKind::Project,
      }),
    }
  }

  async fn validate_factory_project_policy(
    &self,
    intent: &FactoryProjectPolicyIntent,
  ) -> Result<(), FactoryProjectPolicyPublicationError> {
    intent.validate()?;
    crate::discovery::require_project(self.pool(), intent.policy.reference().project_id()).await?;
    check_head(self.pool(), intent).await
  }
  async fn factory_legacy_policy_mapping(
    &self,
    project: ProjectId,
    legacy: &FactoryLegacyPolicyReference,
  ) -> Result<Option<FactoryLegacyPolicyMapping>, StoreError> {
    crate::discovery::require_project(self.pool(), project).await?;
    let mapping = crate::factory_project_policy_mapping::read(self.pool(), project, legacy).await?;
    if let Some(mapping) = &mapping
      && read_policy(self.pool(), project, &mapping.target).await?.is_none()
    {
      return Err(StoreError::Unavailable);
    }
    Ok(mapping)
  }
  async fn factory_project_policy_provisioning(
    &self,
    project: ProjectId,
  ) -> Result<Option<FactoryProjectPolicyProvisioning>, StoreError> {
    crate::discovery::require_project(self.pool(), project).await?;
    crate::factory_project_provision::read(self.pool(), project).await
  }
  async fn provision_factory_project_policies(
    &self,
    request: ManagementMutation<ProvisionFactoryProjectPolicies>,
  ) -> Result<ProvisionFactoryProjectPoliciesOutcome, StoreError> {
    crate::factory_project_provision::provision(self, request).await
  }
  async fn list_factory_project_policies(
    &self,
    project: ProjectId,
    after: Option<&FactoryProjectPolicyRef>,
    limit: u16,
  ) -> Result<FactoryProjectPolicyPage, StoreError> {
    if limit == 0 || limit > MAX_CURRENT_DEFINITION_PAGE_SIZE {
      return Err(StoreError::InvalidInput {
        operation: StoreOperation::ListFactoryProjectPolicies,
        source: StoreInputError::InvalidCurrentDefinitionPageSize,
      });
    }
    if after.is_some_and(|reference| reference.project_id() != project) {
      return Err(StoreError::InvalidInput {
        operation: StoreOperation::ListFactoryProjectPolicies,
        source: StoreInputError::InvalidFactoryProjectPolicy,
      });
    }
    crate::discovery::require_project(self.pool(), project).await?;
    // Earlier importers published a separate launch v1. Hide the redundant
    // catalog option while keeping exact reads and explicit default choices.
    let rows: Vec<(String, uuid::Uuid, i64, Json<FactoryProjectPolicyRecord>)> = sqlx::query_as(
      "WITH standard_launch AS (
         SELECT policy.id, (policy.document #> '{policy,definition}') - 'name' AS rules
         FROM factory_project_policy_provisioning AS provisioning
         JOIN factory_project_policies AS policy
           ON policy.project_id=provisioning.project_id AND policy.kind='launch'
           AND policy.id=(provisioning.document #>> '{standard_launch,id}')::uuid
         WHERE provisioning.project_id=$1 ORDER BY policy.version DESC LIMIT 1
       ), launch_default AS (
         SELECT document->'launch_policy' AS reference
         FROM factory_project_policy_defaults WHERE project_id=$1 ORDER BY version DESC LIMIT 1
       )
       SELECT DISTINCT ON (policy.kind,policy.id) policy.kind,policy.id,policy.version,policy.document
       FROM factory_project_policies AS policy
       WHERE policy.project_id=$1 AND ($2::text IS NULL OR (policy.kind,policy.id)>($2,$3::uuid))
         AND NOT (
           policy.kind='launch' AND policy.version=1
           AND (policy.document #> '{policy,reference}') IS DISTINCT FROM (SELECT reference FROM launch_default)
           AND EXISTS (
             SELECT 1 FROM standard_launch AS standard
             WHERE standard.id<>policy.id AND standard.rules=(policy.document #> '{policy,definition}') - 'name'
           )
           AND EXISTS (
             SELECT 1 FROM factory_project_policy_mappings AS mapping
             WHERE mapping.project_id=policy.project_id AND mapping.document #>> '{target,kind}'='launch'
               AND mapping.document #> '{target,reference}'=policy.document #> '{policy,reference}'
           )
         )
       ORDER BY policy.kind,policy.id,policy.version DESC LIMIT $4",
    )
    .bind(project.as_uuid())
    .bind(after.map(kind))
    .bind(after.map(FactoryProjectPolicyRef::id))
    .bind(i64::from(limit) + 1)
    .fetch_all(self.pool())
    .await
    .map_err(unavailable)?;
    let mut items = Vec::with_capacity(rows.len());
    for (stored_kind, id, version, Json(record)) in rows {
      let reference = record.reference();
      if reference.project_id() != project
        || reference.id() != id
        || reference.version() != version as u64
        || kind(&reference) != stored_kind
      {
        return Err(StoreError::Unavailable);
      }
      items.push(FactoryProjectPolicySummary::from(&record));
    }
    let more = items.len() > usize::from(limit);
    items.truncate(usize::from(limit));
    let next_cursor = more.then(|| items.last().expect("positive page limit").reference.clone());
    Ok(FactoryProjectPolicyPage { items, next_cursor })
  }
  async fn factory_project_policy_defaults(
    &self,
    project: ProjectId,
  ) -> Result<Option<FactoryProjectPolicyDefaults>, StoreError> {
    crate::discovery::require_project(self.pool(), project).await?;
    let row: Option<(i64, Json<FactoryProjectPolicyDefaults>)> = sqlx::query_as(
      "SELECT version,document FROM factory_project_policy_defaults WHERE project_id=$1 ORDER BY version DESC LIMIT 1",
    )
    .bind(project.as_uuid())
    .fetch_optional(self.pool())
    .await
    .map_err(unavailable)?;
    let Some((version, Json(defaults))) = row else {
      return Ok(None);
    };
    if defaults.project_id != project || defaults.version != version as u64 || version <= 0 {
      return Err(StoreError::Unavailable);
    }
    for reference in default_references(&defaults) {
      if read_policy(self.pool(), project, &reference).await?.is_none() {
        return Err(StoreError::Unavailable);
      }
    }
    Ok(Some(defaults))
  }

  async fn select_factory_project_policy_defaults(
    &self,
    request: ManagementMutation<SelectFactoryProjectPolicyDefaults>,
  ) -> Result<FactoryProjectPolicyDefaultsOutcome, FactoryProjectPolicyPublicationError> {
    let (request, audit) = request.into_parts();
    let identity = MutationIdentity::new_management(
      &audit,
      MutationKind::SelectFactoryProjectPolicyDefaults,
      request.idempotency_key.to_string(),
      request.published_at,
      EntityKind::Project,
      &request.intent,
    )?;
    let mut tx = match crate::mutation::begin(self.pool(), &identity).await? {
      MutationStart::Replay(value) => {
        let mut outcome: FactoryProjectPolicyDefaultsOutcome = decode_outcome(value)?;
        outcome.disposition = MutationDisposition::Replayed;
        return Ok(outcome);
      }
      MutationStart::Fresh(tx) => tx,
    };
    let defaults = request.intent.defaults()?;
    lock_project(&mut *tx, defaults.project_id).await?;
    let current: Option<i64> = sqlx::query_scalar(
      "SELECT version FROM factory_project_policy_defaults WHERE project_id=$1 ORDER BY version DESC LIMIT 1",
    )
    .bind(defaults.project_id.as_uuid())
    .fetch_optional(&mut *tx)
    .await
    .map_err(unavailable)?;
    if current.map(|v| v as u64) != request.intent.expected_version {
      return Err(FactoryProjectPolicyPublicationError::PreconditionFailed);
    }
    for reference in default_references(&defaults) {
      if read_policy(&mut *tx, defaults.project_id, &reference).await?.is_none() {
        return Err(
          StoreError::NotFound {
            entity: EntityKind::Project,
          }
          .into(),
        );
      }
    }
    sqlx::query("INSERT INTO factory_project_policy_defaults (project_id,version,document) VALUES ($1,$2,$3)")
      .bind(defaults.project_id.as_uuid())
      .bind(defaults.version as i64)
      .bind(Json(&defaults))
      .execute(&mut *tx)
      .await
      .map_err(unavailable)?;
    let metadata = json!({"defaults": defaults});
    let outcome = FactoryProjectPolicyDefaultsOutcome {
      disposition: MutationDisposition::Applied,
      defaults,
      published_at: request.published_at,
    };
    crate::mutation::commit(
      tx,
      &identity,
      MutationFacts::management(
        &audit,
        request.intent.project_id.to_string(),
        metadata.clone(),
        metadata,
      ),
      encode_outcome(&outcome)?,
    )
    .await?;
    Ok(outcome)
  }
  async fn factory_project_policy(
    &self,
    project: ProjectId,
    reference: &FactoryProjectPolicyRef,
  ) -> Result<Option<FactoryProjectPolicyRecord>, StoreError> {
    crate::discovery::require_project(self.pool(), project).await?;
    read_policy(self.pool(), project, reference).await
  }

  async fn publish_factory_project_policy(
    &self,
    request: ManagementMutation<PublishFactoryProjectPolicy>,
  ) -> Result<FactoryProjectPolicyOutcome, FactoryProjectPolicyPublicationError> {
    let (request, audit) = request.into_parts();
    let reference = request.intent.policy.reference();
    let identity = MutationIdentity::new_management(
      &audit,
      MutationKind::PublishFactoryProjectPolicy,
      request.idempotency_key.to_string(),
      request.published_at,
      EntityKind::Project,
      &request.intent,
    )?;
    let mut tx = match crate::mutation::begin(self.pool(), &identity).await? {
      MutationStart::Replay(value) => {
        let mut outcome: FactoryProjectPolicyOutcome = decode_outcome(value)?;
        outcome.disposition = MutationDisposition::Replayed;
        return Ok(outcome);
      }
      MutationStart::Fresh(tx) => tx,
    };
    request.intent.validate()?;
    lock_project(&mut *tx, reference.project_id()).await?;
    check_head(&mut *tx, &request.intent).await?;
    append_policy(&mut *tx, &request.intent.policy).await?;
    let outcome = FactoryProjectPolicyOutcome {
      disposition: MutationDisposition::Applied,
      policy: request.intent.policy,
      published_at: request.published_at,
    };
    let metadata = json!({"reference": reference});
    crate::mutation::commit(
      tx,
      &identity,
      MutationFacts::management(&audit, reference.project_id().to_string(), metadata.clone(), metadata),
      encode_outcome(&outcome)?,
    )
    .await?;
    Ok(outcome)
  }
}

async fn check_head<'e>(
  executor: impl sqlx::PgExecutor<'e>,
  intent: &FactoryProjectPolicyIntent,
) -> Result<(), FactoryProjectPolicyPublicationError> {
  let reference = intent.policy.reference();
  let current: Option<Json<FactoryProjectPolicyRecord>> = sqlx::query_scalar(
    "SELECT document FROM factory_project_policies WHERE project_id=$1 AND kind=$2 AND id=$3 ORDER BY version DESC LIMIT 1"
  ).bind(reference.project_id().as_uuid()).bind(kind(&reference)).bind(reference.id())
    .fetch_optional(executor).await.map_err(unavailable)?;
  if current.map(|Json(value)| value.reference()) != intent.expected_current {
    return Err(FactoryProjectPolicyPublicationError::PreconditionFailed);
  }
  Ok(())
}

fn kind(reference: &FactoryProjectPolicyRef) -> &'static str {
  match reference {
    FactoryProjectPolicyRef::Launch(_) => "launch",
    FactoryProjectPolicyRef::Access(_) => "access",
  }
}

// One append primitive for publication, initialization and exact legacy imports.
pub(super) async fn append_policy<'e>(
  executor: impl sqlx::PgExecutor<'e>,
  policy: &FactoryProjectPolicyRecord,
) -> Result<(), StoreError> {
  let reference = policy.reference();
  sqlx::query("INSERT INTO factory_project_policies(project_id,kind,id,version,document) VALUES ($1,$2,$3,$4,$5)")
    .bind(reference.project_id().as_uuid())
    .bind(kind(&reference))
    .bind(reference.id())
    .bind(reference.version() as i64)
    .bind(Json(policy))
    .execute(executor)
    .await
    .map_err(unavailable)?;
  Ok(())
}

// Shared Project lock serializes first publications, competing heads and default selections.
pub(super) async fn lock_project<'e>(
  executor: impl sqlx::PgExecutor<'e>,
  project: ProjectId,
) -> Result<(), StoreError> {
  sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM projects WHERE id=$1 FOR UPDATE")
    .bind(project.as_uuid())
    .fetch_optional(executor)
    .await
    .map_err(unavailable)?
    .ok_or(StoreError::NotFound {
      entity: EntityKind::Project,
    })?;
  Ok(())
}

pub(super) async fn read_policy<'e>(
  executor: impl sqlx::PgExecutor<'e>,
  project: ProjectId,
  reference: &FactoryProjectPolicyRef,
) -> Result<Option<FactoryProjectPolicyRecord>, StoreError> {
  if reference.project_id() != project || reference.version() > i64::MAX as u64 {
    return Ok(None);
  }
  let row: Option<Json<FactoryProjectPolicyRecord>> = sqlx::query_scalar(
    "SELECT document FROM factory_project_policies WHERE project_id=$1 AND kind=$2 AND id=$3 AND version=$4",
  )
  .bind(project.as_uuid())
  .bind(kind(reference))
  .bind(reference.id())
  .bind(reference.version() as i64)
  .fetch_optional(executor)
  .await
  .map_err(unavailable)?;
  Ok(
    row
      .map(|Json(value)| value)
      .filter(|value| value.reference() == *reference),
  )
}

fn default_references(defaults: &FactoryProjectPolicyDefaults) -> impl Iterator<Item = FactoryProjectPolicyRef> + '_ {
  defaults
    .launch_policy
    .iter()
    .cloned()
    .map(FactoryProjectPolicyRef::Launch)
    .chain(
      defaults
        .access_profile
        .iter()
        .cloned()
        .map(FactoryProjectPolicyRef::Access),
    )
}
