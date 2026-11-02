//! Project policy management shares accepted authorization and the durable policy journal.
use crate::*;
use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::*;
use octacity_server_store::{FactoryConfigurationStore, FactoryProjectPolicyStore, PublishFactoryProjectPolicy};
pub use octacity_server_store::{
  FactoryProjectPolicyDefaults, FactoryProjectPolicyIntent, FactoryProjectPolicyOutcome, FactoryProjectPolicyRecord,
  FactoryProjectPolicyRef,
};

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
  type Outcome = FactoryProjectPolicyRecord;
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
  ) -> Result<FactoryProjectPolicyRecord, ApplicationError> {
    crate::factory_role_profiles::require_project_visibility(grant, query.intent.policy.reference().project_id())?;
    self.store.validate_factory_project_policy(&query.intent).await?;
    self.qualify(context, &query.intent).await?;
    Ok(query.intent.policy)
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
  ) -> Result<(), ApplicationError> {
    let FactoryProjectPolicyRecord::Access(profile) = &intent.policy else {
      return Ok(());
    };
    if profile.definition().permissions == FactoryPermissionSet::deny_all() {
      return Ok(());
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
    if access_fits_choices(profile, &choices) {
      Ok(())
    } else {
      Err(access_limit())
    }
  }
}
// Reuse one qualification rule for profile publication and Configuration/readiness checks.
pub(super) fn access_fits_choices(profile: &FactoryAccessProfile, choices: &FactoryConfigurationChoices) -> bool {
  if profile.definition().permissions == FactoryPermissionSet::deny_all() {
    return true;
  }
  let entries = choices.entries();
  crate::factory_configuration_choices::permissions_selectable(&profile.definition().permissions, &entries.references)
    && entries
      .references
      .iter()
      .filter(|entry| entry.kind == FactoryChoiceKind::PermissionCeiling)
      .any(|entry| {
        octacity_server_store::factory_policy_access_declaration(choices, &entry.reference)
          .is_ok_and(|ceiling| profile.definition().validate_within(&ceiling).is_ok())
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
