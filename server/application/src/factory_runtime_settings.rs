//! Authorized Project Runtime settings; publication never installs an executor or performs remote I/O.
use crate::*;
use octacity_server_domain::ProjectId;
use octacity_server_factory::*;
use octacity_server_store::{
  FactoryConfigurationStore, FactoryRuntimeSettingsStore, IntegrationAdministrationStore, IntegrationConnectionStore,
  IntegrationProjectAccessStore,
};
pub use octacity_server_store::{FactoryRuntimeSettingsIntent, FactoryRuntimeSettingsOutcome};

/// Reviewed configuration of one already installed Runtime alias.
#[derive(Clone, Debug)]
pub struct PublishFactoryRuntimeSettingsCommand {
  /// Exact client business intent, used unchanged for replay.
  pub intent: FactoryRuntimeSettingsIntent,
  /// Stable scoped mutation key.
  pub idempotency_key: octacity_server_store::IdempotencyKey,
  /// Trusted first acceptance time.
  pub published_at: octacity_server_domain::Timestamp,
}
impl Command for PublishFactoryRuntimeSettingsCommand {
  type Outcome = FactoryRuntimeSettingsOutcome;
}
impl ManagementAuthorizationTarget for PublishFactoryRuntimeSettingsCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Administer, ManagementResourceKind::Project);
  fn management_resource(&self) -> ManagementResourceResult {
    management_security::instance_resource(ManagementResourceKind::Project, self.intent.project_id)
  }
}
/// Dedicated content read; private transport details stay out of generic discovery/audit.
#[derive(Clone, Debug)]
pub struct GetFactoryRuntimeSettingsQuery {
  /// Visible owning Project.
  pub project_id: ProjectId,
  /// Installed Runtime alias.
  pub runtime: FactoryKey,
  /// Exact retained revision, or current when absent.
  pub version: Option<FactoryRuntimeSettingsVersion>,
}
impl Query for GetFactoryRuntimeSettingsQuery {
  type Outcome = Option<FactoryRuntimeSettings>;
}
impl ManagementAuthorizationTarget for GetFactoryRuntimeSettingsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetFactoryRoleQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    management_security::instance_resource(ManagementResourceKind::Project, self.project_id)
  }
}
#[async_trait::async_trait]
impl<S: FactoryRuntimeSettingsStore> ManagementQueryUseCase<GetFactoryRuntimeSettingsQuery>
  for FactoryRoleProfileHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetFactoryRuntimeSettingsQuery,
  ) -> Result<Option<FactoryRuntimeSettings>, ApplicationError> {
    crate::factory_role_profiles::require_project_visibility(grant, query.project_id)?;
    Ok(
      self
        .store
        .factory_runtime_settings(query.project_id, &query.runtime, query.version)
        .await?,
    )
  }
}
#[async_trait::async_trait]
impl<
  S: FactoryRuntimeSettingsStore
    + FactoryConfigurationStore
    + IntegrationProjectAccessStore
    + IntegrationConnectionStore
    + IntegrationAdministrationStore
    + 'static,
> ManagementCommandUseCase<PublishFactoryRuntimeSettingsCommand> for FactoryRoleProfileHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: PublishFactoryRuntimeSettingsCommand,
  ) -> Result<FactoryRuntimeSettingsOutcome, ApplicationError> {
    use octacity_server_store::{PublishFactoryRuntimeSettings, ReplayFactoryRuntimeSettings};
    crate::factory_role_profiles::require_project_visibility(grant, command.intent.project_id)?;
    if let Some(prior) = self
      .store
      .replay_factory_runtime_settings(management_security::audited_mutation(
        context,
        ReplayFactoryRuntimeSettings {
          intent: command.intent.clone(),
          idempotency_key: command.idempotency_key.clone(),
        },
      )?)
      .await?
    {
      return Ok(prior);
    }
    command.intent.settings()?;
    let choices = crate::factory_configuration_choices::visible_installed_factory_choices(
      self.store.as_ref(),
      self.policy.as_ref(),
      context,
      command.intent.project_id,
    )
    .await?;
    let cap = choices
      .role_profile_capabilities()
      .get(&command.intent.runtime)
      .ok_or_else(ApplicationError::capability_unavailable)?;
    if command.intent.definition.toolchain != cap.ceiling.toolchain {
      return Err(ApplicationError::invalid());
    }
    let definition = &command.intent.definition;
    let effective = resolve_factory_permissions(
      &cap.ceiling.permissions,
      &cap.ceiling.permissions,
      &definition.permissions,
      &cap.local_permissions,
    )
    .map_err(|_| ApplicationError::invalid())?;
    if effective != definition.permissions {
      return Err(ApplicationError::invalid());
    }
    for binding in definition.integrations.values() {
      if !self
        .profile_integration_available(context, command.intent.project_id, binding)
        .await?
      {
        return Err(ApplicationError::capability_unavailable());
      }
    }
    let support = choices.runtime_settings_support(&command.intent.runtime);
    for entry in definition.mcp_servers.values() {
      let support = support
        .filter(|s| s.mcp_transports.contains(&entry.server.transport()))
        .ok_or_else(ApplicationError::capability_unavailable)?;
      let endpoint = crate::factory_mcp::validate_transport(&entry.server).map_err(|_| ApplicationError::invalid())?;
      match &entry.server {
        FactoryMcpServer::Http {
          bearer_token_env_var, ..
        } => {
          let url = endpoint.ok_or_else(ApplicationError::invalid)?;
          let host = match url.host().ok_or_else(ApplicationError::invalid)? {
            url::Host::Domain(host) => host.to_owned(),
            url::Host::Ipv4(address) => address.to_string(),
            url::Host::Ipv6(address) => address.to_string(),
          };
          if !definition
            .permissions
            .network_hosts()
            .any(|allowed| allowed.as_str() == host)
            || bearer_token_env_var.as_ref().is_some_and(|name| {
              !matches!(
                cap.ceiling.execution.environment.get(name),
                Some(FactoryRoleEnvironmentValue::Secret { .. })
              )
            })
          {
            return Err(ApplicationError::invalid());
          }
        }
        FactoryMcpServer::Stdio {
          command,
          args,
          env_vars,
          ..
        } => {
          let executable = support
            .mcp_executables
            .get(command)
            .ok_or_else(ApplicationError::capability_unavailable)?;
          let arguments = args
            .iter()
            .map(|arg| FactoryText::new(arg).map(CommandArgumentPattern::Exact))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ApplicationError::invalid())?;
          let requested = FactoryPermissionSet::try_new(FactoryPermissionDraft {
            executables: vec![executable.clone()],
            commands: vec![
              CommandPermission::try_new(executable.clone(), arguments).map_err(|_| ApplicationError::invalid())?,
            ],
            ..Default::default()
          })
          .map_err(|_| ApplicationError::invalid())?;
          if requested.intersection(&definition.permissions) != requested
            || env_vars.iter().any(|name| {
              !cap.ceiling.execution.environment.contains_key(name) && !cap.public_environment.contains_key(name)
            })
          {
            return Err(ApplicationError::invalid());
          }
        }
      }
    }

    Ok(
      self
        .store
        .publish_factory_runtime_settings(management_security::audited_mutation(
          context,
          PublishFactoryRuntimeSettings {
            intent: command.intent,
            idempotency_key: command.idempotency_key,
            published_at: command.published_at,
          },
        )?)
        .await?,
    )
  }
}
