//! Authorized Project Runtime settings; publication never installs an executor or performs remote I/O.
use crate::*;
use octacity_server_domain::ProjectId;
use octacity_server_factory::*;
pub use octacity_server_factory::{
  MAX_FACTORY_MCP_ARGUMENT_BYTES, MAX_FACTORY_MCP_CONFIGURATION_BYTES, MAX_FACTORY_MCP_SELECTION_ENTRIES,
  MAX_FACTORY_MCP_SERVER_NAME_BYTES, MAX_FACTORY_MCP_TOOL_NAME_BYTES, MAX_PERMISSION_PATH_BYTES,
};
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
    self.qualify_runtime_settings(context, &command.intent).await?;

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

/// Side-effect-free review of the same original intent accepted by publication.
#[derive(Clone, Debug)]
pub struct ValidateFactoryRuntimeSettingsQuery {
  /// Project and Runtime-scoped candidate, including its displayed revision.
  pub intent: FactoryRuntimeSettingsIntent,
}
impl Query for ValidateFactoryRuntimeSettingsQuery {
  type Outcome = FactoryRuntimeSettings;
}
impl ManagementAuthorizationTarget for ValidateFactoryRuntimeSettingsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = PublishFactoryRuntimeSettingsCommand::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    management_security::instance_resource(ManagementResourceKind::Project, self.intent.project_id)
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
> ManagementQueryUseCase<ValidateFactoryRuntimeSettingsQuery> for FactoryRoleProfileHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ValidateFactoryRuntimeSettingsQuery,
  ) -> Result<FactoryRuntimeSettings, ApplicationError> {
    crate::factory_role_profiles::require_project_visibility(grant, query.intent.project_id)?;
    self.qualify_runtime_settings(context, &query.intent).await
  }
}
impl<
  S: FactoryRuntimeSettingsStore
    + FactoryConfigurationStore
    + IntegrationProjectAccessStore
    + IntegrationConnectionStore
    + IntegrationAdministrationStore
    + 'static,
> FactoryRoleProfileHandlers<S>
{
  // Review and save share installed limits, transport shape and protected integration checks.
  // Publication still owns the final atomic version/capacity checks after this read-only review.
  async fn qualify_runtime_settings(
    &self,
    context: &ManagementRequestContext,
    intent: &FactoryRuntimeSettingsIntent,
  ) -> Result<FactoryRuntimeSettings, ApplicationError> {
    let settings = intent.settings()?;
    let current = self
      .store
      .factory_runtime_settings(intent.project_id, &intent.runtime, None)
      .await?;
    if current.as_ref().map(FactoryRuntimeSettings::version) != intent.expected_version {
      return Err(ApplicationError::PreconditionFailed);
    }
    let choices = crate::factory_configuration_choices::visible_installed_factory_choices(
      self.store.as_ref(),
      self.policy.as_ref(),
      context,
      intent.project_id,
    )
    .await?;
    let cap = choices
      .role_profile_capabilities()
      .get(&intent.runtime)
      .ok_or_else(ApplicationError::capability_unavailable)?;
    if intent.definition.toolchain != cap.ceiling.toolchain {
      return Err(ApplicationError::invalid());
    }
    let definition = &intent.definition;
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
        .profile_integration_available(context, intent.project_id, binding)
        .await?
      {
        return Err(ApplicationError::capability_unavailable());
      }
    }
    let support = choices.runtime_settings_support(&intent.runtime);
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

    Ok(settings)
  }
}

/// Dedicated bounded administration catalog; private executable paths never enter generic discovery.
#[derive(Clone, Debug)]
pub struct ListFactoryRuntimeChoicesQuery {
  /// Existing Project page contract, continued by installed Runtime alias.
  pub page: CurrentDefinitionPageInput<FactoryKey>,
}
impl Query for ListFactoryRuntimeChoicesQuery {
  type Outcome = FactoryRuntimeChoicePage;
}
impl ManagementAuthorizationTarget for ListFactoryRuntimeChoicesQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = PublishFactoryRuntimeSettingsCommand::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    management_security::instance_resource(ManagementResourceKind::Project, self.page.project_id())
  }
}
/// Installed support and explicit selectable names; no credentials or model discovery snapshot.
#[derive(Clone, Debug, serde::Serialize)]
pub struct FactoryRuntimeChoice {
  /// Existing installed alias, not a second Runtime identity.
  pub alias: FactoryKey,
  /// Protected installation pins adopted explicitly by a reviewed settings revision.
  pub toolchain: FactoryTaskToolchain,
  /// Qualified transports and executable choices.
  pub support: FactoryRuntimeSettingsSupport,
  /// Installed permissions intersected with independently owned local grants.
  pub permissions: FactoryPermissionSet,
  /// Independently advertised backend enforcement; configuration cannot change it.
  pub enforcement: Vec<PermissionCategory>,
  /// Explicit public/secret environment names, without any values or secret locators.
  pub environment_names: Vec<String>,
  /// Subset eligible as logical bearer mappings.
  pub secret_environment_names: Vec<String>,
}
/// Visible count and alias continuation use the existing catalog pagination shape.
#[derive(Clone, Debug, serde::Serialize)]
pub struct FactoryRuntimeChoicePage {
  /// Count after authorization, before continuation.
  pub total: u64,
  /// Bounded administration choices; integrations reuse their existing Project resource catalog.
  pub items: Vec<FactoryRuntimeChoice>,
  /// Exclusive visible alias when more results remain.
  pub next_cursor: Option<FactoryKey>,
}
#[async_trait::async_trait]
impl<S: FactoryConfigurationStore> ManagementQueryUseCase<ListFactoryRuntimeChoicesQuery>
  for FactoryRoleProfileHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListFactoryRuntimeChoicesQuery,
  ) -> Result<FactoryRuntimeChoicePage, ApplicationError> {
    crate::factory_role_profiles::require_project_visibility(grant, query.page.project_id())?;
    let choices = crate::factory_configuration_choices::visible_installed_factory_choices(
      self.store.as_ref(),
      self.policy.as_ref(),
      context,
      query.page.project_id(),
    )
    .await?;
    let total = choices.role_profile_capabilities().len() as u64;
    let mut items: Vec<_> = choices
      .role_profile_capabilities()
      .iter()
      .filter(|(alias, _)| query.page.after().is_none_or(|after| *alias > after))
      .take(usize::from(query.page.limit()) + 1)
      .map(|(alias, cap)| FactoryRuntimeChoice {
        alias: alias.clone(),
        toolchain: cap.ceiling.toolchain.clone(),
        support: choices.runtime_settings_support(alias).cloned().unwrap_or_default(),
        permissions: cap.ceiling.permissions.intersection(cap.local_permissions.grants()),
        enforcement: PermissionCategory::ALL
          .into_iter()
          .filter(|category| cap.local_permissions.can_enforce(*category))
          .collect(),
        environment_names: cap
          .public_environment
          .keys()
          .chain(cap.ceiling.execution.environment.keys())
          .cloned()
          .collect::<std::collections::BTreeSet<_>>()
          .into_iter()
          .collect(),
        secret_environment_names: cap
          .ceiling
          .execution
          .environment
          .iter()
          .filter_map(|(name, value)| {
            matches!(value, FactoryRoleEnvironmentValue::Secret { .. }).then_some(name.clone())
          })
          .collect(),
      })
      .collect();
    let more = items.len() > usize::from(query.page.limit());
    items.truncate(usize::from(query.page.limit()));
    let next_cursor = more.then(|| items.last().unwrap().alias.clone());
    Ok(FactoryRuntimeChoicePage {
      total,
      items,
      next_cursor,
    })
  }
}
