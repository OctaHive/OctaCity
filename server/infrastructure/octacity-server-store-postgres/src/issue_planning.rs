//! Planning persistence reuses Issue revisions and the management evidence transaction.
use crate::{
  PostgresStore,
  database::{number, unavailable},
  issue_workflow,
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};
use octacity_server_domain::{EntityKind, IssueId, ProjectId, Timestamp};
use octacity_server_store::*;
use serde_json::json;
use sqlx::{PgConnection, types::Json};

fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Issue,
  }
}

pub(super) async fn read(
  connection: &mut PgConnection,
  project_id: ProjectId,
  version: Option<u64>,
) -> Result<PublishedIssueProperties, StoreError> {
  let template = issue_workflow::project_template(connection, project_id, false).await?;
  if version.is_some_and(|v| v > i64::MAX as u64) {
    return Err(StoreError::NotFound {
      entity: EntityKind::Project,
    });
  }
  if version != Some(0) {
    let row: Option<(i64, Json<IssuePropertyDefinitionSet>, i64)> = sqlx::query_as(
      "SELECT version,definition,published_at_millis FROM issue_property_versions WHERE project_id=$1 AND ($2::bigint IS NULL OR version=$2) ORDER BY version DESC LIMIT 1")
      .bind(project_id.as_uuid()).bind(version.map(|v|number(v,StoreOperation::PublishIssueProperties)).transpose()?)
      .fetch_optional(&mut *connection).await.map_err(unavailable)?;
    if let Some((version, Json(definition), at)) = row {
      definition.validate().map_err(|_| StoreError::Unavailable)?;
      return Ok(PublishedIssueProperties {
        project_id,
        version: u64::try_from(version).map_err(|_| StoreError::Unavailable)?,
        definition,
        published_at: Timestamp::from_unix_millis(at).map_err(|_| StoreError::Unavailable)?,
      });
    }
    if version.is_some() {
      return Err(StoreError::NotFound {
        entity: EntityKind::Project,
      });
    }
  }
  Ok(PublishedIssueProperties {
    project_id,
    version: 0,
    definition: IssuePropertyDefinitionSet::default(),
    published_at: template.published_at,
  })
}

#[async_trait::async_trait]
impl IssuePropertyStore for PostgresStore {
  async fn issue_properties(
    &self,
    project_id: ProjectId,
    version: Option<u64>,
  ) -> Result<PublishedIssueProperties, StoreError> {
    read(
      &mut *self.pool().acquire().await.map_err(unavailable)?,
      project_id,
      version,
    )
    .await
  }
  async fn publish_issue_properties(
    &self,
    request: ManagementMutation<PublishIssueProperties>,
  ) -> Result<IssuePropertiesOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let i = &request.intent;
    let identity = MutationIdentity::new_management(
      &audit,
      MutationKind::PublishIssueProperties,
      request.idempotency_key.to_string(),
      request.published_at,
      EntityKind::Issue,
      i,
    )?;
    let mut tx = match crate::mutation::begin(self.pool(), &identity).await? {
      MutationStart::Replay(value) => {
        let mut outcome: IssuePropertiesOutcome = decode_outcome(value)?;
        outcome.disposition = MutationDisposition::Replayed;
        return Ok(outcome);
      }
      MutationStart::Fresh(tx) => tx,
    };
    issue_workflow::lock_project(&mut tx, i.project_id).await?;
    let current = read(&mut tx, i.project_id, None).await?;
    if current.version != i.expected_version {
      return Err(conflict());
    }
    i.definition.validate_replacement(&current.definition)?;
    if request.published_at < current.published_at {
      return Err(StoreError::InvalidInput {
        operation: StoreOperation::PublishIssueProperties,
        source: StoreInputError::InvalidIssue,
      });
    }
    let resource = PublishedIssueProperties {
      project_id: i.project_id,
      version: current
        .version
        .checked_add(1)
        .filter(|v| *v <= i64::MAX as u64)
        .ok_or_else(conflict)?,
      definition: i.definition.clone(),
      published_at: request.published_at,
    };
    sqlx::query(
      "INSERT INTO issue_property_versions(project_id,version,definition,published_at_millis) VALUES ($1,$2,$3,$4)",
    )
    .bind(i.project_id.as_uuid())
    .bind(number(resource.version, StoreOperation::PublishIssueProperties)?)
    .bind(Json(&resource.definition))
    .bind(resource.published_at.unix_millis())
    .execute(&mut *tx)
    .await
    .map_err(unavailable)?;
    let metadata = json!({"project_id":i.project_id,"version":resource.version});
    let outcome = IssuePropertiesOutcome {
      disposition: MutationDisposition::Applied,
      resource,
    };
    crate::mutation::commit(
      tx,
      &identity,
      MutationFacts::management(&audit, i.project_id.to_string(), metadata.clone(), metadata),
      encode_outcome(&outcome)?,
    )
    .await?;
    Ok(outcome)
  }
  async fn edit_issue_planning(
    &self,
    request: ManagementMutation<EditIssuePlanning>,
  ) -> Result<IssueMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let i = &request.intent;
    let identity = MutationIdentity::new_management(
      &audit,
      MutationKind::EditIssuePlanning,
      request.idempotency_key.to_string(),
      request.edited_at,
      EntityKind::Issue,
      i,
    )?;
    let mut tx = match crate::mutation::begin(self.pool(), &identity).await? {
      MutationStart::Replay(value) => {
        let mut outcome: IssueMutationOutcome = decode_outcome(value)?;
        outcome.disposition = MutationDisposition::Replayed;
        return Ok(outcome);
      }
      MutationStart::Fresh(tx) => tx,
    };
    // Policy publication and relationship writes share the Project fence, taken
    // before the Issue head. All validation uses this one transaction/connection.
    issue_workflow::lock_project(&mut tx, i.project_id).await?;
    let current = crate::issue::lock(&mut tx, i.project_id, i.id).await?;
    validate_parent(&mut tx, i.project_id, i.id, i.planning.parent_id).await?;
    let catalog = read(&mut tx, i.project_id, None).await?;
    let policy = crate::issue::field_policy(&mut tx, i.project_id, i.id).await?;
    let issue = current.planned(&request, &policy, &catalog)?;
    let outcome = IssueMutationOutcome {
      disposition: MutationDisposition::Applied,
      issue,
    };
    crate::issue::publish(tx, &identity, &audit, &outcome.issue, None, &outcome, None).await?;
    Ok(outcome)
  }
  async fn issue_children(&self, request: ListProjectIssues, parent_id: IssueId) -> Result<IssuePage, StoreError> {
    self.issue(request.project_id(), parent_id, None).await?;
    let visible = crate::read_visibility::sql_read_visibility(request.visibility().view(), |id| id.as_uuid());
    let mut connection = self.pool().acquire().await.map_err(unavailable)?;
    if let Some(after) = request.after() {
      let valid: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM issues i JOIN issue_revisions r \
         ON r.project_id=i.project_id AND r.issue_id=i.id AND r.version=i.current_version \
         WHERE i.project_id=$1 AND i.id=$2 AND r.parent_id=$3)",
      )
      .bind(request.project_id().as_uuid())
      .bind(after.as_uuid())
      .bind(parent_id.as_uuid())
      .fetch_one(&mut *connection)
      .await
      .map_err(unavailable)?;
      if !valid {
        return Err(StoreError::InvalidInput {
          operation: StoreOperation::ListProjectIssues,
          source: StoreInputError::InvalidIssue,
        });
      }
    }
    let rows: Vec<(Json<IssueRevision>,)> = sqlx::query_as(
      "SELECT r.document FROM issues i JOIN issue_revisions r \
       ON r.project_id=i.project_id AND r.issue_id=i.id AND r.version=i.current_version \
       WHERE i.project_id=$1 AND r.parent_id=$2 \
       AND ($3 OR i.id=ANY($4::uuid[])) AND ($5::uuid IS NULL OR i.id>$5) ORDER BY i.id LIMIT $6",
    )
    .bind(request.project_id().as_uuid())
    .bind(parent_id.as_uuid())
    .bind(visible.all)
    .bind(visible.identities)
    .bind(request.after().map(IssueId::as_uuid))
    .bind(i64::from(request.limit()) + 1)
    .fetch_all(&mut *connection)
    .await
    .map_err(unavailable)?;
    Ok(crate::issue::page(rows, request.limit()))
  }
}

// The caller holds the Project fence before reading ancestry and publishing a
// revision. UNION terminates even if stored ancestry was damaged out of band.
pub(super) async fn validate_parent(
  connection: &mut PgConnection,
  project_id: ProjectId,
  id: IssueId,
  parent: Option<IssueId>,
) -> Result<(), StoreError> {
  let Some(parent) = parent else {
    return Ok(());
  };
  if parent == id {
    return Err(conflict());
  }
  let ancestors: Vec<uuid::Uuid> = sqlx::query_scalar(
    "WITH RECURSIVE ancestors(id,parent_id) AS ( \
       SELECT i.id,r.parent_id FROM issues i \
       JOIN issue_revisions r ON r.project_id=i.project_id AND r.issue_id=i.id AND r.version=i.current_version \
       WHERE i.project_id=$1 AND i.id=$2 \
       UNION \
       SELECT i.id,r.parent_id FROM issues i \
       JOIN issue_revisions r ON r.project_id=i.project_id AND r.issue_id=i.id AND r.version=i.current_version \
       JOIN ancestors a ON a.parent_id=i.id WHERE i.project_id=$1 \
     ) SELECT id FROM ancestors",
  )
  .bind(project_id.as_uuid())
  .bind(parent.as_uuid())
  .fetch_all(connection)
  .await
  .map_err(unavailable)?;
  if ancestors.is_empty() {
    return Err(StoreError::NotFound {
      entity: EntityKind::Issue,
    });
  }
  if ancestors.contains(&id.as_uuid()) {
    return Err(conflict());
  }
  Ok(())
}

// Only accepted mutations materialize version zero for revision foreign keys.
pub(super) async fn retain_initial(connection: &mut PgConnection, project_id: ProjectId) -> Result<(), StoreError> {
  sqlx::query(
    "INSERT INTO issue_property_versions(project_id,version,definition,published_at_millis) \
    SELECT id,0,'{\"labels\":[],\"properties\":[]}'::jsonb,FLOOR(EXTRACT(EPOCH FROM created_at)*1000)::bigint \
    FROM projects WHERE id=$1 ON CONFLICT DO NOTHING",
  )
  .bind(project_id.as_uuid())
  .execute(connection)
  .await
  .map_err(unavailable)?;
  Ok(())
}
