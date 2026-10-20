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
  async fn issue_status_command(
    &self,
    project_id: ProjectId,
    id: IssueId,
    scope: &ManagementSecurityScope,
    operation_id: octacity_server_factory::FactoryDigest,
  ) -> Result<Option<RetainedIssueStatusCommand>, StoreError> {
    let value: Option<Json<RetainedIssueStatusCommand>> = sqlx::query_scalar(
      "SELECT outcome->'retained_command' FROM idempotency_records WHERE scope=$1 AND security_scope=$2 AND idempotency_key=$3 AND outcome ? 'retained_command'",
    ).bind(MutationKind::TransitionIssue.scope()).bind(scope.as_str()).bind(issue_status_operation_key(operation_id).as_str())
      .fetch_optional(self.pool()).await.map_err(unavailable)?;
    Ok(
      value
        .map(|Json(command)| command)
        .filter(|c| c.intent.project_id == project_id && c.intent.id == id),
    )
  }
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
    Ok(page(rows, request.limit()))
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
    let issue = create_initial_issue(&mut tx, &request, &audit, IssueTransitionOrigin::Manual).await?;
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
    let (mut request, audit) = request.into_parts();
    let command = request.retained_pipeline_command().map(|command| json!(command));
    let canonical = request.pipeline_action().map(|action| {
      let mut value = json!({
        "intent":request.intent,"selection":action.selection(),"provenance":action.provenance()
      });
      if let Some(review) = action.push_mapping_review() {
        value["push_review"] = json!(review);
      }
      value
    });
    let mut identity = if let Some(canonical) = canonical {
      MutationIdentity::new_management(
        &audit,
        MutationKind::TransitionIssue,
        request.idempotency_key.to_string(),
        request.moved_at,
        EntityKind::Issue,
        &canonical,
      )?
    } else {
      // Preserve the exact pre-existing manual command digest/receipt encoding.
      MutationIdentity::new_management(
        &audit,
        MutationKind::TransitionIssue,
        request.idempotency_key.to_string(),
        request.moved_at,
        EntityKind::Issue,
        &request.intent,
      )?
    };
    if let Some(command) = &command {
      // Seal accepted provenance before the business transaction. A crash can leave
      // only the original pending command, never a retargetable empty reservation.
      match crate::mutation::begin_retained(self.pool(), &identity, command.clone()).await? {
        MutationStart::Replay(value) => return transition_replay(value),
        MutationStart::Fresh(tx) => tx.commit().await.map_err(unavailable)?,
      }
    }
    let start = match command {
      Some(command) => crate::mutation::begin_retained(self.pool(), &identity, command).await?,
      None => crate::mutation::begin(self.pool(), &identity).await?,
    };
    let mut tx = match start {
      MutationStart::Replay(value) => return transition_replay(value),
      MutationStart::Fresh(tx) => tx,
    };
    if request.pipeline_action().is_some() {
      let Json(original): Json<RetainedIssueStatusCommand> = sqlx::query_scalar(
        "SELECT outcome->'retained_command' FROM idempotency_records WHERE scope=$1 AND security_scope=$2 AND idempotency_key=$3",
      ).bind(identity.kind.scope()).bind(identity.persisted_security_scope()).bind(&identity.key)
        .fetch_one(&mut *tx).await.map_err(unavailable)?;
      // History and the shared audit/outbox publisher must retain the same command time.
      identity.occurred_at = original.moved_at;
      request.moved_at = original.moved_at;
    }
    let intent = &request.intent;
    let evaluated = async {
      let current = lock(&mut tx, intent.project_id, intent.id).await?;
      if let Some(action) = request.pipeline_action() {
        let p = action.provenance();
        let association: Option<(Json<IssueWorkAssociation>,Option<i64>)> = sqlx::query_as(
          "SELECT document,status_applicability_version FROM issue_work_associations WHERE project_id=$1 AND issue_id=$2 AND work_id=$3",
        ).bind(current.project_id.as_uuid()).bind(current.id.as_uuid()).bind(p.work_id.as_uuid())
          .fetch_optional(&mut *tx).await.map_err(unavailable)?;
        if association.is_none_or(|(Json(a),marker)| a.issue_id != p.issue_id || a.run_id != p.run_id
          || a.requirements_version != Some(p.requirements_version)
          || marker != i64::try_from(current.applicability_version.get()).ok()) {
          return Err(StoreError::Conflict { entity: EntityKind::Issue });
        }
      }
      let initial = issue_workflow::project_template(&mut tx, intent.project_id, false).await?;
      let workflow = issue_workflow::read(&mut tx, &initial, current.workflow_id, Some(current.workflow_version)).await?;
      let policy = field_policy(&mut tx, intent.project_id, intent.id).await?;
      let outcome = current.transitioned(&request, &workflow, &policy, audit.actor())?;
      Ok((current,policy,outcome))
    }.await;
    let (current, policy, outcome) = match evaluated {
      Ok(value) => value,
      Err(error) => {
        if request.pipeline_action().is_some()
          && let Some(rejection) = TransitionRejection::from_error(&error)
        {
          crate::mutation::complete_record(tx, &identity, json!({"pipeline_rejection":rejection})).await?;
        }
        return Err(error);
      }
    };
    if let Some(action) = request.pipeline_action() {
      sqlx::query("UPDATE issue_work_associations SET status_applicability_version=$2 WHERE work_id=$1")
        .bind(action.provenance().work_id.as_uuid())
        .bind(number(
          outcome.issue.applicability_version.get(),
          StoreOperation::TransitionIssue,
        )?)
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?;
    }
    publish(
      tx,
      &identity,
      &audit,
      &outcome.issue,
      Some(&outcome.transition),
      &outcome,
      policy
        .push_intent(&current, &outcome.issue)
        .filter(|_| request.allows_push_intent()),
    )
    .await?;
    Ok(outcome)
  }
}
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum TransitionRejection {
  Conflict,
  MissingIssue,
  Invalid,
}
impl TransitionRejection {
  fn from_error(error: &StoreError) -> Option<Self> {
    match error {
      StoreError::Conflict {
        entity: EntityKind::Issue,
      } => Some(Self::Conflict),
      StoreError::NotFound {
        entity: EntityKind::Issue,
      } => Some(Self::MissingIssue),
      StoreError::InvalidInput {
        operation: StoreOperation::TransitionIssue,
        source: StoreInputError::InvalidIssue,
      } => Some(Self::Invalid),
      _ => None,
    }
  }
  fn into_error(self) -> StoreError {
    match self {
      Self::Conflict => StoreError::Conflict {
        entity: EntityKind::Issue,
      },
      Self::MissingIssue => StoreError::NotFound {
        entity: EntityKind::Issue,
      },
      Self::Invalid => invalid(StoreOperation::TransitionIssue),
    }
  }
}
fn transition_replay(value: serde_json::Value) -> Result<IssueTransitionOutcome, StoreError> {
  if let Some(rejection) = value.get("pipeline_rejection") {
    return Err(
      serde_json::from_value::<TransitionRejection>(rejection.clone())
        .map_err(|_| StoreError::Unavailable)?
        .into_error(),
    );
  }
  let mut outcome: IssueTransitionOutcome = decode_outcome(value)?;
  outcome.disposition = MutationDisposition::Replayed;
  Ok(outcome)
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

pub(super) async fn lock(
  connection: &mut PgConnection,
  project_id: ProjectId,
  id: IssueId,
) -> Result<IssueRevision, StoreError> {
  // Parent foreign keys take KEY SHARE: metadata edits must exclude competing
  // edits without blocking references to this stable Issue identity.
  sqlx::query("SELECT id FROM issues WHERE project_id=$1 AND id=$2 FOR NO KEY UPDATE")
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
pub(super) async fn publish<T: serde::Serialize>(
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
  if let Some(mut intent) = push_intent {
    let link: Option<Json<IssueSourceLink>> = sqlx::query_scalar(
      "SELECT document FROM issue_source_links WHERE project_id=$1 AND issue_id=$2 AND security_scope=$3",
    )
    .bind(issue.project_id.as_uuid())
    .bind(issue.id.as_uuid())
    .bind(audit.security_scope().as_str())
    .fetch_optional(&mut *tx)
    .await
    .map_err(unavailable)?;
    if let Some(Json(link)) = link
      && let Some(mapping) = crate::issue_source_mapping::read(&mut tx, issue.project_id, issue.id, None)
        .await?
        .filter(|m| m.reference() == intent.mapping)
    {
      let key = IdempotencyKey::new(identity.key.clone()).map_err(|_| StoreError::Unavailable)?;
      intent.desired = Some(intent.desired_state(issue, &link, &mapping, audit, &key, history)?);
    }
    if let Some(row) = intent.projection_record(issue.updated_at) {
      crate::factory_source_projection::record_issue_publication(&mut tx, &intent, &row).await?;
      crate::factory_run::insert_outbox(&mut tx, &row).await?;
    }
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

pub(super) async fn field_policy(
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

// Ordinary discovery and child discovery expose the same bounded safe summary.
pub(super) fn page(rows: Vec<(Json<IssueRevision>,)>, limit: u16) -> IssuePage {
  let mut items: Vec<_> = rows.into_iter().map(|(Json(i),)| IssueSummary::from(&i)).collect();
  let more = items.len() > usize::from(limit);
  if more {
    items.pop();
  }
  let next_cursor = more.then(|| items.last().unwrap().id);
  IssuePage { items, next_cursor }
}

// Manual creation and canonical native import share all initial Issue policy and publication rows.
pub(super) async fn create_initial_issue(
  connection: &mut PgConnection,
  request: &CreateIssue,
  audit: &MutationAuditContext,
  origin: IssueTransitionOrigin,
) -> Result<IssueRevision, StoreError> {
  let intent = &request.intent;
  intent.content.validate()?;
  let initial = issue_workflow::lock_project(connection, intent.project_id).await?;
  if request.created_at < initial.published_at {
    return Err(invalid(StoreOperation::CreateIssue));
  }
  crate::issue_planning::validate_parent(connection, intent.project_id, request.id, intent.planning.parent_id).await?;
  let catalog = crate::issue_planning::read(connection, intent.project_id, None).await?;
  intent.planning.validate(&catalog, None, &request.assignees)?;
  crate::issue_planning::retain_initial(connection, intent.project_id).await?;
  let workflow = issue_workflow::read_default(connection, &initial).await?.workflow;
  if workflow.id == initial.id {
    issue_workflow::retain_initial(connection, &initial).await?;
  }
  let allocated: i64 = sqlx::query_scalar("INSERT INTO issue_numbers (project_id,last_number) VALUES ($1,1) ON CONFLICT (project_id) DO UPDATE SET last_number=issue_numbers.last_number+1 RETURNING last_number")
      .bind(intent.project_id.as_uuid()).fetch_one(&mut *connection).await.map_err(unavailable)?;
  let issue = IssueRevision {
    id: request.id,
    project_id: intent.project_id,
    number: u64::try_from(allocated).map_err(|_| StoreError::Unavailable)?,
    version: IssueVersion::INITIAL,
    applicability_version: IssueVersion::INITIAL,
    content: intent.content.clone(),
    planning: intent.planning.clone(),
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
    .execute(&mut *connection)
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
    source: None,
    issue_id: issue.id,
    version: issue.version,
    workflow_id: issue.workflow_id,
    workflow_version: issue.workflow_version,
    from: None,
    to: issue.status_id,
    transition_id: None,
    previous_workflow: None,
    origin,
    pipeline: None,
    reason: None,
    actor: audit.actor().clone(),
    occurred_at: issue.created_at,
  };
  insert_revision(connection, &issue, Some(&history)).await?;
  Ok(issue)
}
