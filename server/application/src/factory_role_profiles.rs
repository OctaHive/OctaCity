//! Project AI-agent definitions over audited management and operation-shaped store ports.
//! Publication retains definitions only; installed compatibility is qualified separately.
use crate::*;
use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::{
  FactoryRole, FactoryRoleId, FactoryRoleProfile, FactoryRoleProfileId, FactoryRoleProfileVersion, FactoryRoleVersion,
};
pub use octacity_server_store::{
  FactoryRoleIntent, FactoryRoleProfileIntent, FactoryRoleProfilesIntent, FactoryRoleProfilesOutcome,
};
use octacity_server_store::{FactoryRoleProfileStore, IdempotencyKey, PublishFactoryRoleProfiles};
use std::sync::Arc;

/// Atomic reviewed role/profile publication, also used for template copies.
#[derive(Clone, Debug)]
pub struct PublishFactoryRoleProfilesCommand {
  /// Immutable definitions and current-version preconditions.
  pub intent: FactoryRoleProfilesIntent,
  /// Stable scoped mutation identity.
  pub idempotency_key: IdempotencyKey,
  /// Trusted first acceptance time.
  pub published_at: Timestamp,
}
impl Command for PublishFactoryRoleProfilesCommand {
  type Outcome = FactoryRoleProfilesOutcome;
}
impl ManagementAuthorizationTarget for PublishFactoryRoleProfilesCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Administer, ManagementResourceKind::Project);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Project, self.intent.project_id)
  }
}
/// Authorized exact role content read; no provider or worker connection is needed.
#[derive(Clone, Debug)]
pub struct GetFactoryRoleQuery {
  /// Owning visible Project.
  pub project_id: ProjectId,
  /// Stable role identity.
  pub id: FactoryRoleId,
  /// Exact immutable version; None selects current.
  pub version: Option<FactoryRoleVersion>,
}
impl Query for GetFactoryRoleQuery {
  type Outcome = FactoryRole;
}
impl ManagementAuthorizationTarget for GetFactoryRoleQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::Project);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Project, self.project_id)
  }
}
/// Authorized exact profile content read, including its original role pin.
#[derive(Clone, Debug)]
pub struct GetFactoryRoleProfileQuery {
  /// Owning visible Project.
  pub project_id: ProjectId,
  /// Stable profile identity.
  pub id: FactoryRoleProfileId,
  /// Exact immutable version; None selects current.
  pub version: Option<FactoryRoleProfileVersion>,
}
impl Query for GetFactoryRoleProfileQuery {
  type Outcome = FactoryRoleProfile;
}
impl ManagementAuthorizationTarget for GetFactoryRoleProfileQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetFactoryRoleQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Project, self.project_id)
  }
}
/// Uses the existing Project authorization/audit boundary without another agent registry.
pub struct FactoryRoleProfileHandlers<S> {
  pub(super) store: Arc<S>,
  pub(super) policy: Arc<dyn ManagementAuthorizationPolicy>,
}
impl<S> FactoryRoleProfileHandlers<S> {
  /// Shares the accepted policy with dependency discovery and validation.
  pub fn with_policy(store: Arc<S>, policy: Arc<dyn ManagementAuthorizationPolicy>) -> Self {
    Self { store, policy }
  }
  /// Composes trusted-network handlers; scoped deployments use `with_policy`
  /// so dependency discovery applies the same accepted management policy.
  pub fn new(store: Arc<S>) -> Self {
    Self::with_policy(store, Arc::new(TrustedNetworkManagementPolicy))
  }
}
#[async_trait::async_trait]
impl<S: FactoryRoleProfileStore> ManagementCommandUseCase<PublishFactoryRoleProfilesCommand>
  for FactoryRoleProfileHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: PublishFactoryRoleProfilesCommand,
  ) -> Result<FactoryRoleProfilesOutcome, ApplicationError> {
    require_project_visibility(grant, command.intent.project_id)?;
    if let Some(role) = &command.intent.role {
      validate_avatar(&role.definition)?;
    }
    Ok(
      self
        .store
        .publish_factory_role_profiles(crate::management_security::audited_mutation(
          context,
          PublishFactoryRoleProfiles {
            intent: command.intent,
            idempotency_key: command.idempotency_key,
            published_at: command.published_at,
          },
        )?)
        .await?,
    )
  }
}
#[async_trait::async_trait]
impl<S: FactoryRoleProfileStore> ManagementQueryUseCase<GetFactoryRoleQuery> for FactoryRoleProfileHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetFactoryRoleQuery,
  ) -> Result<FactoryRole, ApplicationError> {
    require_project_visibility(grant, query.project_id)?;
    Ok(
      self
        .store
        .factory_role(query.project_id, query.id, query.version)
        .await?,
    )
  }
}
#[async_trait::async_trait]
impl<S: FactoryRoleProfileStore> ManagementQueryUseCase<GetFactoryRoleProfileQuery> for FactoryRoleProfileHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetFactoryRoleProfileQuery,
  ) -> Result<FactoryRoleProfile, ApplicationError> {
    require_project_visibility(grant, query.project_id)?;
    Ok(
      self
        .store
        .factory_role_profile(query.project_id, query.id, query.version)
        .await?,
    )
  }
}

pub(super) fn require_project_visibility(
  grant: &ManagementAuthorizationGrant,
  project: ProjectId,
) -> Result<(), ApplicationError> {
  let visibility = grant
    .visibility_for::<ListProjectsQuery>()
    .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
  if !visibility.allows(&project) {
    return Err(
      octacity_server_store::StoreError::NotFound {
        entity: octacity_server_domain::EntityKind::Project,
      }
      .into(),
    );
  }
  Ok(())
}

/// Side-effect-free normalization of a complete custom role draft.
#[derive(Clone, Debug)]
pub struct ValidateFactoryRoleQuery {
  /// Visible owning Project.
  pub project_id: ProjectId,
  /// Caller-owned stable role identity.
  pub id: FactoryRoleId,
  /// Reviewed metadata and instruction entries.
  pub definition: octacity_server_factory::FactoryRoleDefinition,
}
impl Query for ValidateFactoryRoleQuery {
  type Outcome = FactoryRole;
}
impl ManagementAuthorizationTarget for ValidateFactoryRoleQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetFactoryRoleQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Project, self.project_id)
  }
}
#[async_trait::async_trait]
impl<S: FactoryRoleProfileStore> ManagementQueryUseCase<ValidateFactoryRoleQuery> for FactoryRoleProfileHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ValidateFactoryRoleQuery,
  ) -> Result<FactoryRole, ApplicationError> {
    require_project_visibility(grant, query.project_id)?;
    validate_avatar(&query.definition)?;
    FactoryRole::publish(query.id, query.project_id, query.definition).map_err(|_| ApplicationError::invalid())
  }
}

fn validate_avatar(definition: &octacity_server_factory::FactoryRoleDefinition) -> Result<(), ApplicationError> {
  if let Some(avatar) = &definition.avatar {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
      .decode(avatar.encoded_png())
      .map_err(|_| ApplicationError::invalid())?;
    crate::raster_preview::png("image/png", bytes).map_err(|_| ApplicationError::invalid())?;
  }
  Ok(())
}
/// Explicit independent copy, published by the ordinary atomic Role/Profile operation.
#[derive(Clone, Debug)]
pub struct CopyFactoryRoleTemplateCommand {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Exact release template, never a mutable alias.
  pub template: octacity_server_factory::ImmutableReference,
  /// Caller-owned new role identity.
  pub role_id: FactoryRoleId,
  /// Complete reviewed shared content.
  pub role: octacity_server_factory::FactoryRoleDefinition,
  /// Explicit complete profile settings for each template seed.
  pub profiles: std::collections::BTreeMap<
    octacity_server_factory::FactoryKey,
    octacity_server_factory::FactoryRoleTemplateBinding,
  >,
  /// Scoped stable transport identity.
  pub idempotency_key: IdempotencyKey,
  /// Trusted publication time.
  pub published_at: Timestamp,
}
impl Command for CopyFactoryRoleTemplateCommand {
  type Outcome = FactoryRoleProfilesOutcome;
}
impl ManagementAuthorizationTarget for CopyFactoryRoleTemplateCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = PublishFactoryRoleProfilesCommand::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Project, self.project_id)
  }
}
#[async_trait::async_trait]
impl<S: FactoryRoleProfileStore> ManagementCommandUseCase<CopyFactoryRoleTemplateCommand>
  for FactoryRoleProfileHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: CopyFactoryRoleTemplateCommand,
  ) -> Result<FactoryRoleProfilesOutcome, ApplicationError> {
    require_project_visibility(grant, command.project_id)?;
    let template = octacity_server_factory::factory_role_templates()
      .map_err(|_| ApplicationError::invalid())?
      .into_iter()
      .find(|t| t.reference().is_ok_and(|r| r == command.template))
      .ok_or_else(ApplicationError::capability_unavailable)?;
    let instance = template
      .instantiate(command.project_id, command.role_id, command.role, command.profiles)
      .map_err(|_| ApplicationError::invalid())?;
    self
      .execute_management_command(
        context,
        grant,
        PublishFactoryRoleProfilesCommand {
          intent: instance.into(),
          idempotency_key: command.idempotency_key,
          published_at: command.published_at,
        },
      )
      .await
  }
}
