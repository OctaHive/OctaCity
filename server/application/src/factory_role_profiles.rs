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
