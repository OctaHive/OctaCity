//! Project policy management shares accepted authorization and the durable policy journal.
use crate::*;
use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::*;
use octacity_server_store::{FactoryConfigurationStore, FactoryProjectPolicyStore, PublishFactoryProjectPolicy};
pub use octacity_server_store::{
  FactoryProjectPolicyDefaults, FactoryProjectPolicyIntent, FactoryProjectPolicyOutcome, FactoryProjectPolicyRecord,
  FactoryProjectPolicyRef,
};

/// Read-only review of a policy and the installation envelope that qualified it.
#[derive(Debug, serde::Serialize)]
pub struct FactoryProjectPolicyValidation {
  /// Exact proposed publication.
  pub policy: FactoryProjectPolicyRecord,
  /// A verified installed ceiling for nonempty access profiles.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub installed_ceiling: Option<FactoryPolicyInstalledCeiling>,
}

/// The complete installed declaration that covered the requested profile.
#[derive(Debug, serde::Serialize)]
pub struct FactoryPolicyInstalledCeiling {
  /// Operator-facing installed choice name.
  pub alias: FactoryKey,
  /// Exact installed version.
  pub reference: ImmutableReference,
  /// Declared upper bound when all its dependencies are visible.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub permissions: Option<FactoryPermissionSet>,
}

/// Editable contents; the server computes publication versions and content digests.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FactoryProjectPolicyDraft {
  /// Manual/automatic launch rules under a stable Project-owned identity.
  Launch {
    /// Stable identity chosen for this policy.
    id: FactoryLaunchPolicyId,
    /// Exact displayed head, absent only for a new identity.
    expected_current: Option<FactoryLaunchPolicyRef>,
    /// Reviewed editable rules.
    definition: FactoryLaunchPolicyDefinition,
  },
  /// Typed execution ceiling; publication attaches no runtime, credential or input.
  Access {
    /// Stable profile identity.
    id: FactoryAccessProfileId,
    /// Exact displayed head, absent only for a new identity.
    expected_current: Option<FactoryAccessProfileRef>,
    /// Existing permission declarations, without credential values.
    definition: Box<FactoryAccessProfileDefinition>,
  },
}
impl FactoryProjectPolicyDraft {
  /// Constructs a bounded publication without trusting a client-supplied content digest.
  pub fn publication(self, project: ProjectId) -> Result<FactoryProjectPolicyIntent, ApplicationError> {
    match self {
      Self::Launch {
        id,
        expected_current,
        definition,
      } => {
        let revision = expected_current
          .as_ref()
          .map_or(Some(1), |value| value.version.get().checked_add(1))
          .ok_or_else(ApplicationError::invalid)?;
        let intent = FactoryProjectPolicyIntent {
          policy: FactoryProjectPolicyRecord::Launch(
            FactoryLaunchPolicy::new(
              id,
              project,
              FactoryLaunchPolicyVersion::new(revision).map_err(|_| ApplicationError::invalid())?,
              definition,
            )
            .map_err(|_| ApplicationError::invalid())?,
          ),
          expected_current: expected_current.map(FactoryProjectPolicyRef::Launch),
        };
        intent.validate()?;
        Ok(intent)
      }
      Self::Access {
        id,
        expected_current,
        definition,
      } => {
        let revision = expected_current
          .as_ref()
          .map_or(Some(1), |value| value.version.get().checked_add(1))
          .ok_or_else(ApplicationError::invalid)?;
        let intent = FactoryProjectPolicyIntent {
          policy: FactoryProjectPolicyRecord::Access(Box::new(
            FactoryAccessProfile::new(
              id,
              project,
              FactoryAccessProfileVersion::new(revision).map_err(|_| ApplicationError::invalid())?,
              *definition,
            )
            .map_err(|_| ApplicationError::invalid())?,
          )),
          expected_current: expected_current.map(FactoryProjectPolicyRef::Access),
        };
        intent.validate()?;
        Ok(intent)
      }
    }
  }
}
/// Accepted immutable publication; the store owns the atomic current-head precondition.
#[derive(Clone, Debug)]
pub struct PublishFactoryProjectPolicyCommand {
  /// Bounded original contents and exact precondition.
  pub intent: FactoryProjectPolicyIntent,
  /// Existing caller/security-scoped replay key.
  pub idempotency_key: octacity_server_store::IdempotencyKey,
  /// Trusted acceptance time.
  pub published_at: Timestamp,
}
impl Command for PublishFactoryProjectPolicyCommand {
  type Outcome = FactoryProjectPolicyOutcome;
}
/// Side-effect-free review of the same contents and current-head precondition as publication.
#[derive(Clone, Debug)]
pub struct ValidateFactoryProjectPolicyQuery {
  /// Exact original candidate and expected head.
  pub intent: FactoryProjectPolicyIntent,
}
impl Query for ValidateFactoryProjectPolicyQuery {
  type Outcome = FactoryProjectPolicyValidation;
}
impl ManagementAuthorizationTarget for ValidateFactoryProjectPolicyQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = PublishFactoryProjectPolicyCommand::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    management_security::instance_resource(
      ManagementResourceKind::Project,
      self.intent.policy.reference().project_id(),
    )
  }
}
impl ManagementAuthorizationTarget for PublishFactoryProjectPolicyCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = PublishFactoryRuntimeSettingsCommand::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    management_security::instance_resource(
      ManagementResourceKind::Project,
      self.intent.policy.reference().project_id(),
    )
  }
}
/// Dedicated exact content read; current defaults cannot substitute a missing version.
#[derive(Clone, Debug)]
pub struct GetFactoryProjectPolicyQuery {
  /// Exact typed Project-owned reference.
  pub reference: FactoryProjectPolicyRef,
}
impl Query for GetFactoryProjectPolicyQuery {
  type Outcome = FactoryProjectPolicyRecord;
}
impl ManagementAuthorizationTarget for GetFactoryProjectPolicyQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetFactoryRoleQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    management_security::instance_resource(ManagementResourceKind::Project, self.reference.project_id())
  }
}
/// Reviewed installation selection and bounded exact legacy backfill.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryProjectPolicyProvisionDraft {
  /// Exact installed ceiling designated by the deployment.
  pub designated_access: Option<ImmutableReference>,
  /// Original immutable legacy references to import.
  pub legacy_references: Vec<FactoryLegacyPolicyReference>,
}

/// Explicit Project initialization through ordinary administration.
#[derive(Clone, Debug)]
pub struct ProvisionFactoryProjectPoliciesCommand {
  /// Bounded lifecycle intent with trusted acceptance time and scoped replay identity.
  pub request: octacity_server_store::ProvisionFactoryProjectPolicies,
}
impl Command for ProvisionFactoryProjectPoliciesCommand {
  type Outcome = octacity_server_store::ProvisionFactoryProjectPoliciesOutcome;
}
impl ManagementAuthorizationTarget for ProvisionFactoryProjectPoliciesCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = PublishFactoryProjectPolicyCommand::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    management_security::instance_resource(ManagementResourceKind::Project, self.request.intent.project_id)
  }
}
/// One policy owner, independent of role profiles, Runtime installation and execution.
pub struct FactoryProjectPolicyHandlers<S> {
  store: std::sync::Arc<S>,
  policy: std::sync::Arc<dyn ManagementAuthorizationPolicy>,
}
impl<S> FactoryProjectPolicyHandlers<S> {
  /// Uses the existing backend-neutral policy journal.
  pub fn new(store: std::sync::Arc<S>, policy: std::sync::Arc<dyn ManagementAuthorizationPolicy>) -> Self {
    Self { store, policy }
  }
}
#[async_trait::async_trait]
impl<S: FactoryProjectPolicyStore> ManagementCommandUseCase<ProvisionFactoryProjectPoliciesCommand>
  for FactoryProjectPolicyHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: ProvisionFactoryProjectPoliciesCommand,
  ) -> Result<<ProvisionFactoryProjectPoliciesCommand as Command>::Outcome, ApplicationError> {
    crate::factory_role_profiles::require_project_visibility(grant, command.request.intent.project_id)?;
    Ok(
      self
        .store
        .provision_factory_project_policies(management_security::audited_mutation(context, command.request)?)
        .await?,
    )
  }
}

pub use octacity_server_store::MAX_FACTORY_LEGACY_POLICY_REFERENCES;

#[async_trait::async_trait]
impl<S: FactoryProjectPolicyStore + FactoryConfigurationStore>
  ManagementCommandUseCase<PublishFactoryProjectPolicyCommand> for FactoryProjectPolicyHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: PublishFactoryProjectPolicyCommand,
  ) -> Result<FactoryProjectPolicyOutcome, ApplicationError> {
    crate::factory_role_profiles::require_project_visibility(grant, command.intent.policy.reference().project_id())?;
    if let Some(prior) = self
      .store
      .replay_factory_project_policy(management_security::audited_mutation(
        context,
        octacity_server_store::ReplayFactoryProjectPolicy {
          intent: command.intent.clone(),
          idempotency_key: command.idempotency_key.clone(),
        },
      )?)
      .await?
    {
      return Ok(prior);
    }
    self.qualify(context, &command.intent).await?;
    self
      .store
      .publish_factory_project_policy(management_security::audited_mutation(
        context,
        PublishFactoryProjectPolicy {
          intent: command.intent,
          idempotency_key: command.idempotency_key,
          published_at: command.published_at,
        },
      )?)
      .await
      .map_err(Into::into)
  }
}
#[async_trait::async_trait]
impl<S: FactoryProjectPolicyStore + FactoryConfigurationStore> ManagementQueryUseCase<ValidateFactoryProjectPolicyQuery>
  for FactoryProjectPolicyHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ValidateFactoryProjectPolicyQuery,
  ) -> Result<FactoryProjectPolicyValidation, ApplicationError> {
    crate::factory_role_profiles::require_project_visibility(grant, query.intent.policy.reference().project_id())?;
    self.store.validate_factory_project_policy(&query.intent).await?;
    let installed_ceiling = self.qualify(context, &query.intent).await?;
    Ok(FactoryProjectPolicyValidation {
      policy: query.intent.policy,
      installed_ceiling,
    })
  }
}
#[async_trait::async_trait]
impl<S: FactoryProjectPolicyStore> ManagementQueryUseCase<GetFactoryProjectPolicyQuery>
  for FactoryProjectPolicyHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetFactoryProjectPolicyQuery,
  ) -> Result<FactoryProjectPolicyRecord, ApplicationError> {
    let project = query.reference.project_id();
    crate::factory_role_profiles::require_project_visibility(grant, project)?;
    self
      .store
      .factory_project_policy(project, &query.reference)
      .await?
      .ok_or_else(|| {
        octacity_server_store::StoreError::NotFound {
          entity: octacity_server_domain::EntityKind::Project,
        }
        .into()
      })
  }
}

/// Bounded current metadata; permission contents require a dedicated exact read.
#[derive(Clone, Debug)]
pub struct ListFactoryProjectPoliciesQuery {
  /// Stable typed kind/identity continuation within the owning Project.
  pub page: CurrentDefinitionPageInput<FactoryProjectPolicyRef>,
}
impl Query for ListFactoryProjectPoliciesQuery {
  type Outcome = octacity_server_store::FactoryProjectPolicyPage;
}
impl ManagementAuthorizationTarget for ListFactoryProjectPoliciesQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetFactoryProjectPolicyQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    management_security::instance_resource(ManagementResourceKind::Project, self.page.project_id())
  }
}
/// Read exact Project defaults without initialization or latest-version substitution.
#[derive(Clone, Debug)]
pub struct GetFactoryProjectPolicyDefaultsQuery {
  /// Existing visible owner.
  pub project_id: ProjectId,
}
impl Query for GetFactoryProjectPolicyDefaultsQuery {
  type Outcome = Option<octacity_server_store::FactoryProjectPolicyDefaults>;
}
impl ManagementAuthorizationTarget for GetFactoryProjectPolicyDefaultsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetFactoryProjectPolicyQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    management_security::instance_resource(ManagementResourceKind::Project, self.project_id)
  }
}
/// Reviewed selection; deliberate null pins remain unresolved rather than selecting by catalog order.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryProjectPolicyDefaultsDraft {
  /// Exact displayed default-selection revision, absent only before the first selection.
  pub expected_version: Option<u64>,
  /// Exact launch pin or deliberate absence.
  pub launch_policy: Option<FactoryLaunchPolicyRef>,
  /// Exact access pin or deliberate absence.
  pub access_profile: Option<FactoryAccessProfileRef>,
}
/// Explicit default replacement under the existing scoped mutation contract.
#[derive(Clone, Debug)]
pub struct SelectFactoryProjectPolicyDefaultsCommand {
  /// Exact owned pins and current-selection precondition.
  pub intent: octacity_server_store::FactoryProjectPolicyDefaultsIntent,
  /// Caller/security-scoped replay key.
  pub idempotency_key: octacity_server_store::IdempotencyKey,
  /// Trusted acceptance time.
  pub published_at: Timestamp,
}
impl Command for SelectFactoryProjectPolicyDefaultsCommand {
  type Outcome = octacity_server_store::FactoryProjectPolicyDefaultsOutcome;
}
impl ManagementAuthorizationTarget for SelectFactoryProjectPolicyDefaultsCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = PublishFactoryProjectPolicyCommand::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    management_security::instance_resource(ManagementResourceKind::Project, self.intent.project_id)
  }
}
#[async_trait::async_trait]
impl<S: FactoryProjectPolicyStore> ManagementQueryUseCase<ListFactoryProjectPoliciesQuery>
  for FactoryProjectPolicyHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListFactoryProjectPoliciesQuery,
  ) -> Result<<ListFactoryProjectPoliciesQuery as Query>::Outcome, ApplicationError> {
    crate::factory_role_profiles::require_project_visibility(grant, query.page.project_id())?;
    Ok(
      self
        .store
        .list_factory_project_policies(query.page.project_id(), query.page.after(), query.page.limit())
        .await?,
    )
  }
}
#[async_trait::async_trait]
impl<S: FactoryProjectPolicyStore> ManagementQueryUseCase<GetFactoryProjectPolicyDefaultsQuery>
  for FactoryProjectPolicyHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetFactoryProjectPolicyDefaultsQuery,
  ) -> Result<<GetFactoryProjectPolicyDefaultsQuery as Query>::Outcome, ApplicationError> {
    crate::factory_role_profiles::require_project_visibility(grant, query.project_id)?;
    Ok(self.store.factory_project_policy_defaults(query.project_id).await?)
  }
}
#[async_trait::async_trait]
impl<S: FactoryProjectPolicyStore> ManagementCommandUseCase<SelectFactoryProjectPolicyDefaultsCommand>
  for FactoryProjectPolicyHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: SelectFactoryProjectPolicyDefaultsCommand,
  ) -> Result<<SelectFactoryProjectPolicyDefaultsCommand as Command>::Outcome, ApplicationError> {
    crate::factory_role_profiles::require_project_visibility(grant, command.intent.project_id)?;
    Ok(
      self
        .store
        .select_factory_project_policy_defaults(management_security::audited_mutation(
          context,
          octacity_server_store::SelectFactoryProjectPolicyDefaults {
            intent: command.intent,
            idempotency_key: command.idempotency_key,
            published_at: command.published_at,
          },
        )?)
        .await?,
    )
  }
}

impl<S: FactoryConfigurationStore> FactoryProjectPolicyHandlers<S> {
  // A Project profile must fit one complete verified installed ceiling; inventories are never unioned.
  async fn qualify(
    &self,
    context: &ManagementRequestContext,
    intent: &FactoryProjectPolicyIntent,
  ) -> Result<Option<FactoryPolicyInstalledCeiling>, ApplicationError> {
    let FactoryProjectPolicyRecord::Access(profile) = &intent.policy else {
      return Ok(None);
    };
    if profile.definition().permissions == FactoryPermissionSet::deny_all() {
      return Ok(None);
    }
    let choices = match crate::factory_configuration_choices::visible_installed_factory_choices(
      self.store.as_ref(),
      self.policy.as_ref(),
      context,
      profile.reference().project_id,
    )
    .await
    {
      Ok(choices) => choices,
      Err(ApplicationError::CapabilityUnavailable) => return Err(access_limit()),
      Err(error) => return Err(error),
    };
    qualifying_access_ceiling(profile, &choices)
      .map(Some)
      .ok_or_else(access_limit)
  }
}

// Configuration readiness uses the same qualification as profile publication.
pub(super) fn access_fits_choices(profile: &FactoryAccessProfile, choices: &FactoryConfigurationChoices) -> bool {
  profile.definition().permissions == FactoryPermissionSet::deny_all()
    || qualifying_access_ceiling(profile, choices).is_some()
}

fn qualifying_access_ceiling(
  profile: &FactoryAccessProfile,
  choices: &FactoryConfigurationChoices,
) -> Option<FactoryPolicyInstalledCeiling> {
  let entries = choices.entries();
  if !crate::factory_configuration_choices::permissions_selectable(
    &profile.definition().permissions,
    &entries.references,
  ) {
    return None;
  }
  entries
    .references
    .iter()
    .filter(|entry| entry.kind == FactoryChoiceKind::PermissionCeiling)
    .find_map(|entry| {
      let permissions = octacity_server_store::factory_policy_access_declaration(choices, &entry.reference).ok()?;
      profile.definition().validate_within(&permissions).ok()?;
      // Apply the catalog's visibility rule to the complete declaration.
      let permissions = crate::factory_configuration_choices::permissions_selectable(&permissions, &entries.references)
        .then_some(permissions);
      Some(FactoryPolicyInstalledCeiling {
        alias: entry.alias.clone(),
        reference: entry.reference.clone(),
        permissions,
      })
    })
}

fn access_limit() -> ApplicationError {
  ApplicationError::FactoryValidation(Box::new(FactoryValidationDiagnostics::single_issue(
    "/definition/permissions",
    "permission_exceeds_ceiling",
    Some("installation"),
  )))
}

/// Exact legacy-reference resolution, distinct from Project-owned pins and live defaults.
#[derive(Clone, Debug)]
pub struct ResolveFactoryLegacyPolicyQuery {
  /// Authorized owning Project.
  pub project_id: ProjectId,
  /// Complete typed original installation reference.
  pub legacy: octacity_server_store::FactoryLegacyPolicyReference,
}
impl Query for ResolveFactoryLegacyPolicyQuery {
  type Outcome = Option<octacity_server_store::FactoryLegacyPolicyMapping>;
}
impl ManagementAuthorizationTarget for ResolveFactoryLegacyPolicyQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetFactoryProjectPolicyQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    management_security::instance_resource(ManagementResourceKind::Project, self.project_id)
  }
}
#[async_trait::async_trait]
impl<S: FactoryProjectPolicyStore> ManagementQueryUseCase<ResolveFactoryLegacyPolicyQuery>
  for FactoryProjectPolicyHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ResolveFactoryLegacyPolicyQuery,
  ) -> Result<<ResolveFactoryLegacyPolicyQuery as Query>::Outcome, ApplicationError> {
    crate::factory_role_profiles::require_project_visibility(grant, query.project_id)?;
    Ok(
      self
        .store
        .factory_legacy_policy_mapping(query.project_id, &query.legacy)
        .await?,
    )
  }
}
pub use octacity_server_store::FactoryLegacyPolicyReference;
