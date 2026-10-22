//! Separate requirements/discussion maintenance through ordinary Issue administration.
use super::*;
pub use octacity_server_store::{
  IssueContentHold, IssueContentRetentionKind, IssueContentRetentionPlan, IssueContentRetentionScope,
  IssueContentRetentionState,
};
use octacity_server_store::{IssueContentRetentionStore, SetIssueContentRetention};

/// Read one independently versioned lifetime; no plan means no automatic expiry.
#[derive(Clone, Debug)]
pub struct GetIssueContentRetentionQuery {
  /// Exact owning Project.
  pub project_id: ProjectId,
  /// Exact existing Issue.
  pub issue_id: IssueId,
  /// Independently administered body class.
  pub kind: IssueContentRetentionKind,
}
impl Query for GetIssueContentRetentionQuery {
  type Outcome = Option<IssueContentRetentionState>;
}
impl ManagementAuthorizationTarget for GetIssueContentRetentionQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Administer, ManagementResourceKind::Issue);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.issue_id)
  }
}
/// Explicit reviewed body cleanup policy, separate from editing or transitioning a task.
#[derive(Clone, Debug)]
pub struct SetIssueContentRetentionCommand {
  /// Exact owning Project.
  pub project_id: ProjectId,
  /// Exact existing Issue.
  pub issue_id: IssueId,
  /// Zero reviews absence; positive values review the current plan of the same kind.
  pub expected_version: u64,
  /// Frozen cutoff, deadline and optional hold.
  pub plan: IssueContentRetentionPlan,
  /// Original scoped replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Trusted acceptance time.
  pub configured_at: Timestamp,
}
impl Command for SetIssueContentRetentionCommand {
  type Outcome = IssueContentRetentionState;
}
impl ManagementAuthorizationTarget for SetIssueContentRetentionCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueContentRetentionQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.issue_id)
  }
}
#[async_trait::async_trait]
impl<S: IssueContentRetentionStore> ManagementQueryUseCase<GetIssueContentRetentionQuery> for IssueHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetIssueContentRetentionQuery,
  ) -> Result<Option<IssueContentRetentionState>, ApplicationError> {
    visible_issue(grant, query.issue_id)?;
    Ok(
      self
        .store
        .issue_content_retention(query.project_id, query.issue_id, query.kind)
        .await?,
    )
  }
}
#[async_trait::async_trait]
impl<S: IssueContentRetentionStore> ManagementCommandUseCase<SetIssueContentRetentionCommand> for IssueHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: SetIssueContentRetentionCommand,
  ) -> Result<IssueContentRetentionState, ApplicationError> {
    visible_issue(grant, command.issue_id)?;
    Ok(
      self
        .store
        .set_issue_content_retention(crate::management_security::audited_mutation(
          context,
          SetIssueContentRetention {
            project_id: command.project_id,
            issue_id: command.issue_id,
            expected_version: command.expected_version,
            plan: command.plan,
            idempotency_key: command.idempotency_key,
            configured_at: command.configured_at,
          },
        )?)
        .await?,
    )
  }
}
