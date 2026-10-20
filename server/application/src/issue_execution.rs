//! Authorized execution reservation, separate from manual Issue creation and ordinary admission.
use crate::*;
use octacity_server_domain::{IssueId, ProjectId, Timestamp};
use octacity_server_store::{FactoryAdmissionStore, IdempotencyKey, IssueExecutionStore, RequestIssueExecution};
pub use octacity_server_store::{
  IssueExecutionMapping, IssueExecutionOutcome, IssueExecutionRequest, IssueExecutionTarget,
  RequestIssueExecutionIntent,
};
use std::sync::Arc;

#[path = "issue_execution_read.rs"]
mod reads;
pub use reads::*;

/// Explicit reviewed request; saving an Issue never invokes this command.
#[derive(Clone, Debug)]
pub struct RequestIssueExecutionCommand {
  /// Frozen Issue revision and target.
  pub intent: RequestIssueExecutionIntent,
  /// Scoped transport replay key.
  pub idempotency_key: IdempotencyKey,
  /// Trusted request time.
  pub requested_at: Timestamp,
}
impl Command for RequestIssueExecutionCommand {
  type Outcome = IssueExecutionOutcome;
}
impl ManagementAuthorizationTarget for RequestIssueExecutionCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Execute, ManagementResourceKind::Issue);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.intent.issue_id)
  }
}
/// Dedicated read of exact request lineage; never performs publication or admission.
#[derive(Clone, Debug)]
pub struct GetIssueExecutionQuery {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Visible owning Issue.
  pub issue_id: IssueId,
  /// Exact retained request.
  pub request_id: octacity_server_domain::IssueExecutionRequestId,
}
impl Query for GetIssueExecutionQuery {
  type Outcome = IssueExecutionRequest;
}
impl ManagementAuthorizationTarget for GetIssueExecutionQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.issue_id)
  }
}
/// Request-only composition; publication/admission remains the existing intake's responsibility.
pub struct IssueExecutionHandlers<S, F> {
  store: Arc<S>,
  factory: Arc<F>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
}
impl<S, F> IssueExecutionHandlers<S, F> {
  /// Keeps optional execution dependencies outside manual Issue handlers.
  pub fn new(store: Arc<S>, factory: Arc<F>, policy: Arc<dyn ManagementAuthorizationPolicy>) -> Self {
    Self { store, factory, policy }
  }
}
#[async_trait::async_trait]
impl<S: IssueExecutionStore + octacity_server_store::IssueStore, F: FactoryAdmissionStore>
  ManagementCommandUseCase<RequestIssueExecutionCommand> for IssueExecutionHandlers<S, F>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: RequestIssueExecutionCommand,
  ) -> Result<IssueExecutionOutcome, ApplicationError> {
    crate::issue::visible_issue(grant, command.intent.issue_id)?;
    crate::factory_admission::authorize_project_admission(self.policy.as_ref(), context, command.intent.project_id)
      .await?;
    crate::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Issue,
      command.intent.issue_id,
    )
    .await?;
    crate::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::FactoryConfiguration,
      command.intent.target.configuration_id,
    )
    .await?;
    crate::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Repository,
      command.intent.target.repository_id,
    )
    .await?;
    let request = crate::management_security::audited_mutation(
      context,
      RequestIssueExecution {
        id: octacity_server_domain::IssueExecutionRequestId::generate(),
        intent: command.intent,
        idempotency_key: command.idempotency_key,
        requested_at: command.requested_at,
      },
    )?;
    if let Some(replay) = self.store.replay_issue_execution(&request).await? {
      return Ok(replay);
    }
    let intent = &request.mutation().intent;
    let target = &intent.target;
    let selected = self
      .factory
      .factory_admission_context(octacity_server_store::ReadFactoryAdmissionContext {
        project_id: intent.project_id,
        configuration_id: target.configuration_id,
        configuration_version: target.configuration_version,
        repository_id: target.repository_id,
        repository_version: target.repository_version,
      })
      .await?;
    crate::factory_admission::validate_admission_project(&selected, intent.project_id)?;
    let mut actions =
      crate::factory_issue_status_authoring::flow_issue_status_selections(selected.configuration.configuration.flow())
        .peekable();
    if actions.peek().is_some() {
      let issue = self.store.issue(intent.project_id, intent.issue_id, None).await?;
      if issue.version != intent.expected_version {
        return Err(
          octacity_server_store::StoreError::Conflict {
            entity: octacity_server_domain::EntityKind::Issue,
          }
          .into(),
        );
      }

      for action in actions {
        let action = action?;
        if action.workflow_id != issue.workflow_id || action.workflow_version != issue.workflow_version {
          return Err(FactoryValidationDiagnostics::invalid("/target"));
        }
      }
    }

    crate::manual_trigger::select_source(&target.source, &selected.repository)
      .map_err(|_| ApplicationError::InvalidInput)?;
    Ok(self.store.request_issue_execution(request).await?)
  }
}
#[async_trait::async_trait]
impl<S: IssueExecutionStore, F: Send + Sync> ManagementQueryUseCase<GetIssueExecutionQuery>
  for IssueExecutionHandlers<S, F>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetIssueExecutionQuery,
  ) -> Result<IssueExecutionRequest, ApplicationError> {
    crate::issue::visible_issue(grant, query.issue_id)?;
    Ok(
      self
        .store
        .issue_execution(query.project_id, query.issue_id, query.request_id)
        .await?,
    )
  }
}

#[path = "issue_execution_history.rs"]
mod history;
pub use history::*;
