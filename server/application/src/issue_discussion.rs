//! Operator discussion is a separate authorized boundary with no execution dependencies.
use crate::*;
use octacity_server_domain::{IssueCommentId, IssueId, ProjectId};
use octacity_server_store::{AppendIssueComment, IssueDiscussionStore, IssueStore};
pub use octacity_server_store::{
  AppendIssueCommentIntent, IssueCommentBody, IssueCommentContent, IssueCommentMetadata, IssueCommentOutcome,
  IssueCommentPage, IssueDiscussionLimits, MAX_ISSUE_COMMENT_BODY_BYTES, MAX_ISSUE_DISCUSSION_COMMENTS,
};
use std::sync::Arc;

/// Bounded safe references for an Issue card/timeline, with no body-read authority.
#[derive(Clone, Debug)]
pub struct ListIssueCommentsQuery {
  project_id: ProjectId,
  issue_id: IssueId,
  after: Option<u64>,
  limit: u16,
}
impl ListIssueCommentsQuery {
  /// Uses the existing history page bound; sequence is an internal application continuation.
  pub fn try_new(
    project_id: ProjectId,
    issue_id: IssueId,
    after: Option<u64>,
    limit: u16,
  ) -> Result<Self, ApplicationError> {
    if limit == 0 || limit > octacity_server_store::MAX_ISSUE_HISTORY_PAGE_SIZE {
      return Err(ApplicationError::invalid());
    }
    Ok(Self {
      project_id,
      issue_id,
      after,
      limit,
    })
  }
}
impl Query for ListIssueCommentsQuery {
  type Outcome = IssueCommentPage;
}
impl ManagementAuthorizationTarget for ListIssueCommentsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.issue_id)
  }
}

/// Append-only operator input; no actor, timestamp or execution-state setter.
#[derive(Clone, Debug)]
pub struct AppendIssueCommentCommand {
  /// Frozen plain-text body and optional root reply.
  pub intent: AppendIssueCommentIntent,
  /// Scoped original retry identity.
  pub idempotency_key: octacity_server_store::IdempotencyKey,
}
impl Command for AppendIssueCommentCommand {
  type Outcome = IssueCommentOutcome;
}
impl ManagementAuthorizationTarget for AppendIssueCommentCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Create, ManagementResourceKind::IssueDiscussion);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::IssueDiscussion, self.intent.issue_id)
  }
}
/// Dedicated body query; generic Issue projections contain only comment metadata.
#[derive(Clone, Debug)]
pub struct GetIssueCommentBodyQuery {
  /// Exact owning Project.
  pub project_id: ProjectId,
  /// Visible owning Issue.
  pub issue_id: IssueId,
  /// Original comment identity.
  pub comment_id: IssueCommentId,
}
impl Query for GetIssueCommentBodyQuery {
  type Outcome = IssueCommentContent;
}
impl ManagementAuthorizationTarget for GetIssueCommentBodyQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::IssueDiscussion);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::IssueDiscussion, self.issue_id)
  }
}
/// Narrow optional discussion composition; manual Issue services install no discussion/execution ports.
pub struct IssueDiscussionHandlers<S> {
  limits: IssueDiscussionLimits,
  store: Arc<S>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<S> IssueDiscussionHandlers<S> {
  /// Reuses existing task authority, management policy and trusted clock.
  pub fn new(
    store: Arc<S>,
    policy: Arc<dyn ManagementAuthorizationPolicy>,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Self {
    Self {
      store,
      policy,
      clock,
      limits: IssueDiscussionLimits::default(),
    }
  }
  /// Installs already validated stricter deployment bounds without accepting wire authority.
  pub fn with_limits(mut self, limits: IssueDiscussionLimits) -> Self {
    self.limits = limits;
    self
  }
  async fn visible_issue(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    issue: IssueId,
  ) -> Result<(), ApplicationError> {
    let visibility = grant
      .visibility_for::<GetIssueCommentBodyQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    if !visibility.allows(&issue) {
      return Err(ApplicationError::Store(octacity_server_store::StoreError::NotFound {
        entity: octacity_server_domain::EntityKind::Issue,
      }));
    }
    crate::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Issue,
      issue,
    )
    .await
  }
}
#[async_trait::async_trait]
impl<S: IssueDiscussionStore + IssueStore> ManagementCommandUseCase<AppendIssueCommentCommand>
  for IssueDiscussionHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: AppendIssueCommentCommand,
  ) -> Result<IssueCommentOutcome, ApplicationError> {
    self.visible_issue(context, grant, command.intent.issue_id).await?;
    self
      .store
      .issue(command.intent.project_id, command.intent.issue_id, None)
      .await?;
    Ok(
      self
        .store
        .append_issue_comment(crate::management_security::audited_mutation(
          context,
          AppendIssueComment {
            limits: self.limits,
            id: IssueCommentId::generate(),
            intent: command.intent,
            idempotency_key: command.idempotency_key,
            created_at: self.clock.now()?,
          },
        )?)
        .await?,
    )
  }
}
#[async_trait::async_trait]
impl<S: IssueDiscussionStore + IssueStore> ManagementQueryUseCase<GetIssueCommentBodyQuery>
  for IssueDiscussionHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetIssueCommentBodyQuery,
  ) -> Result<IssueCommentContent, ApplicationError> {
    self.visible_issue(context, grant, query.issue_id).await?;
    Ok(
      self
        .store
        .issue_comment_body(query.project_id, query.issue_id, query.comment_id)
        .await?,
    )
  }
}
#[async_trait::async_trait]
impl<S: IssueDiscussionStore> ManagementQueryUseCase<ListIssueCommentsQuery> for IssueDiscussionHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListIssueCommentsQuery,
  ) -> Result<IssueCommentPage, ApplicationError> {
    crate::issue::visible_issue(grant, query.issue_id)?;
    Ok(
      self
        .store
        .issue_comments(query.project_id, query.issue_id, query.after, query.limit)
        .await?,
    )
  }
}
