//! Immutable mapping publication shares existing Issue locking, replay and audit/outbox.
use crate::{
  PostgresStore,
  database::unavailable,
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};
use octacity_server_domain::{EntityKind, IssueId, IssueSourceMappingVersion, ProjectId, Timestamp};
use octacity_server_store::*;
use serde_json::json;
use sqlx::{PgConnection, types::Json};

pub(super) async fn read(
  connection: &mut PgConnection,
  project: ProjectId,
  issue: IssueId,
  version: Option<IssueSourceMappingVersion>,
) -> Result<Option<PublishedIssueSourceMapping>, StoreError> {
  let version = version
    .map(|v| i64::try_from(v.get()).map_err(|_| conflict()))
    .transpose()?;
  let row: Option<Json<PublishedIssueSourceMapping>> = sqlx::query_scalar(
    "SELECT document FROM issue_source_mappings WHERE project_id=$1 AND issue_id=$2 AND ($3::bigint IS NULL OR version=$3) ORDER BY version DESC LIMIT 1")
    .bind(project.as_uuid()).bind(issue.as_uuid()).bind(version).fetch_optional(connection).await.map_err(unavailable)?;
  Ok(row.map(|Json(mapping)| mapping))
}
fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Issue,
  }
}
fn replay(value: serde_json::Value) -> Result<IssueSourceMappingOutcome, StoreError> {
  let mut outcome: IssueSourceMappingOutcome = decode_outcome(value)?;
  outcome.disposition = MutationDisposition::Replayed;
  Ok(outcome)
}
#[async_trait::async_trait]
impl IssueSourceMappingStore for PostgresStore {
  async fn issue_source_mapping(
    &self,
    project: ProjectId,
    issue: IssueId,
    scope: &ManagementSecurityScope,
    version: Option<IssueSourceMappingVersion>,
  ) -> Result<Option<PublishedIssueSourceMapping>, StoreError> {
    if self.issue_source_link(project, issue, scope).await?.is_none() {
      return Err(conflict());
    }
    read(
      &mut *self.pool().acquire().await.map_err(unavailable)?,
      project,
      issue,
      version,
    )
    .await
  }
  async fn issue_source_mapping_replay(
    &self,
    scope: &ManagementSecurityScope,
    intent: &PublishIssueSourceMappingIntent,
    key: &IdempotencyKey,
  ) -> Result<Option<IssueSourceMappingOutcome>, StoreError> {
    let identity = MutationIdentity::new_management_replay(
      scope,
      MutationKind::PublishIssueSourceMapping,
      key.to_string(),
      Timestamp::from_unix_millis(0).map_err(|_| StoreError::Unavailable)?,
      EntityKind::Issue,
      intent,
    )?;
    crate::mutation::probe(self.pool(), &identity)
      .await?
      .map(replay)
      .transpose()
  }

  async fn publish_issue_source_mapping(
    &self,
    request: ManagementMutation<PublishIssueSourceMapping>,
  ) -> Result<IssueSourceMappingOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    publish_mapping(self, request, audit, None).await
  }
}

async fn publish_mapping(
  store: &PostgresStore,
  request: PublishIssueSourceMapping,
  audit: MutationAuditContext,
  review: Option<IssueSourceResolutionReview>,
) -> Result<IssueSourceMappingOutcome, StoreError> {
  let identity = if let Some(review) = &review {
    MutationIdentity::new_management(
      &audit,
      MutationKind::ResolveIssueSourceConflict,
      request.idempotency_key.to_string(),
      request.published_at,
      EntityKind::Issue,
      &ResolveIssueSourceConflictIntent {
        mapping: request.intent.clone(),
        review: review.clone(),
      },
    )?
  } else {
    MutationIdentity::new_management(
      &audit,
      MutationKind::PublishIssueSourceMapping,
      request.idempotency_key.to_string(),
      request.published_at,
      EntityKind::Issue,
      &request.intent,
    )?
  };
  let mut tx = match crate::mutation::begin(store.pool(), &identity).await? {
    MutationStart::Replay(value) => return replay(value),
    MutationStart::Fresh(tx) => tx,
  };
  let i = &request.intent;
  let current = crate::issue::lock(&mut tx, i.project_id, i.issue_id).await?;
  let Json(link): Json<IssueSourceLink> = sqlx::query_scalar(
    "SELECT document FROM issue_source_links WHERE project_id=$1 AND issue_id=$2 AND security_scope=$3",
  )
  .bind(i.project_id.as_uuid())
  .bind(i.issue_id.as_uuid())
  .bind(audit.security_scope().as_str())
  .fetch_optional(&mut *tx)
  .await
  .map_err(unavailable)?
  .ok_or_else(conflict)?;
  let previous = read(&mut tx, i.project_id, i.issue_id, None).await?;
  let initial = crate::issue_workflow::project_template(&mut tx, i.project_id, false).await?;
  let workflow =
    crate::issue_workflow::read(&mut tx, &initial, current.workflow_id, Some(current.workflow_version)).await?;
  let mapping = request.resolve(&current, &link, previous.as_ref(), &workflow)?;
  let push = if let Some(review) = &review {
    let original = crate::issue_reporter::read(&mut tx, review.operation_id).await?;
    Some(
      ResolveIssueSourceConflict {
        publication: request.clone(),
        review: review.clone(),
      }
      .push_intent(&current, &link, &mapping, &original, &audit)?,
    )
  } else {
    None
  };
  let Json(mut control): Json<IssueSourceRefreshState> =
    sqlx::query_scalar("SELECT document FROM issue_source_refresh WHERE project_id=$1 AND issue_id=$2 FOR UPDATE")
      .bind(i.project_id.as_uuid())
      .bind(i.issue_id.as_uuid())
      .fetch_one(&mut *tx)
      .await
      .map_err(unavailable)?;
  control.mapping_published()?;
  sqlx::query("UPDATE issue_source_refresh SET document=$3 WHERE project_id=$1 AND issue_id=$2")
    .bind(i.project_id.as_uuid())
    .bind(i.issue_id.as_uuid())
    .bind(Json(control))
    .execute(&mut *tx)
    .await
    .map_err(unavailable)?;
  sqlx::query(
    "INSERT INTO issue_source_mappings(project_id,issue_id,version,document,writer_access) VALUES($1,$2,$3,$4,$5)",
  )
  .bind(i.project_id.as_uuid())
  .bind(i.issue_id.as_uuid())
  .bind(i64::try_from(mapping.version.get()).map_err(|_| conflict())?)
  .bind(Json(&mapping))
  .bind(request.writer_access.as_ref().map(Json))
  .execute(&mut *tx)
  .await
  .map_err(unavailable)?;
  sqlx::query("UPDATE issues SET field_policy=$3 WHERE project_id=$1 AND id=$2")
    .bind(i.project_id.as_uuid())
    .bind(i.issue_id.as_uuid())
    .bind(Json(mapping.field_policy()))
    .execute(&mut *tx)
    .await
    .map_err(unavailable)?;
  let projection = if let Some(intent) = &push {
    Some(crate::issue_reporter::publish(&mut tx, intent, request.published_at).await?)
  } else {
    None
  };
  let outcome = IssueSourceMappingOutcome {
    projection,
    disposition: MutationDisposition::Applied,
    mapping,
  };
  let mut metadata = json!({"project_id":i.project_id,"issue_id":i.issue_id,"mapping":outcome.mapping.reference()});
  if let Some(review) = &review {
    metadata["resolution"] = json!(review);
  }
  let mut payload = metadata.clone();
  if let Some(intent) = &push {
    payload["push_intent"] = json!(intent);
    payload["push_operation"] = json!(projection);
  }
  crate::mutation::commit(
    tx,
    &identity,
    MutationFacts::management(&audit, i.issue_id.to_string(), metadata, payload),
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

#[async_trait::async_trait]
impl IssueSourceResolutionStore for PostgresStore {
  async fn issue_source_resolution_replay(
    &self,
    scope: &ManagementSecurityScope,
    intent: &ResolveIssueSourceConflictIntent,
    key: &IdempotencyKey,
  ) -> Result<Option<IssueSourceMappingOutcome>, StoreError> {
    let identity = MutationIdentity::new_management_replay(
      scope,
      MutationKind::ResolveIssueSourceConflict,
      key.to_string(),
      Timestamp::from_unix_millis(0).map_err(|_| StoreError::Unavailable)?,
      EntityKind::Issue,
      intent,
    )?;
    crate::mutation::probe(self.pool(), &identity)
      .await?
      .map(replay)
      .transpose()
  }
  async fn resolve_issue_source_conflict(
    &self,
    request: ManagementMutation<ResolveIssueSourceConflict>,
  ) -> Result<IssueSourceMappingOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    publish_mapping(self, request.publication, audit, Some(request.review)).await
  }
}

// Private installed authority is not projected in public mapping/choice responses.
pub(super) async fn writer_access(
  connection: &mut PgConnection,
  mapping: &PublishedIssueSourceMapping,
) -> Result<Option<IssueSourceWriterAccess>, StoreError> {
  let pin: Option<Json<IssueSourceWriterAccess>> = sqlx::query_scalar(
    "SELECT writer_access FROM issue_source_mappings WHERE project_id=$1 AND issue_id=$2 AND version=$3",
  )
  .bind(mapping.project_id.as_uuid())
  .bind(mapping.issue_id.as_uuid())
  .bind(mapping.version.get() as i64)
  .fetch_one(connection)
  .await
  .map_err(unavailable)?;
  Ok(pin.map(|Json(pin)| pin))
}
