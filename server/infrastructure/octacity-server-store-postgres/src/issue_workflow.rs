//! Project business workflows reuse the management mutation boundary, never Factory execution.
use crate::{
  PostgresStore,
  database::{number, unavailable},
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};
use octacity_server_domain::{
  EntityKind, IssueWorkflowId, IssueWorkflowSelectionVersion, IssueWorkflowVersion, ProjectId, Timestamp,
};
use octacity_server_store::*;
use serde_json::json;
use sqlx::{PgConnection, types::Json};

#[async_trait::async_trait]
impl IssueWorkflowStore for PostgresStore {
  async fn list_issue_workflows(&self, request: ListProjectIssueWorkflows) -> Result<IssueWorkflowPage, StoreError> {
    if request.visibility().kind() == ReadVisibilityKind::None {
      return Ok(IssueWorkflowPage {
        items: vec![],
        next_cursor: None,
      });
    }
    let project = self.project(request.project_id()).await?.project;
    let initial = initial_issue_workflow(project.id, project.created_at)?;
    let visibility = crate::read_visibility::sql_read_visibility(request.visibility().view(), |id| id.as_uuid());
    let mut rows: Vec<(uuid::Uuid, i64, String, i64)> = sqlx::query_as(
      "WITH current AS ( \
        SELECT w.id, v.version, v.definition->>'name' AS name, v.published_at_millis \
        FROM issue_workflows w JOIN issue_workflow_versions v \
          ON v.project_id=w.project_id AND v.workflow_id=w.id AND v.version=w.current_version \
        WHERE w.project_id=$1 \
        UNION ALL SELECT $2::uuid, 1::bigint, $3::text, $4::bigint \
        WHERE NOT EXISTS (SELECT 1 FROM issue_workflows WHERE project_id=$1 AND id=$2) \
      ) SELECT id, version, name, published_at_millis FROM current \
        WHERE ($5 OR id=ANY($6::uuid[])) AND ($7::uuid IS NULL OR id>$7) ORDER BY id LIMIT $8",
    )
    .bind(project.id.as_uuid())
    .bind(initial.id.as_uuid())
    .bind(&initial.definition.name)
    .bind(initial.published_at.unix_millis())
    .bind(visibility.all)
    .bind(visibility.identities)
    .bind(request.after().map(IssueWorkflowId::as_uuid))
    .bind(i64::from(request.limit().get()) + 1)
    .fetch_all(self.pool())
    .await
    .map_err(unavailable)?;
    let more = rows.len() > usize::from(request.limit().get());
    if more {
      rows.pop();
    }
    let items = rows
      .into_iter()
      .map(|(id, version, name, at)| {
        Ok(IssueWorkflowSummary {
          project_id: project.id,
          id: IssueWorkflowId::from_uuid(id).map_err(|_| StoreError::Unavailable)?,
          version: IssueWorkflowVersion::new(u64::try_from(version).map_err(|_| StoreError::Unavailable)?)
            .map_err(|_| StoreError::Unavailable)?,
          name,
          published_at: Timestamp::from_unix_millis(at).map_err(|_| StoreError::Unavailable)?,
        })
      })
      .collect::<Result<Vec<_>, StoreError>>()?;
    let next_cursor = more.then(|| items.last().unwrap().id);
    Ok(IssueWorkflowPage { items, next_cursor })
  }

  async fn default_issue_workflow(&self, project_id: ProjectId) -> Result<IssueWorkflowDefault, StoreError> {
    let project = self.project(project_id).await?.project;
    let initial = initial_issue_workflow(project_id, project.created_at)?;
    read_default(&mut *self.pool().acquire().await.map_err(unavailable)?, &initial).await
  }

  async fn issue_workflow(
    &self,
    project_id: ProjectId,
    id: IssueWorkflowId,
    version: Option<IssueWorkflowVersion>,
  ) -> Result<PublishedIssueWorkflow, StoreError> {
    let project = self.project(project_id).await?.project;
    let initial = initial_issue_workflow(project_id, project.created_at)?;
    read(
      &mut *self.pool().acquire().await.map_err(unavailable)?,
      &initial,
      id,
      version,
    )
    .await
  }

  async fn publish_issue_workflow(
    &self,
    request: ManagementMutation<PublishIssueWorkflow>,
  ) -> Result<IssueWorkflowPublicationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let intent = &request.intent;
    let identity = MutationIdentity::new_management(
      &audit,
      MutationKind::PublishIssueWorkflow,
      request.idempotency_key.to_string(),
      request.published_at,
      EntityKind::IssueWorkflow,
      intent,
    )?;
    let mut tx = match crate::mutation::begin(self.pool(), &identity).await? {
      MutationStart::Replay(value) => {
        let mut outcome: IssueWorkflowPublicationOutcome = decode_outcome(value)?;
        outcome.disposition = MutationDisposition::Replayed;
        return Ok(outcome);
      }
      MutationStart::Fresh(tx) => tx,
    };
    let initial = lock_project(&mut tx, intent.project_id).await?;
    let current = match read(&mut tx, &initial, intent.id, None).await {
      Ok(workflow) => Some(workflow),
      Err(StoreError::NotFound {
        entity: EntityKind::IssueWorkflow,
      }) => None,
      Err(error) => return Err(error),
    };
    if current.as_ref().map(|w| w.version) != intent.expected_current_version {
      return Err(conflict());
    }
    if request.published_at < initial.published_at
      || current.as_ref().is_some_and(|w| request.published_at < w.published_at)
    {
      return Err(invalid());
    }
    intent.definition.validate()?;
    let version = intent
      .expected_current_version
      .map_or(Ok(IssueWorkflowVersion::INITIAL), |v| v.next())
      .map_err(|_| invalid())?;
    if intent.id == initial.id {
      retain_initial(&mut tx, &initial).await?;
    }
    let workflow = PublishedIssueWorkflow {
      project_id: intent.project_id,
      id: intent.id,
      version,
      definition: intent.definition.clone(),
      published_at: request.published_at,
    };
    sqlx::query("INSERT INTO issue_workflows (project_id, id, current_version) VALUES ($1,$2,$3) ON CONFLICT (project_id, id) DO UPDATE SET current_version = EXCLUDED.current_version")
      .bind(intent.project_id.as_uuid()).bind(intent.id.as_uuid()).bind(number(version.get(), StoreOperation::PublishIssueWorkflow)?)
      .execute(&mut *tx).await.map_err(unavailable)?;
    insert_version(&mut tx, &workflow).await?;
    let outcome = IssueWorkflowPublicationOutcome {
      disposition: MutationDisposition::Applied,
      workflow,
    };
    let facts = MutationFacts::management(
      &audit,
      intent.id.to_string(),
      json!({"project_id": intent.project_id, "version":version.get()}),
      json!({"project_id": intent.project_id, "workflow_id": intent.id, "version":version.get()}),
    );
    crate::mutation::commit(tx, &identity, facts, encode_outcome(&outcome)?).await?;
    Ok(outcome)
  }

  async fn select_default_issue_workflow(
    &self,
    request: ManagementMutation<SelectDefaultIssueWorkflow>,
  ) -> Result<IssueWorkflowSelectionOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let intent = &request.intent;
    let now: i64 = sqlx::query_scalar("SELECT FLOOR(EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT")
      .fetch_one(self.pool())
      .await
      .map_err(unavailable)?;
    let occurred_at = Timestamp::from_unix_millis(now).map_err(|_| StoreError::Unavailable)?;
    let identity = MutationIdentity::new_management(
      &audit,
      MutationKind::SelectDefaultIssueWorkflow,
      request.idempotency_key.to_string(),
      occurred_at,
      EntityKind::IssueWorkflow,
      intent,
    )?;
    let mut tx = match crate::mutation::begin(self.pool(), &identity).await? {
      MutationStart::Replay(value) => {
        let mut outcome: IssueWorkflowSelectionOutcome = decode_outcome(value)?;
        outcome.disposition = MutationDisposition::Replayed;
        return Ok(outcome);
      }
      MutationStart::Fresh(tx) => tx,
    };
    let initial = lock_project(&mut tx, intent.project_id).await?;
    let current = read_default(&mut tx, &initial).await?;
    if current.version != intent.expected_version {
      return Err(conflict());
    }
    let workflow = read(&mut tx, &initial, intent.workflow_id, Some(intent.workflow_version)).await?;
    if workflow.id == initial.id {
      retain_initial(&mut tx, &initial).await?;
    }
    let version = current.version.next().map_err(|_| invalid())?;
    sqlx::query("INSERT INTO issue_workflow_defaults (project_id, selection_version, workflow_id, workflow_version) VALUES ($1,$2,$3,$4) ON CONFLICT (project_id) DO UPDATE SET selection_version = EXCLUDED.selection_version, workflow_id = EXCLUDED.workflow_id, workflow_version = EXCLUDED.workflow_version")
      .bind(intent.project_id.as_uuid()).bind(number(version.get(), StoreOperation::SelectDefaultIssueWorkflow)?)
      .bind(workflow.id.as_uuid()).bind(number(workflow.version.get(), StoreOperation::SelectDefaultIssueWorkflow)?)
      .execute(&mut *tx).await.map_err(unavailable)?;
    let outcome = IssueWorkflowSelectionOutcome {
      disposition: MutationDisposition::Applied,
      default: IssueWorkflowDefault { version, workflow },
    };
    let metadata = json!({
      "project_id": intent.project_id,
      "version": version.get(),
      "workflow_id": intent.workflow_id,
      "workflow_version": intent.workflow_version.get(),
    });
    let facts = MutationFacts::management(&audit, intent.project_id.to_string(), metadata.clone(), metadata);
    crate::mutation::commit(tx, &identity, facts, encode_outcome(&outcome)?).await?;
    Ok(outcome)
  }
}

fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::IssueWorkflow,
  }
}
fn invalid() -> StoreError {
  StoreError::InvalidInput {
    operation: StoreOperation::PublishIssueWorkflow,
    source: StoreInputError::InvalidIssueWorkflow,
  }
}

// Serializes only business-policy changes within this Project, including the first
// publication when no workflow row exists yet. It also fences concurrent deletion.
pub(super) async fn lock_project(
  connection: &mut PgConnection,
  project_id: ProjectId,
) -> Result<PublishedIssueWorkflow, StoreError> {
  project_template(connection, project_id, true).await
}

// Issue mutations already lock their own head. Reading immutable policy must use
// the same connection without taking the Project publication lock or another pool slot.
pub(super) async fn project_template(
  connection: &mut PgConnection,
  project_id: ProjectId,
  lock: bool,
) -> Result<PublishedIssueWorkflow, StoreError> {
  let query = if lock {
    "SELECT FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT FROM projects WHERE id = $1 FOR UPDATE"
  } else {
    "SELECT FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT FROM projects WHERE id = $1"
  };
  let created_at: i64 = sqlx::query_scalar(query)
    .bind(project_id.as_uuid())
    .fetch_optional(connection)
    .await
    .map_err(unavailable)?
    .ok_or(StoreError::NotFound {
      entity: EntityKind::Project,
    })?;
  initial_issue_workflow(
    project_id,
    Timestamp::from_unix_millis(created_at).map_err(|_| StoreError::Unavailable)?,
  )
}

pub(super) async fn read(
  connection: &mut PgConnection,
  initial: &PublishedIssueWorkflow,
  id: IssueWorkflowId,
  version: Option<IssueWorkflowVersion>,
) -> Result<PublishedIssueWorkflow, StoreError> {
  let row: Option<(i64, Json<IssueWorkflowDefinition>, i64)> = sqlx::query_as(
    "SELECT version, definition, published_at_millis FROM issue_workflow_versions WHERE project_id = $1 AND workflow_id = $2 AND version = COALESCE($3, (SELECT current_version FROM issue_workflows WHERE project_id = $1 AND id = $2))")
    .bind(initial.project_id.as_uuid()).bind(id.as_uuid()).bind(version.map(|v| number(v.get(), StoreOperation::PublishIssueWorkflow)).transpose()?)
    .fetch_optional(connection).await.map_err(unavailable)?;
  if let Some((version, Json(definition), published_at)) = row {
    definition.validate().map_err(|_| StoreError::Unavailable)?;
    return Ok(PublishedIssueWorkflow {
      project_id: initial.project_id,
      id,
      version: IssueWorkflowVersion::new(u64::try_from(version).map_err(|_| StoreError::Unavailable)?)
        .map_err(|_| StoreError::Unavailable)?,
      definition,
      published_at: Timestamp::from_unix_millis(published_at).map_err(|_| StoreError::Unavailable)?,
    });
  }
  if id == initial.id && version.is_none_or(|v| v == initial.version) {
    return Ok(initial.clone());
  }
  Err(StoreError::NotFound {
    entity: EntityKind::IssueWorkflow,
  })
}

// Materialize the retained template only inside an authorized mutation, so exact
// default references get a foreign key without turning reads into initialization.
pub(super) async fn retain_initial(
  connection: &mut PgConnection,
  initial: &PublishedIssueWorkflow,
) -> Result<(), StoreError> {
  sqlx::query("INSERT INTO issue_workflows (project_id, id, current_version) VALUES ($1,$2,1) ON CONFLICT DO NOTHING")
    .bind(initial.project_id.as_uuid())
    .bind(initial.id.as_uuid())
    .execute(&mut *connection)
    .await
    .map_err(unavailable)?;
  sqlx::query("INSERT INTO issue_workflow_versions (project_id, workflow_id, version, definition, published_at_millis) VALUES ($1,$2,1,$3,$4) ON CONFLICT DO NOTHING")
    .bind(initial.project_id.as_uuid()).bind(initial.id.as_uuid()).bind(Json(&initial.definition)).bind(initial.published_at.unix_millis())
    .execute(connection).await.map_err(unavailable)?;
  Ok(())
}

async fn insert_version(connection: &mut PgConnection, workflow: &PublishedIssueWorkflow) -> Result<(), StoreError> {
  sqlx::query("INSERT INTO issue_workflow_versions (project_id, workflow_id, version, definition, published_at_millis) VALUES ($1,$2,$3,$4,$5)")
    .bind(workflow.project_id.as_uuid()).bind(workflow.id.as_uuid()).bind(number(workflow.version.get(), StoreOperation::PublishIssueWorkflow)?)
    .bind(Json(&workflow.definition)).bind(workflow.published_at.unix_millis()).execute(connection).await.map_err(unavailable)?;
  Ok(())
}

pub(super) async fn read_default(
  connection: &mut PgConnection,
  initial: &PublishedIssueWorkflow,
) -> Result<IssueWorkflowDefault, StoreError> {
  let selected: Option<(i64, uuid::Uuid, i64)> = sqlx::query_as(
    "SELECT selection_version, workflow_id, workflow_version FROM issue_workflow_defaults WHERE project_id = $1",
  )
  .bind(initial.project_id.as_uuid())
  .fetch_optional(&mut *connection)
  .await
  .map_err(unavailable)?;
  let Some((version, id, workflow_version)) = selected else {
    return Ok(IssueWorkflowDefault {
      version: IssueWorkflowSelectionVersion::INITIAL,
      workflow: initial.clone(),
    });
  };
  Ok(IssueWorkflowDefault {
    version: IssueWorkflowSelectionVersion::new(u64::try_from(version).map_err(|_| StoreError::Unavailable)?)
      .map_err(|_| StoreError::Unavailable)?,
    workflow: read(
      connection,
      initial,
      IssueWorkflowId::from_uuid(id).map_err(|_| StoreError::Unavailable)?,
      Some(
        IssueWorkflowVersion::new(u64::try_from(workflow_version).map_err(|_| StoreError::Unavailable)?)
          .map_err(|_| StoreError::Unavailable)?,
      ),
    )
    .await?,
  })
}
