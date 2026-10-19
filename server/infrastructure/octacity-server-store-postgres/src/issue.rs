//! Persistent Issue authority; provider delivery and Work state are outside this adapter.
use crate::{
  PostgresStore,
  database::{number, unavailable},
  issue_workflow,
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};
use octacity_server_domain::{EntityKind, IssueId, IssueVersion, ProjectId};
use octacity_server_store::*;
use serde_json::json;
use sqlx::{PgConnection, types::Json};

#[async_trait::async_trait]
impl IssueStore for PostgresStore {
  async fn issue_field_policy(&self, project_id: ProjectId, id: IssueId) -> Result<IssueFieldPolicy, StoreError> {
    self.issue(project_id, id, None).await?;
    field_policy(&mut *self.pool().acquire().await.map_err(unavailable)?, project_id, id).await
  }

  async fn issue_revisions(
    &self,
    project_id: ProjectId,
    id: IssueId,
    after: Option<IssueVersion>,
    limit: u16,
  ) -> Result<IssueRevisionPage, StoreError> {
    if limit == 0 || limit > MAX_ISSUE_HISTORY_PAGE_SIZE {
      return Err(invalid(StoreOperation::ListIssueRevisions));
    }
    self.issue(project_id, id, None).await?;
    let rows:Vec<(Json<IssueRevision>,)>=sqlx::query_as("SELECT document FROM issue_revisions WHERE project_id=$1 AND issue_id=$2 AND ($3::bigint IS NULL OR version>$3) ORDER BY version LIMIT $4").bind(project_id.as_uuid()).bind(id.as_uuid()).bind(after.map(|v|number(v.get(),StoreOperation::ListIssueRevisions)).transpose()?).bind(i64::from(limit)+1).fetch_all(self.pool()).await.map_err(unavailable)?;
    let mut items: Vec<_> = rows
      .into_iter()
      .map(|(Json(i),)| IssueRevisionHeader::from(&i))
      .collect();
    let more = items.len() > usize::from(limit);
    if more {
      items.pop();
    }
    let next_cursor = more.then(|| items.last().unwrap().version);
    Ok(IssueRevisionPage { items, next_cursor })
  }

  async fn list_issues(&self, request: ListProjectIssues) -> Result<IssuePage, StoreError> {
    if let Some(after) = request.after() {
      let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM issues WHERE project_id=$1 AND id=$2)")
        .bind(request.project_id().as_uuid())
        .bind(after.as_uuid())
        .fetch_one(self.pool())
        .await
        .map_err(unavailable)?;
      if !exists {
        return Err(invalid(StoreOperation::ListProjectIssues));
      }
    }
    let ids: Option<Vec<uuid::Uuid>> = match request.visibility().view() {
      ReadVisibilityView::All => None,
      ReadVisibilityView::None => Some(vec![]),
      ReadVisibilityView::Restricted(ids) => Some(ids.iter().map(|id| id.as_uuid()).collect()),
    };
    let rows:Vec<(Json<IssueRevision>,)>=sqlx::query_as("SELECT r.document FROM issues i JOIN issue_revisions r ON r.project_id=i.project_id AND r.issue_id=i.id AND r.version=i.current_version WHERE i.project_id=$1 AND ($2::uuid[] IS NULL OR i.id=ANY($2)) AND ($3::uuid IS NULL OR i.id>$3) ORDER BY i.id LIMIT $4")
      .bind(request.project_id().as_uuid()).bind(ids).bind(request.after().map(|id|id.as_uuid())).bind(i64::from(request.limit())+1).fetch_all(self.pool()).await.map_err(unavailable)?;
    let mut items: Vec<_> = rows.into_iter().map(|(Json(i),)| IssueSummary::from(&i)).collect();
    let more = items.len() > usize::from(request.limit());
    if more {
      items.pop();
    }
    let next_cursor = more.then(|| items.last().unwrap().id);
    Ok(IssuePage { items, next_cursor })
  }

  async fn adopt_issue_workflow(
    &self,
    request: ManagementMutation<AdoptIssueWorkflow>,
  ) -> Result<IssueTransitionOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let intent = &request.intent;
    let identity = MutationIdentity::new_management(
      &audit,
      MutationKind::AdoptIssueWorkflow,
      request.idempotency_key.to_string(),
      request.adopted_at,
      EntityKind::Issue,
      intent,
    )?;
    let mut tx = match crate::mutation::begin(self.pool(), &identity).await? {
      MutationStart::Replay(value) => {
        let mut outcome: IssueTransitionOutcome = decode_outcome(value)?;
        outcome.disposition = MutationDisposition::Replayed;
        return Ok(outcome);
      }
      MutationStart::Fresh(tx) => tx,
    };
    let current = lock(&mut tx, intent.project_id, intent.id).await?;
    let initial = issue_workflow::project_template(&mut tx, intent.project_id, false).await?;
    let source = issue_workflow::read(&mut tx, &initial, current.workflow_id, Some(current.workflow_version)).await?;
    let target = issue_workflow::read(
      &mut tx,
      &initial,
      intent.target_workflow_id,
      Some(intent.target_workflow_version),
    )
    .await?;
    if target.id == initial.id {
      issue_workflow::retain_initial(&mut tx, &initial).await?;
    }
    let policy = field_policy(&mut tx, intent.project_id, intent.id).await?;
    let outcome = current.adopted(&request, &source, &target, &policy, audit.actor())?;
    publish(
      tx,
      &identity,
      &audit,
      &outcome.issue,
      Some(&outcome.transition),
      &outcome,
      None,
    )
    .await?;
    Ok(outcome)
  }

  async fn create_issue(&self, request: ManagementMutation<CreateIssue>) -> Result<IssueMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let intent = &request.intent;
    let identity = MutationIdentity::new_management(
      &audit,
      MutationKind::CreateIssue,
      request.idempotency_key.to_string(),
      request.created_at,
      EntityKind::Issue,
      intent,
    )?;
    let mut tx = match crate::mutation::begin(self.pool(), &identity).await? {
      MutationStart::Replay(value) => {
        let mut outcome: IssueMutationOutcome = decode_outcome(value)?;
        outcome.disposition = MutationDisposition::Replayed;
        return Ok(outcome);
      }
      MutationStart::Fresh(tx) => tx,
    };
    intent.content.validate()?;
    let initial = issue_workflow::lock_project(&mut tx, intent.project_id).await?;
    if request.created_at < initial.published_at {
      return Err(invalid(StoreOperation::CreateIssue));
    }
    let workflow = issue_workflow::read_default(&mut tx, &initial).await?.workflow;
    if workflow.id == initial.id {
      issue_workflow::retain_initial(&mut tx, &initial).await?;
    }
    let allocated: i64 = sqlx::query_scalar("INSERT INTO issue_numbers (project_id,last_number) VALUES ($1,1) ON CONFLICT (project_id) DO UPDATE SET last_number=issue_numbers.last_number+1 RETURNING last_number")
      .bind(intent.project_id.as_uuid()).fetch_one(&mut *tx).await.map_err(unavailable)?;
    let issue = IssueRevision {
      id: request.id,
      project_id: intent.project_id,
      number: u64::try_from(allocated).map_err(|_| StoreError::Unavailable)?,
      version: IssueVersion::INITIAL,
      applicability_version: IssueVersion::INITIAL,
      content: intent.content.clone(),
      workflow_id: workflow.id,
      workflow_version: workflow.version,
      status_id: workflow.definition.initial_status_id,
      created_at: request.created_at,
      updated_at: request.created_at,
    };
    sqlx::query("INSERT INTO issues (project_id,id,number,current_version) VALUES ($1,$2,$3,1)")
      .bind(issue.project_id.as_uuid())
      .bind(issue.id.as_uuid())
      .bind(allocated)
      .execute(&mut *tx)
      .await
      .map_err(|error| {
        if error.as_database_error().is_some_and(|e| e.is_unique_violation()) {
          StoreError::Duplicate {
            entity: EntityKind::Issue,
          }
        } else {
          unavailable(error)
        }
      })?;
    let history = IssueStatusHistoryEntry {
      issue_id: issue.id,
      version: issue.version,
      workflow_id: issue.workflow_id,
      workflow_version: issue.workflow_version,
      from: None,
      to: issue.status_id,
      transition_id: None,
      previous_workflow: None,
      origin: IssueTransitionOrigin::Manual,
      reason: None,
      actor: audit.actor().clone(),
      occurred_at: issue.created_at,
    };
    insert_revision(&mut tx, &issue, Some(&history)).await?;
    let outcome = IssueMutationOutcome {
      disposition: MutationDisposition::Applied,
      issue,
    };
    let metadata =
      json!({"project_id": outcome.issue.project_id, "issue_id": outcome.issue.id, "version":outcome.issue.version});
    crate::mutation::commit(
      tx,
      &identity,
      MutationFacts::management(&audit, outcome.issue.id.to_string(), metadata.clone(), metadata),
      encode_outcome(&outcome)?,
    )
    .await?;
    Ok(outcome)
  }
  async fn issue(
    &self,
    project_id: ProjectId,
    id: IssueId,
    version: Option<IssueVersion>,
  ) -> Result<IssueRevision, StoreError> {
    read(
      &mut *self.pool().acquire().await.map_err(unavailable)?,
      project_id,
      id,
      version,
    )
    .await
  }
  async fn issue_status_history(
    &self,
    project_id: ProjectId,
    id: IssueId,
    after: Option<IssueVersion>,
    limit: u16,
  ) -> Result<IssueStatusHistoryPage, StoreError> {
    if limit == 0 || limit > MAX_ISSUE_HISTORY_PAGE_SIZE {
      return Err(invalid(StoreOperation::TransitionIssue));
    }
    self.issue(project_id, id, None).await?;
    let rows: Vec<(Json<IssueStatusHistoryEntry>,)> = sqlx::query_as("SELECT document FROM issue_status_history WHERE project_id=$1 AND issue_id=$2 AND ($3::bigint IS NULL OR version>$3) ORDER BY version LIMIT $4")
      .bind(project_id.as_uuid()).bind(id.as_uuid()).bind(after.map(|v| number(v.get(), StoreOperation::TransitionIssue)).transpose()?).bind(i64::from(limit)+1)
      .fetch_all(self.pool()).await.map_err(unavailable)?;
    let mut items: Vec<_> = rows.into_iter().map(|(Json(entry),)| entry).collect();
    let more = items.len() > usize::from(limit);
    if more {
      items.pop();
    }
    let next_cursor = more.then(|| items.last().unwrap().version);
    Ok(IssueStatusHistoryPage { items, next_cursor })
  }
  async fn edit_issue(&self, request: ManagementMutation<EditIssue>) -> Result<IssueMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let intent = &request.intent;
    let identity = MutationIdentity::new_management(
      &audit,
      MutationKind::EditIssue,
      request.idempotency_key.to_string(),
      request.edited_at,
      EntityKind::Issue,
      intent,
    )?;
    let mut tx = match crate::mutation::begin(self.pool(), &identity).await? {
      MutationStart::Replay(value) => {
        let mut outcome: IssueMutationOutcome = decode_outcome(value)?;
        outcome.disposition = MutationDisposition::Replayed;
        return Ok(outcome);
      }
      MutationStart::Fresh(tx) => tx,
    };
    let current = lock(&mut tx, intent.project_id, intent.id).await?;
    let policy = field_policy(&mut tx, intent.project_id, intent.id).await?;
    let issue = current.edited(&request, &policy)?;
    let outcome = IssueMutationOutcome {
      disposition: MutationDisposition::Applied,
      issue,
    };
    publish(
      tx,
      &identity,
      &audit,
      &outcome.issue,
      None,
      &outcome,
      policy.push_intent(&current, &outcome.issue),
    )
    .await?;
    Ok(outcome)
  }
  async fn transition_issue(
    &self,
    request: ManagementMutation<TransitionIssue>,
  ) -> Result<IssueTransitionOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let intent = &request.intent;
    let identity = MutationIdentity::new_management(
      &audit,
      MutationKind::TransitionIssue,
      request.idempotency_key.to_string(),
      request.moved_at,
      EntityKind::Issue,
      intent,
    )?;
    let mut tx = match crate::mutation::begin(self.pool(), &identity).await? {
      MutationStart::Replay(value) => {
        let mut outcome: IssueTransitionOutcome = decode_outcome(value)?;
        outcome.disposition = MutationDisposition::Replayed;
        return Ok(outcome);
      }
      MutationStart::Fresh(tx) => tx,
    };
    let current = lock(&mut tx, intent.project_id, intent.id).await?;
    let initial = issue_workflow::project_template(&mut tx, intent.project_id, false).await?;
    let workflow = issue_workflow::read(&mut tx, &initial, current.workflow_id, Some(current.workflow_version)).await?;
    let policy = field_policy(&mut tx, intent.project_id, intent.id).await?;
    let outcome = current.transitioned(&request, &workflow, &policy, audit.actor())?;
    publish(
      tx,
      &identity,
      &audit,
      &outcome.issue,
      Some(&outcome.transition),
      &outcome,
      policy.push_intent(&current, &outcome.issue),
    )
    .await?;
    Ok(outcome)
  }
}
fn invalid(operation: StoreOperation) -> StoreError {
  StoreError::InvalidInput {
    operation,
    source: StoreInputError::InvalidIssue,
  }
}
async fn read(
  connection: &mut PgConnection,
  project_id: ProjectId,
  id: IssueId,
  version: Option<IssueVersion>,
) -> Result<IssueRevision, StoreError> {
  let row: Option<(Json<IssueRevision>,)> = sqlx::query_as("SELECT r.document FROM issues i JOIN issue_revisions r ON r.project_id=i.project_id AND r.issue_id=i.id AND r.version=COALESCE($3,i.current_version) WHERE i.project_id=$1 AND i.id=$2")
    .bind(project_id.as_uuid()).bind(id.as_uuid()).bind(version.map(|v| number(v.get(),StoreOperation::EditIssue)).transpose()?).fetch_optional(connection).await.map_err(unavailable)?;
  row.map(|(Json(issue),)| issue).ok_or(StoreError::NotFound {
    entity: EntityKind::Issue,
  })
}
async fn insert_revision(
  connection: &mut PgConnection,
  issue: &IssueRevision,
  history: Option<&IssueStatusHistoryEntry>,
) -> Result<(), StoreError> {
  let version = number(issue.version.get(), StoreOperation::EditIssue)?;
  sqlx::query("INSERT INTO issue_revisions (project_id,issue_id,version,workflow_id,workflow_version,document) VALUES ($1,$2,$3,$4,$5,$6)")
    .bind(issue.project_id.as_uuid()).bind(issue.id.as_uuid()).bind(version).bind(issue.workflow_id.as_uuid())
    .bind(number(issue.workflow_version.get(), StoreOperation::EditIssue)?).bind(Json(issue)).execute(&mut *connection).await.map_err(unavailable)?;
  if let Some(history) = history {
    sqlx::query("INSERT INTO issue_status_history (project_id,issue_id,version,document) VALUES ($1,$2,$3,$4)")
      .bind(issue.project_id.as_uuid())
      .bind(issue.id.as_uuid())
      .bind(version)
      .bind(Json(history))
      .execute(&mut *connection)
      .await
      .map_err(unavailable)?;
  }
  Ok(())
}

async fn lock(connection: &mut PgConnection, project_id: ProjectId, id: IssueId) -> Result<IssueRevision, StoreError> {
  sqlx::query("SELECT id FROM issues WHERE project_id=$1 AND id=$2 FOR UPDATE")
    .bind(project_id.as_uuid())
    .bind(id.as_uuid())
    .fetch_optional(&mut *connection)
    .await
    .map_err(unavailable)?
    .ok_or(StoreError::NotFound {
      entity: EntityKind::Issue,
    })?;
  read(connection, project_id, id, None).await
}
async fn publish<T: serde::Serialize>(
  mut tx: sqlx::Transaction<'_, sqlx::Postgres>,
  identity: &MutationIdentity,
  audit: &MutationAuditContext,
  issue: &IssueRevision,
  history: Option<&IssueStatusHistoryEntry>,
  outcome: &T,
  push_intent: Option<IssuePushIntent>,
) -> Result<(), StoreError> {
  insert_revision(&mut tx, issue, history).await?;
  sqlx::query("UPDATE issues SET current_version=$3 WHERE project_id=$1 AND id=$2")
    .bind(issue.project_id.as_uuid())
    .bind(issue.id.as_uuid())
    .bind(number(issue.version.get(), StoreOperation::EditIssue)?)
    .execute(&mut *tx)
    .await
    .map_err(unavailable)?;
  let metadata = json!({"project_id": issue.project_id, "issue_id":issue.id, "version":issue.version});
  let mut payload = metadata.clone();
  if let Some(intent) = push_intent {
    payload["push_intent"] = serde_json::to_value(intent).map_err(|_| StoreError::Unavailable)?;
  }
  crate::mutation::commit(
    tx,
    identity,
    MutationFacts::management(audit, issue.id.to_string(), metadata, payload),
    encode_outcome(outcome)?,
  )
  .await
}

async fn field_policy(
  connection: &mut PgConnection,
  project_id: ProjectId,
  id: IssueId,
) -> Result<IssueFieldPolicy, StoreError> {
  let (policy,): (Option<Json<IssueFieldPolicy>>,) =
    sqlx::query_as("SELECT field_policy FROM issues WHERE project_id=$1 AND id=$2")
      .bind(project_id.as_uuid())
      .bind(id.as_uuid())
      .fetch_one(connection)
      .await
      .map_err(unavailable)?;
  Ok(policy.map(|Json(policy)| policy).unwrap_or_default())
}
