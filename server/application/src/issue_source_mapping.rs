//! Explicit ownership publication over installed Source/Reporter authority.
use super::*;
use octacity_server_factory::{ExternalWorkIdentity, ImmutableReference, WorkSourceBinding, WorkSourceReportingPolicy};
pub use octacity_server_store::{IssueSourceMappingDefinition, PublishIssueSourceMappingIntent};
use octacity_server_store::{IssueSourceMappingStore, PublishIssueSourceMapping};

/// Trusted installed readable normalized values, with an independently selected writer.
pub struct InstalledIssueSourceMapping {
  /// Permitted owner.
  pub project_id: ProjectId,
  /// Exact verified read profile.
  pub source: WorkSourceBinding,
  /// Verified native container, never a URL or display value.
  pub native_resource: ExternalWorkIdentity,
  /// Finite native values advertised by the installed read owner.
  pub readable: WorkSourceReportingPolicy,
  /// Separate installed write authority; absent for read-only deployments.
  pub writer: Option<InstalledIssueSourceWriter>,
}
/// Installed Reporter ceiling and shared current managed-resource grant checks.
pub struct InstalledIssueSourceWriter {
  /// Exact installed writer settings.
  pub binding: WorkSourceBinding,
  /// Content-verified installed executable.
  pub plugin: ImmutableReference,
  /// Existing independent TaskUpdate authority resolver.
  pub access: ManagedIntegrationAccess,
  /// Existing bounded managed effects, not a grant inferred from a read.
  pub effects: WorkSourceReportingPolicy,
}
impl InstalledIssueSourceWriter {
  pub(super) async fn authorize(
    &self,
    policy: &dyn ManagementAuthorizationPolicy,
    context: &ManagementRequestContext,
    project_id: ProjectId,
    native_resource: &ExternalWorkIdentity,
  ) -> Result<(), ApplicationError> {
    crate::factory_configuration_choices::require_visible_reference(
      policy,
      context,
      ManagementResourceKind::IntegrationConnection,
      self.access.connection_id(),
    )
    .await?;
    crate::factory_configuration_choices::require_authorized_reference(
      policy,
      context,
      ManagementAction::Execute,
      ManagementResourceKind::IntegrationConnection,
      self.access.connection_id(),
    )
    .await?;
    self
      .access
      .authorize_work_binding(
        project_id,
        &self.binding,
        &self.plugin,
        native_resource.as_str(),
        IntegrationPurpose::TaskUpdate,
      )
      .await
  }
}
impl<S> IssueSourceHandlers<S> {
  /// Installs bounded trusted mapping choices without starting synchronization.
  pub fn with_source_mappings(mut self, mappings: Vec<InstalledIssueSourceMapping>) -> Result<Self, ApplicationError> {
    if mappings.len() > octacity_server_factory::MAX_FACTORY_EXECUTOR_BINDINGS
      || mappings.iter().enumerate().any(|(i, m)| {
        mappings[..i]
          .iter()
          .any(|p| p.project_id == m.project_id && p.source == m.source)
      })
    {
      return Err(ApplicationError::invalid());
    }
    for m in &mappings {
      m.readable.validate().map_err(|_| ApplicationError::invalid())?;
    }
    self.mappings = mappings;
    Ok(self)
  }
}
/// Reviewed mapping publication; no task text or provider credential enters the intent.
#[derive(Clone, Debug)]
pub struct PublishIssueSourceMappingCommand {
  /// Frozen selection and exact review preconditions.
  pub intent: PublishIssueSourceMappingIntent,
  /// Original scoped operation key.
  pub idempotency_key: IdempotencyKey,
  /// Trusted first publication time.
  pub published_at: Timestamp,
}
impl Command for PublishIssueSourceMappingCommand {
  type Outcome = octacity_server_store::IssueSourceMappingOutcome;
}
impl ManagementAuthorizationTarget for PublishIssueSourceMappingCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Administer, ManagementResourceKind::Issue);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.intent.issue_id)
  }
}
#[async_trait::async_trait]
impl<S: IssueSourceMappingStore> ManagementCommandUseCase<PublishIssueSourceMappingCommand> for IssueSourceHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: PublishIssueSourceMappingCommand,
  ) -> Result<octacity_server_store::IssueSourceMappingOutcome, ApplicationError> {
    crate::issue::visible_issue(grant, command.intent.issue_id)?;
    self.visible(context, command.intent.issue_id).await?;
    let scope = context.security_scope().to_store();
    if let Some(replay) = self
      .store
      .issue_source_mapping_replay(&scope, &command.intent, &command.idempotency_key)
      .await?
    {
      return Ok(replay);
    }
    let installed = self
      .mappings
      .iter()
      .find(|m| m.project_id == command.intent.project_id && m.source == command.intent.definition.source)
      .ok_or_else(ApplicationError::capability_unavailable)?;
    self
      .reader
      .authorize(context, installed.project_id, installed.source.clone())
      .await?;
    let writable = if let Some(binding) = &command.intent.definition.reporter {
      let writer = installed
        .writer
        .as_ref()
        .filter(|w| w.binding == *binding)
        .ok_or_else(ApplicationError::capability_unavailable)?;
      if command.intent.definition.push_enabled {
        writer
          .authorize(
            self.policy.as_ref(),
            context,
            installed.project_id,
            &installed.native_resource,
          )
          .await?;
      }
      Some(writer.effects.clone())
    } else {
      None
    };
    Ok(
      self
        .store
        .publish_issue_source_mapping(crate::management_security::audited_mutation(
          context,
          PublishIssueSourceMapping {
            intent: command.intent,
            native_resource: installed.native_resource.clone(),
            readable: installed.readable.clone(),
            writable,
            idempotency_key: command.idempotency_key,
            published_at: self.clock.now()?.max(command.published_at),
          },
        )?)
        .await?,
    )
  }
}
