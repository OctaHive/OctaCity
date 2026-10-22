//! Authorized task commands; task intent never owns execution or provider dispatch.
use crate::*;
pub use octacity_server_domain::IssueVersion;
use octacity_server_domain::{IssueId, ProjectId, Timestamp};
pub use octacity_server_store::{
  AdoptIssueWorkflowIntent, CreateIssueIntent, EditIssueIntent, IssueContent, IssueField, IssueFieldPolicy,
  IssueMutationOutcome, IssuePage, IssueRevision, IssueStatusHistoryPage, IssueStore, IssueSummary,
  IssueTransitionOutcome, MAX_ISSUE_DOCUMENT_BYTES, MAX_ISSUE_TITLE_BYTES, TransitionIssueIntent,
};
use octacity_server_store::{CreateIssue, EditIssue, IdempotencyKey, TransitionIssue};
use std::sync::Arc;

#[path = "issue_card.rs"]
mod card;
pub use card::*;

/// Manual task creation independent of workflow administration or execution setup.
#[derive(Clone, Debug)]
pub struct CreateIssueCommand {
  /// Frozen bounded task text and owning Project.
  pub intent: CreateIssueIntent,
  /// Scoped caller replay key.
  pub idempotency_key: IdempotencyKey,
  /// Original server time.
  pub created_at: Timestamp,
}
impl Command for CreateIssueCommand {
  type Outcome = IssueMutationOutcome;
}
impl ManagementAuthorizationTarget for CreateIssueCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::owned_collection(
    ManagementAction::Create,
    ManagementResourceKind::Issue,
    ManagementResourceKind::Project,
  );
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::owned_collection_resource(
      ManagementResourceKind::Issue,
      ManagementResourceKind::Project,
      self.intent.project_id,
    )
  }
}
/// Dedicated current/retained task content read.
#[derive(Clone, Debug)]
pub struct GetIssueQuery {
  /// Exact owning Project.
  pub project_id: ProjectId,
  /// Stable task identity.
  pub id: IssueId,
  /// Exact retained revision, or current when omitted.
  pub version: Option<IssueVersion>,
}
impl Query for GetIssueQuery {
  type Outcome = IssueRevision;
}
impl ManagementAuthorizationTarget for GetIssueQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::Issue);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.id)
  }
}
/// Thin composition of transactional task authority, with no execution or provider ports.
pub struct IssueHandlers<S> {
  store: Arc<S>,
  directory: Option<Arc<dyn IssueAssigneeDirectory>>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
}
impl<S> IssueHandlers<S> {
  /// Reuses the same Project/task store for atomic policy and revision publication.
  pub fn new(store: Arc<S>, policy: Arc<dyn ManagementAuthorizationPolicy>) -> Self {
    Self {
      store,
      policy,
      directory: None,
    }
  }
}
#[async_trait::async_trait]
impl<S: IssueStore> ManagementCommandUseCase<CreateIssueCommand> for IssueHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: CreateIssueCommand,
  ) -> Result<IssueMutationOutcome, ApplicationError> {
    let projects = grant
      .visibility_for::<ListProjectsQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    if !projects.allows(&command.intent.project_id) {
      return Err(
        octacity_server_store::StoreError::NotFound {
          entity: octacity_server_domain::EntityKind::Project,
        }
        .into(),
      );
    }
    self.visible_parent(context, command.intent.planning.parent_id).await?;
    let assignees = self
      .planning_assignees(context, command.intent.project_id, &command.intent.planning)
      .await?;
    Ok(
      self
        .store
        .create_issue(crate::management_security::audited_mutation(
          context,
          CreateIssue {
            assignees,
            id: IssueId::generate(),
            intent: command.intent,
            idempotency_key: command.idempotency_key,
            created_at: command.created_at,
          },
        )?)
        .await?,
    )
  }
}
#[async_trait::async_trait]
impl<S: IssueStore> ManagementQueryUseCase<GetIssueQuery> for IssueHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetIssueQuery,
  ) -> Result<IssueRevision, ApplicationError> {
    visible_issue(grant, query.id)?;
    Ok(self.store.issue(query.project_id, query.id, query.version).await?)
  }
}

/// Explicit content revision; does not authorize business transitions or execution.
#[derive(Clone, Debug)]
pub struct EditIssueCommand {
  /// Frozen edited fields and original version.
  pub intent: EditIssueIntent,
  /// Scoped caller replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Server mutation time.
  pub edited_at: Timestamp,
}
impl Command for EditIssueCommand {
  type Outcome = IssueMutationOutcome;
}
impl ManagementAuthorizationTarget for EditIssueCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Update, ManagementResourceKind::Issue);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.intent.id)
  }
}
#[async_trait::async_trait]
impl<S: IssueStore> ManagementCommandUseCase<EditIssueCommand> for IssueHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: EditIssueCommand,
  ) -> Result<IssueMutationOutcome, ApplicationError> {
    visible_issue(grant, command.intent.id)?;
    Ok(
      self
        .store
        .edit_issue(crate::management_security::audited_mutation(
          context,
          EditIssue {
            intent: command.intent,
            idempotency_key: command.idempotency_key,
            edited_at: command.edited_at,
          },
        )?)
        .await?,
    )
  }
}

/// Explicit organizational move with rights distinct from text editing and execution.
#[derive(Clone, Debug)]
pub struct TransitionIssueCommand {
  /// Frozen edge and exact task/workflow/status preconditions.
  pub intent: TransitionIssueIntent,
  /// Scoped replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Trusted mutation time.
  pub moved_at: Timestamp,
}
impl Command for TransitionIssueCommand {
  type Outcome = IssueTransitionOutcome;
}
impl ManagementAuthorizationTarget for TransitionIssueCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Publish, ManagementResourceKind::Issue);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.intent.id)
  }
}
#[async_trait::async_trait]
impl<S: IssueStore> ManagementCommandUseCase<TransitionIssueCommand> for IssueHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: TransitionIssueCommand,
  ) -> Result<IssueTransitionOutcome, ApplicationError> {
    visible_issue(grant, command.intent.id)?;
    Ok(
      self
        .store
        .transition_issue(crate::management_security::audited_mutation(
          context,
          TransitionIssue::manual(command.intent, command.idempotency_key, command.moved_at),
        )?)
        .await?,
    )
  }
}
/// Bounded organizational history with the same dedicated task visibility as content reads.
#[derive(Clone, Debug)]
pub struct ListIssueStatusHistoryQuery {
  project_id: ProjectId,
  id: IssueId,
  after: Option<IssueVersion>,
  limit: u16,
}
impl ListIssueStatusHistoryQuery {
  /// Validates the bounded public history request before dispatch.
  pub fn try_new(
    project_id: ProjectId,
    id: IssueId,
    after: Option<IssueVersion>,
    limit: u16,
  ) -> Result<Self, ApplicationError> {
    if limit == 0 || limit > octacity_server_store::MAX_ISSUE_HISTORY_PAGE_SIZE {
      return Err(ApplicationError::InvalidInput);
    }
    Ok(Self {
      project_id,
      id,
      after,
      limit,
    })
  }
}
impl Query for ListIssueStatusHistoryQuery {
  type Outcome = IssueStatusHistoryPage;
}
impl ManagementAuthorizationTarget for ListIssueStatusHistoryQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.id)
  }
}
#[async_trait::async_trait]
impl<S: IssueStore> ManagementQueryUseCase<ListIssueStatusHistoryQuery> for IssueHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListIssueStatusHistoryQuery,
  ) -> Result<IssueStatusHistoryPage, ApplicationError> {
    visible_issue(grant, query.id)?;
    Ok(
      self
        .store
        .issue_status_history(query.project_id, query.id, query.after, query.limit)
        .await?,
    )
  }
}

pub(crate) fn visible_issue(grant: &ManagementAuthorizationGrant, id: IssueId) -> Result<(), ApplicationError> {
  match grant.visibility().view() {
    ManagementVisibilityView::All => Ok(()),
    ManagementVisibilityView::None => Err(
      octacity_server_store::StoreError::NotFound {
        entity: octacity_server_domain::EntityKind::Issue,
      }
      .into(),
    ),
    ManagementVisibilityView::Restricted(resources) => {
      if resources.iter().any(|r| {
        r.kind() != ManagementResourceKind::Issue
          || r.identity().is_none_or(|id| id.as_str().parse::<IssueId>().is_err())
      }) {
        return Err(ApplicationError::InvalidAuthorizationVisibility);
      }
      let target = crate::management_security::instance_resource(ManagementResourceKind::Issue, id)
        .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
      if resources.contains(&target) {
        Ok(())
      } else {
        Err(
          octacity_server_store::StoreError::NotFound {
            entity: octacity_server_domain::EntityKind::Issue,
          }
          .into(),
        )
      }
    }
  }
}

/// Separately authorized adoption of one reviewed immutable workflow version.
#[derive(Clone, Debug)]
pub struct AdoptIssueWorkflowCommand {
  /// Frozen status correspondence and current policy preconditions.
  pub intent: octacity_server_store::AdoptIssueWorkflowIntent,
  /// Scoped replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Trusted mutation time.
  pub adopted_at: Timestamp,
}
impl Command for AdoptIssueWorkflowCommand {
  type Outcome = IssueTransitionOutcome;
}
impl ManagementAuthorizationTarget for AdoptIssueWorkflowCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Administer, ManagementResourceKind::Issue);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.intent.id)
  }
}
#[async_trait::async_trait]
impl<S: IssueStore> ManagementCommandUseCase<AdoptIssueWorkflowCommand> for IssueHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: AdoptIssueWorkflowCommand,
  ) -> Result<IssueTransitionOutcome, ApplicationError> {
    visible_issue(grant, command.intent.id)?;
    Ok(
      self
        .store
        .adopt_issue_workflow(crate::management_security::audited_mutation(
          context,
          octacity_server_store::AdoptIssueWorkflow {
            intent: command.intent,
            idempotency_key: command.idempotency_key,
            adopted_at: command.adopted_at,
          },
        )?)
        .await?,
    )
  }
}

/// Bounded current task discovery under the same identity scope as point reads.
#[derive(Clone, Debug)]
pub struct ListProjectIssuesQuery {
  page: crate::CurrentDefinitionPageInput<IssueId>,
}
impl ListProjectIssuesQuery {
  /// Validates the existing shared page bound.
  pub fn try_new(
    project_id: ProjectId,
    after: Option<IssueId>,
    limit: u16,
  ) -> Result<Self, crate::CurrentDefinitionPageError> {
    Ok(Self {
      page: crate::CurrentDefinitionPageInput::try_new(project_id, after, limit)?,
    })
  }
}
impl Query for ListProjectIssuesQuery {
  type Outcome = IssuePage;
}
impl ManagementAuthorizationTarget for ListProjectIssuesQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::owned_collection(
    ManagementAction::View,
    ManagementResourceKind::Issue,
    ManagementResourceKind::Project,
  );
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::owned_collection_resource(
      ManagementResourceKind::Issue,
      ManagementResourceKind::Project,
      self.page.project_id(),
    )
  }
}
#[async_trait::async_trait]
impl<S: IssueStore> ManagementQueryUseCase<ListProjectIssuesQuery> for IssueHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListProjectIssuesQuery,
  ) -> Result<IssuePage, ApplicationError> {
    let visibility = grant
      .visibility_for::<ListProjectIssuesQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    Ok(
      self
        .store
        .list_issues(octacity_server_store::ListProjectIssues::new(
          query.page.project_id(),
          query.page.after().copied(),
          query.page.limit(),
          visibility,
        )?)
        .await?,
    )
  }
}

/// Dedicated bounded requirements revision discovery.
#[derive(Clone, Debug)]
pub struct ListIssueRevisionsQuery(ListIssueStatusHistoryQuery);
impl ListIssueRevisionsQuery {
  /// Reuses the task history grammar and limits.
  pub fn try_new(
    project_id: ProjectId,
    id: IssueId,
    after: Option<IssueVersion>,
    limit: u16,
  ) -> Result<Self, ApplicationError> {
    ListIssueStatusHistoryQuery::try_new(project_id, id, after, limit).map(Self)
  }
}
impl Query for ListIssueRevisionsQuery {
  type Outcome = octacity_server_store::IssueRevisionPage;
}
impl ManagementAuthorizationTarget for ListIssueRevisionsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    self.0.management_resource()
  }
}
#[async_trait::async_trait]
impl<S: IssueStore> ManagementQueryUseCase<ListIssueRevisionsQuery> for IssueHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListIssueRevisionsQuery,
  ) -> Result<octacity_server_store::IssueRevisionPage, ApplicationError> {
    let query = query.0;
    visible_issue(grant, query.id)?;
    Ok(
      self
        .store
        .issue_revisions(query.project_id, query.id, query.after, query.limit)
        .await?,
    )
  }
}

/// Current task/status review; observation reserves no move.
#[derive(Clone, Debug)]
pub struct GetIssueTransitionsQuery {
  /// Exact owning Project.
  pub project_id: ProjectId,
  /// Visible Issue identity.
  pub id: IssueId,
}
impl Query for GetIssueTransitionsQuery {
  type Outcome = IssueTransitionChoices;
}
impl ManagementAuthorizationTarget for GetIssueTransitionsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.id)
  }
}
/// Read-only exact review; mutation still reauthorizes and checks its original preconditions.
#[derive(Clone, Debug, serde::Serialize)]
pub struct IssueTransitionChoices {
  /// Current Issue revision used by If-Match.
  pub issue_version: IssueVersion,
  /// Exact pinned policy identity.
  pub workflow_id: octacity_server_domain::IssueWorkflowId,
  /// Exact immutable policy publication.
  pub workflow_version: octacity_server_domain::IssueWorkflowVersion,
  /// Original status required by a move.
  pub status_id: octacity_server_domain::IssueStatusId,
  /// Current ownership/mapping review for explicit adoption and safe edit feedback.
  pub field_policy: octacity_server_store::IssueFieldPolicy,
  /// Only direct outgoing edges in configured order.
  pub items: Vec<IssueTransitionChoice>,
}
/// One direct business edge with truthful current manual eligibility.
#[derive(Clone, Debug, serde::Serialize)]
pub struct IssueTransitionChoice {
  /// Retained edge definition, with no execution-state setter.
  pub transition: octacity_server_store::IssueWorkflowTransition,
  /// True only with current transition rights and business eligibility.
  pub available: bool,
  /// Safe finite reason; never raw provider/authorization diagnostics.
  pub reason: Option<octacity_server_store::IssueMoveBlockReason>,
}
#[async_trait::async_trait]
impl<S: IssueStore + IssueWorkflowStore> ManagementQueryUseCase<GetIssueTransitionsQuery> for IssueHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetIssueTransitionsQuery,
  ) -> Result<IssueTransitionChoices, ApplicationError> {
    visible_issue(grant, query.id)?;
    let issue = self.store.issue(query.project_id, query.id, None).await?;
    let workflow = self
      .store
      .issue_workflow(query.project_id, issue.workflow_id, Some(issue.workflow_version))
      .await?;
    let field_policy = self.store.issue_field_policy(query.project_id, query.id).await?;
    let target = crate::management_security::instance_resource(ManagementResourceKind::Issue, query.id)
      .map_err(|_| ApplicationError::InvalidInput)?;
    let permitted = match self.policy.authorize(context, ManagementAction::Publish, &target).await {
      Ok(grant) => visible_issue(&grant, query.id).is_ok(),
      Err(_) => false,
    };
    let items = workflow
      .definition
      .transitions
      .into_iter()
      .filter(|e| e.from == issue.status_id)
      .map(|transition| {
        let reason = if permitted {
          transition.manual_move_block_reason(&field_policy)
        } else {
          Some(octacity_server_store::IssueMoveBlockReason::PermissionRequired)
        };
        IssueTransitionChoice {
          transition,
          available: reason.is_none(),
          reason,
        }
      })
      .collect();
    Ok(IssueTransitionChoices {
      issue_version: issue.version,
      workflow_id: issue.workflow_id,
      workflow_version: issue.workflow_version,
      status_id: issue.status_id,
      field_policy,
      items,
    })
  }
}

#[path = "issue_planning.rs"]
mod planning;
pub use planning::*;

/// Explicit accepted pipeline producer input; no public wire route accepts provenance.
#[derive(Clone, Debug)]
pub struct ApplyIssuePipelineStatusCommand {
  /// Exact Issue/workflow/status preconditions and configured direct edge.
  pub intent: TransitionIssueIntent,
  /// Accepted immutable Work/Run/action origin.
  pub accepted: octacity_server_store::AcceptedIssueStatusAction,
  /// Trusted application time.
  pub moved_at: Timestamp,
}
impl Command for ApplyIssuePipelineStatusCommand {
  type Outcome = IssueTransitionOutcome;
}
impl ManagementAuthorizationTarget for ApplyIssuePipelineStatusCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = TransitionIssueCommand::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.intent.id)
  }
}
#[async_trait::async_trait]
impl<S: IssueStore> ManagementCommandUseCase<ApplyIssuePipelineStatusCommand> for IssueHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: ApplyIssuePipelineStatusCommand,
  ) -> Result<IssueTransitionOutcome, ApplicationError> {
    visible_issue(grant, command.intent.id)?;
    Ok(
      self
        .store
        .transition_issue(crate::management_security::audited_mutation(
          context,
          TransitionIssue::pipeline(command.intent, command.accepted, command.moved_at),
        )?)
        .await?,
    )
  }
}

#[path = "issue_retention.rs"]
mod retention;
pub use retention::*;
