//! Authorized installed choices and side-effect-free qualification over the shared catalog.
use crate::*;
use octacity_server_domain::ProjectId;
use octacity_server_factory::*;
use octacity_server_store::{
  FactoryConfigurationStore, FactoryRoleProfileStore, IntegrationAdministrationStore, IntegrationConnectionStore,
  IntegrationProjectAccessStore, IntegrationResourceAccess,
};
use std::collections::BTreeMap;

/// Bounded compatible technical-profile catalog; dependencies are filtered before counts/cursors.
#[derive(Clone, Debug)]
pub struct ListFactoryRoleProfileCapabilitiesQuery {
  /// Uses the existing Project page bound and stable alias continuation.
  pub page: CurrentDefinitionPageInput<FactoryKey>,
}
impl Query for ListFactoryRoleProfileCapabilitiesQuery {
  type Outcome = FactoryRoleProfileChoicePage;
}
impl ManagementAuthorizationTarget for ListFactoryRoleProfileCapabilitiesQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetFactoryRoleQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    management_security::instance_resource(ManagementResourceKind::Project, self.page.project_id())
  }
}
/// Safe installed selections. Instructions, environment values, secret locators and argv are absent.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct FactoryRoleProfileChoice {
  /// Authorized runtime/profile alias from the existing Factory choice catalog.
  pub alias: FactoryKey,
  /// Exact runtime/plugin/model pins; no provider-specific settings enum.
  pub toolchain: FactoryTaskToolchain,
  /// Ordinary compatible Project-owned Build configuration.
  pub build_configuration: BuildConfigurationRef,
  /// Supported model/thinking values, using the common closed schema.
  pub parameters: FlowDataSchema,
  /// Supported public environment names and value shapes.
  pub public_environment: BTreeMap<String, FlowDataSchema>,
  /// Secret entry names only; logical references stay in dedicated content operations.
  pub secret_environment_names: Vec<String>,
  /// Exact selectable skills.
  pub skills: BTreeMap<FactoryKey, ImmutableReference>,
  /// Exact installed MCP selections, without commands or connection credentials.
  pub mcp_servers: BTreeMap<FactoryKey, FactoryRoleMcpBinding>,
  /// Exact independently managed integration selections.
  pub integrations: BTreeMap<FactoryKey, FactoryRoleIntegrationBinding>,
  /// Installed credential consumer purpose.
  pub credential_purpose: FactoryCredentialPurpose,
  /// Authorized tool identities only; command patterns stay in the dedicated content read.
  pub tools: Vec<ImmutableReference>,
  /// Maximum selectable usage.
  pub budget: BudgetLimit,
}
/// Total visible entries and bounded alias-ordered results.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize)]
pub struct FactoryRoleProfileChoicePage {
  /// Visible count before continuation.
  pub total: u64,
  /// Safe selectable declarations.
  pub items: Vec<FactoryRoleProfileChoice>,
  /// Exclusive alias when more visible choices remain.
  pub next_cursor: Option<FactoryKey>,
}
/// One fully specified candidate; no version, Build or provider operation is published.
#[derive(Clone, Debug)]
pub struct ValidateFactoryRoleProfileQuery {
  /// Authorized owning Project.
  pub project_id: ProjectId,
  /// Caller-owned candidate profile identity.
  pub id: FactoryRoleProfileId,
  /// Exact shared instruction version, checked before compatibility disclosure.
  pub role: FactoryRoleRef,
  /// Explicit installed technical-profile alias.
  pub capability: FactoryKey,
  /// Complete reviewed candidate settings.
  pub definition: FactoryRoleProfileDefinition,
}
impl Query for ValidateFactoryRoleProfileQuery {
  type Outcome = FactoryRoleProfileValidation;
}
impl ManagementAuthorizationTarget for ValidateFactoryRoleProfileQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetFactoryRoleQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    management_security::instance_resource(ManagementResourceKind::Project, self.project_id)
  }
}
/// Normalized exact content or a bounded safe incompatibility category.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct FactoryRoleProfileValidation {
  /// Candidate with canonical digest; not a persisted publication or execution grant.
  pub profile: Option<FactoryRoleProfile>,
  /// Safe reason without submitted values or hidden dependency identities.
  pub incompatibility: Option<FactoryRoleProfileIncompatibility>,
}

fn projection(alias: FactoryKey, cap: &FactoryRoleProfileCapability) -> FactoryRoleProfileChoice {
  let definition = &cap.ceiling;
  FactoryRoleProfileChoice {
    alias,
    toolchain: definition.toolchain.clone(),
    build_configuration: definition.build_configuration.clone(),
    parameters: cap.parameters.clone(),
    public_environment: cap.public_environment.clone(),
    secret_environment_names: definition
      .execution
      .environment
      .iter()
      .filter(|(_, value)| matches!(value, FactoryRoleEnvironmentValue::Secret { .. }))
      .map(|(name, _)| name.clone())
      .collect(),
    skills: definition.skills.clone(),
    mcp_servers: definition.execution.mcp_servers.clone(),
    integrations: definition.execution.integrations.clone(),
    credential_purpose: definition.credential_purpose,
    tools: definition.permissions.tools().cloned().collect(),
    budget: definition.budget,
  }
}
#[async_trait::async_trait]
impl<
  S: FactoryConfigurationStore
    + IntegrationProjectAccessStore
    + IntegrationConnectionStore
    + IntegrationAdministrationStore
    + 'static,
> ManagementQueryUseCase<ListFactoryRoleProfileCapabilitiesQuery> for FactoryRoleProfileHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListFactoryRoleProfileCapabilitiesQuery,
  ) -> Result<FactoryRoleProfileChoicePage, ApplicationError> {
    crate::factory_role_profiles::require_project_visibility(grant, query.page.project_id())?;
    let choices = crate::factory_configuration_choices::visible_factory_choices(
      self.store.as_ref(),
      self.policy.as_ref(),
      context,
      query.page.project_id(),
    )
    .await?;
    let mut items = Vec::new();
    for (alias, cap) in choices.role_profile_capabilities() {
      let cap = self
        .available_profile_capability(context, query.page.project_id(), cap.clone())
        .await?;
      items.push(projection(alias.clone(), &cap));
    }
    let total = items.len() as u64;
    items.retain(|item| query.page.after().is_none_or(|after| &item.alias > after));
    let more = items.len() > usize::from(query.page.limit());
    items.truncate(usize::from(query.page.limit()));
    let next_cursor = more.then(|| items.last().unwrap().alias.clone());
    Ok(FactoryRoleProfileChoicePage {
      total,
      items,
      next_cursor,
    })
  }
}
#[async_trait::async_trait]
impl<
  S: FactoryConfigurationStore
    + FactoryRoleProfileStore
    + IntegrationProjectAccessStore
    + IntegrationConnectionStore
    + IntegrationAdministrationStore
    + 'static,
> ManagementQueryUseCase<ValidateFactoryRoleProfileQuery> for FactoryRoleProfileHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ValidateFactoryRoleProfileQuery,
  ) -> Result<FactoryRoleProfileValidation, ApplicationError> {
    crate::factory_role_profiles::require_project_visibility(grant, query.project_id)?;
    let role = self
      .store
      .factory_role(query.project_id, query.role.id, Some(query.role.version))
      .await?;
    if role.reference() != &query.role {
      return Err(ApplicationError::invalid());
    }
    let profile = FactoryRoleProfile::publish(query.id, query.project_id, &role, query.definition)
      .map_err(|_| ApplicationError::invalid())?;
    let cap = self
      .profile_capability(context, query.project_id, &query.capability)
      .await?;
    let incompatibility = cap.validate(&profile).err();
    Ok(FactoryRoleProfileValidation {
      profile: incompatibility.is_none().then_some(profile),
      incompatibility,
    })
  }
}

impl<S: IntegrationProjectAccessStore + IntegrationConnectionStore + IntegrationAdministrationStore>
  FactoryRoleProfileHandlers<S>
{
  // Prune optional selections once for discovery, details and candidate validation.
  // The submitted candidate remains unchanged and must fit this available ceiling.
  async fn available_profile_capability(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    mut cap: FactoryRoleProfileCapability,
  ) -> Result<FactoryRoleProfileCapability, ApplicationError> {
    for (alias, binding) in std::mem::take(&mut cap.ceiling.execution.integrations) {
      if self.profile_integration_available(context, project, &binding).await? {
        cap.ceiling.execution.integrations.insert(alias, binding);
      }
    }
    cap.ceiling.execution.mcp_servers.retain(|_, binding| {
      binding
        .integration
        .as_ref()
        .is_none_or(|alias| cap.ceiling.execution.integrations.contains_key(alias))
    });
    Ok(cap)
  }
  // Reuses the existing protected-use owner: current grants, exact revisions,
  // configured credentials and resource purpose, with no remote/secret reads.
  async fn profile_integration_available(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    binding: &FactoryRoleIntegrationBinding,
  ) -> Result<bool, ApplicationError> {
    if binding.project_id != project {
      return Ok(false);
    }
    let resource =
      management_security::instance_resource(ManagementResourceKind::IntegrationConnection, binding.connection_id)
        .map_err(|_| ApplicationError::invalid())?;
    let Ok(grant) = self.policy.authorize(context, ManagementAction::View, &resource).await else {
      return Ok(false);
    };
    if !matches!(grant.visibility().view(), ManagementVisibilityView::All)
      && !matches!(grant.visibility().view(), ManagementVisibilityView::Restricted(resources) if resources.contains(&resource))
    {
      return Ok(false);
    }
    let Some(access) = self
      .store
      .integration_project_access(binding.connection_id, project)
      .await?
    else {
      return Ok(false);
    };
    if access.version != binding.access_version || access.connection_version != binding.connection_version {
      return Ok(false);
    }
    let metadata = self.store.integration_connection(binding.connection_id, None).await?;
    if metadata.version != binding.connection_version {
      return Ok(false);
    }
    let Some(capability) = metadata
      .definition
      .resources
      .iter()
      .find(|r| r.reference == binding.resource)
      .and_then(|r| r.capabilities.iter().find(|c| c.capability == binding.capability))
    else {
      return Ok(false);
    };
    let request = IntegrationResourceAccess {
      connection_id: binding.connection_id,
      connection_version: binding.connection_version,
      project_id: project,
      resource: binding.resource.clone(),
      purpose: capability.purpose,
    };
    match authorize_integration_resource(self.store.as_ref(), &request).await {
      Ok(_) => {}
      Err(ApplicationError::CapabilityUnavailable) => return Ok(false),
      Err(error) => return Err(error),
    }
    Ok(true)
  }
}

/// Dedicated authorized content read for one selectable installed technical profile.
/// Transport adapters must treat the returned settings as private authoring content.
#[derive(Clone, Debug)]
pub struct GetFactoryRoleProfileCapabilityQuery {
  /// Visible owning Project.
  pub project_id: ProjectId,
  /// Current authorized runtime/profile alias.
  pub capability: FactoryKey,
}
impl Query for GetFactoryRoleProfileCapabilityQuery {
  type Outcome = FactoryRoleProfileCapabilityDetails;
}
impl ManagementAuthorizationTarget for GetFactoryRoleProfileCapabilityQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetFactoryRoleQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    management_security::instance_resource(ManagementResourceKind::Project, self.project_id)
  }
}
/// Complete installed authoring settings; not a credential resolution or execution grant.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct FactoryRoleProfileCapabilityDetails {
  /// Shared ordinary profile shape, including approved argv and logical secret references.
  pub definition: FactoryRoleProfileDefinition,
  /// Supported model parameter contract.
  pub parameters: FlowDataSchema,
  /// Supported public environment names and value contracts.
  pub public_environment: BTreeMap<String, FlowDataSchema>,
}
#[async_trait::async_trait]
impl<
  S: FactoryConfigurationStore
    + IntegrationProjectAccessStore
    + IntegrationConnectionStore
    + IntegrationAdministrationStore
    + 'static,
> ManagementQueryUseCase<GetFactoryRoleProfileCapabilityQuery> for FactoryRoleProfileHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetFactoryRoleProfileCapabilityQuery,
  ) -> Result<FactoryRoleProfileCapabilityDetails, ApplicationError> {
    super::factory_role_profiles::require_project_visibility(grant, query.project_id)?;
    let cap = self
      .profile_capability(context, query.project_id, &query.capability)
      .await?;
    Ok(FactoryRoleProfileCapabilityDetails {
      definition: cap.ceiling,
      parameters: cap.parameters,
      public_environment: cap.public_environment,
    })
  }
}
impl<
  S: FactoryConfigurationStore
    + IntegrationProjectAccessStore
    + IntegrationConnectionStore
    + IntegrationAdministrationStore,
> FactoryRoleProfileHandlers<S>
{
  async fn profile_capability(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    alias: &FactoryKey,
  ) -> Result<FactoryRoleProfileCapability, ApplicationError> {
    let choices = crate::factory_configuration_choices::visible_factory_choices(
      self.store.as_ref(),
      self.policy.as_ref(),
      context,
      project,
    )
    .await?;
    let cap = choices
      .role_profile_capabilities()
      .get(alias)
      .ok_or_else(ApplicationError::capability_unavailable)?;
    self.available_profile_capability(context, project, cap.clone()).await
  }
}
