//! Authorized Project business-policy management, independent of execution and providers.
use crate::*;
use octacity_server_domain::{IssueWorkflowId, IssueWorkflowVersion, ProjectId, Timestamp};
use octacity_server_store::{IdempotencyKey, PublishIssueWorkflow, SelectDefaultIssueWorkflow};
pub use octacity_server_store::{
  IssueWorkflowDefault, IssueWorkflowDefinition, IssueWorkflowPublicationOutcome, IssueWorkflowSelectionOutcome,
  IssueWorkflowStore, PublishIssueWorkflowIntent, PublishedIssueWorkflow, SelectDefaultIssueWorkflowIntent,
};
use std::sync::Arc;

/// Reads a default without requiring workflow administration or initializing state.
#[derive(Clone, Debug)]
pub struct GetDefaultIssueWorkflowQuery {
  /// Visible Project whose new Issues use the selected workflow.
  pub project_id: ProjectId,
}
impl Query for GetDefaultIssueWorkflowQuery {
  type Outcome = IssueWorkflowDefault;
}

/// Reads one retained version or the current immutable publication.
#[derive(Clone, Debug)]
pub struct GetIssueWorkflowQuery {
  /// Owning Project; foreign identities cannot escape this scope.
  pub project_id: ProjectId,
  /// Stable workflow identity.
  pub id: IssueWorkflowId,
  /// Exact retained version, or current publication when absent.
  pub version: Option<IssueWorkflowVersion>,
}
impl Query for GetIssueWorkflowQuery {
  type Outcome = PublishedIssueWorkflow;
}
impl ManagementAuthorizationTarget for GetIssueWorkflowQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetDefaultIssueWorkflowQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    GetDefaultIssueWorkflowQuery {
      project_id: self.project_id,
    }
    .management_resource()
  }
}
/// Explicit policy publication, separately authorized from ordinary creation/reading.
#[derive(Clone, Debug)]
pub struct PublishIssueWorkflowCommand {
  /// Frozen bounded policy and version precondition.
  pub intent: PublishIssueWorkflowIntent,
  /// Scoped replay key.
  pub idempotency_key: IdempotencyKey,
  /// Original authoritative publication time.
  pub published_at: Timestamp,
}
impl Command for PublishIssueWorkflowCommand {
  type Outcome = IssueWorkflowPublicationOutcome;
}

/// Selects an exact default for new Issues, without moving existing Issues or running a pipeline.
#[derive(Clone, Debug)]
pub struct SelectDefaultIssueWorkflowCommand {
  /// Original reviewed selection and current selection version.
  pub intent: SelectDefaultIssueWorkflowIntent,
  /// Scoped replay key.
  pub idempotency_key: IdempotencyKey,
}
impl Command for SelectDefaultIssueWorkflowCommand {
  type Outcome = IssueWorkflowSelectionOutcome;
}
impl ManagementAuthorizationTarget for SelectDefaultIssueWorkflowCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = PublishIssueWorkflowCommand::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    GetDefaultIssueWorkflowQuery {
      project_id: self.intent.project_id,
    }
    .management_resource()
  }
}
#[async_trait::async_trait]
impl<S: IssueWorkflowStore> ManagementCommandUseCase<SelectDefaultIssueWorkflowCommand> for IssueWorkflowHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    command: SelectDefaultIssueWorkflowCommand,
  ) -> Result<IssueWorkflowSelectionOutcome, ApplicationError> {
    Ok(
      self
        .store
        .select_default_issue_workflow(crate::management_security::audited_mutation(
          context,
          SelectDefaultIssueWorkflow {
            intent: command.intent,
            idempotency_key: command.idempotency_key,
          },
        )?)
        .await?,
    )
  }
}
impl ManagementAuthorizationTarget for PublishIssueWorkflowCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::owned_collection(
    ManagementAction::Administer,
    ManagementResourceKind::IssueWorkflow,
    ManagementResourceKind::Project,
  );
  fn management_resource(&self) -> ManagementResourceResult {
    GetDefaultIssueWorkflowQuery {
      project_id: self.intent.project_id,
    }
    .management_resource()
  }
}
#[async_trait::async_trait]
impl<S: IssueWorkflowStore> ManagementQueryUseCase<GetIssueWorkflowQuery> for IssueWorkflowHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: GetIssueWorkflowQuery,
  ) -> Result<PublishedIssueWorkflow, ApplicationError> {
    Ok(
      self
        .store
        .issue_workflow(query.project_id, query.id, query.version)
        .await?,
    )
  }
}
#[async_trait::async_trait]
impl<S: IssueWorkflowStore> ManagementCommandUseCase<PublishIssueWorkflowCommand> for IssueWorkflowHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    command: PublishIssueWorkflowCommand,
  ) -> Result<IssueWorkflowPublicationOutcome, ApplicationError> {
    Ok(
      self
        .store
        .publish_issue_workflow(crate::management_security::audited_mutation(
          context,
          PublishIssueWorkflow {
            intent: command.intent,
            idempotency_key: command.idempotency_key,
            published_at: command.published_at,
          },
        )?)
        .await?,
    )
  }
}
impl ManagementAuthorizationTarget for GetDefaultIssueWorkflowQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::owned_collection(
    ManagementAction::View,
    ManagementResourceKind::IssueWorkflow,
    ManagementResourceKind::Project,
  );
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::owned_collection_resource(
      ManagementResourceKind::IssueWorkflow,
      ManagementResourceKind::Project,
      self.project_id,
    )
  }
}

/// Small policy handlers backed by the same Project authority as manual tasks.
pub struct IssueWorkflowHandlers<S> {
  store: Arc<S>,
}

/// Validates a proposed bounded business workflow without publication or execution.
#[derive(Clone, Debug)]
pub struct ValidateIssueWorkflowQuery {
  /// Visible owning Project.
  pub project_id: ProjectId,
  /// Untrusted proposed business policy, never a script or execution graph.
  pub definition: IssueWorkflowDefinition,
}
impl Query for ValidateIssueWorkflowQuery {
  type Outcome = IssueWorkflowDefinition;
}
impl ManagementAuthorizationTarget for ValidateIssueWorkflowQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetDefaultIssueWorkflowQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    GetDefaultIssueWorkflowQuery {
      project_id: self.project_id,
    }
    .management_resource()
  }
}
#[async_trait::async_trait]
impl<S: IssueWorkflowStore> ManagementQueryUseCase<ValidateIssueWorkflowQuery> for IssueWorkflowHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: ValidateIssueWorkflowQuery,
  ) -> Result<IssueWorkflowDefinition, ApplicationError> {
    self.store.default_issue_workflow(query.project_id).await?;
    query.definition.validate()?;
    Ok(query.definition)
  }
}
impl<S> IssueWorkflowHandlers<S> {
  /// Composes one backend-neutral workflow port; no installed provider is needed.
  pub fn new(store: Arc<S>) -> Self {
    Self { store }
  }
}
#[async_trait::async_trait]
impl<S: IssueWorkflowStore> ManagementQueryUseCase<GetDefaultIssueWorkflowQuery> for IssueWorkflowHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: GetDefaultIssueWorkflowQuery,
  ) -> Result<IssueWorkflowDefault, ApplicationError> {
    Ok(self.store.default_issue_workflow(query.project_id).await?)
  }
}
