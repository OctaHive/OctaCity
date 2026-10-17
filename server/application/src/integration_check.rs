//! Read-only connection diagnostics; no Project grant, Factory or task-state mutation port is held.
use crate::management_security::instance_resource;
use crate::*;
use async_trait::async_trait;
use octacity_server_domain::{IntegrationId, IntegrationVersion};
use octacity_server_factory::{ExternalWorkIdentity, ImmutableReference};
use octacity_server_store::{
  IntegrationAdministrationStore, IntegrationConnectionStore, IntegrationResource, PublishedIntegrationConnection,
};
use serde::Serialize;
use std::sync::Arc;

/// Safe independently observed readiness; a check result never grants authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationCheckState {
  /// Proven by this bounded read.
  Ready,
  /// Required installed material is absent or unusable.
  Missing,
  /// Configuration/frame violates its installed ceiling.
  Invalid,
  /// Authentication or native access was refused.
  Denied,
  /// Original native identity/configuration no longer matches.
  Conflict,
  /// No compatible installed check exists for this operation.
  Unsupported,
  /// Read-only evidence has not proven this aspect.
  Unverified,
  /// Provider requires a bounded later retry.
  Throttled,
  /// Provider or process could not complete this check.
  Unavailable,
}
/// Independent diagnostic aspects, without raw errors, private handles or provider payloads.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct IntegrationCheckReadiness {
  /// Verified installed configuration.
  pub configuration: IntegrationCheckState,
  /// Current purpose credential authentication.
  pub authentication: IntegrationCheckState,
  /// Original native resource access and identity.
  pub resource_access: IntegrationCheckState,
  /// Requested operation readiness, independently of token breadth or Project grants.
  pub capability: IntegrationCheckState,
  /// Bounded provider hint; this operation never retries automatically.
  pub retry_after_millis: Option<u32>,
}
impl IntegrationCheckReadiness {
  /// Starts with no aspect claimed ready.
  pub const fn unverified() -> Self {
    Self {
      configuration: IntegrationCheckState::Unverified,
      authentication: IntegrationCheckState::Unverified,
      resource_access: IntegrationCheckState::Unverified,
      capability: IntegrationCheckState::Unverified,
      retry_after_millis: None,
    }
  }
}
/// Trusted installed checker, implemented through verified bounded provider processes.
#[async_trait]
pub trait IntegrationConnectionChecker: Send + Sync {
  /// Checks only this published resource and purpose, without granting or activating anything.
  async fn check(
    &self,
    connection: &PublishedIntegrationConnection,
    resource: &IntegrationResource,
    purpose: IntegrationPurpose,
  ) -> Result<IntegrationCheckReadiness, ApplicationError>;
}
/// Safe native resource/capability choice with this check's separate readiness facts.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct IntegrationConnectionCheckResult {
  /// Original stable connection.
  pub connection_id: IntegrationId,
  /// Exact reviewed metadata version, never an implicit latest replacement.
  pub connection_version: IntegrationVersion,
  /// Original installed resource definition.
  pub resource: ImmutableReference,
  /// Opaque native container, independent of an OctaCity Project/code Repository.
  pub native_id: ExternalWorkIdentity,
  /// Safe display context from the authorized connection, never a provider-selected target.
  pub locator: ExternalWorkIdentity,
  /// Independently checked purpose.
  pub purpose: IntegrationPurpose,
  /// Observation, not authority or input activation.
  pub readiness: IntegrationCheckReadiness,
}
/// Separately authorized administrative check; it needs no Factory target or internal Issue.
#[derive(Clone, Debug)]
pub struct CheckIntegrationConnectionQuery {
  /// Connection to inspect under current management rights.
  pub id: IntegrationId,
  /// Reviewed current metadata precondition.
  pub version: IntegrationVersion,
  /// Explicit selected native resource definition.
  pub resource: ImmutableReference,
  /// Independent purpose to check.
  pub purpose: IntegrationPurpose,
}
impl Query for CheckIntegrationConnectionQuery {
  type Outcome = IntegrationConnectionCheckResult;
}
impl ManagementAuthorizationTarget for CheckIntegrationConnectionQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::instance(
    ManagementAction::Administer,
    ManagementResourceKind::IntegrationConnection,
  );
  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::IntegrationConnection, self.id)
  }
}
/// A check reads safe current metadata/state and uses a bounded installed checker, never mutation ports.
pub struct IntegrationCheckHandlers<S> {
  store: Arc<S>,
  checker: Arc<dyn IntegrationConnectionChecker>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
}
impl<S> IntegrationCheckHandlers<S> {
  /// Requires the same policy as the outer authorized dispatcher for dependent visibility/use checks.
  pub fn new(
    store: Arc<S>,
    checker: Arc<dyn IntegrationConnectionChecker>,
    policy: Arc<dyn ManagementAuthorizationPolicy>,
  ) -> Self {
    Self { store, checker, policy }
  }
}
#[async_trait]
impl<S: IntegrationConnectionStore + IntegrationAdministrationStore>
  ManagementQueryUseCase<CheckIntegrationConnectionQuery> for IntegrationCheckHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    _: &ManagementAuthorizationGrant,
    query: CheckIntegrationConnectionQuery,
  ) -> Result<IntegrationConnectionCheckResult, ApplicationError> {
    let target = query.management_resource().map_err(|_| ApplicationError::invalid())?;
    for action in [
      ManagementAction::Administer,
      ManagementAction::View,
      ManagementAction::Execute,
    ] {
      let grant = self
        .policy
        .authorize(context, action, &target)
        .await
        .map_err(|_| ApplicationError::invalid())?;
      match grant.visibility().view() {
        ManagementVisibilityView::All => {}
        ManagementVisibilityView::Restricted(resources) if resources.contains(&target) => {}
        _ => return Err(ApplicationError::invalid()),
      }
    }
    let connection = self.store.integration_connection(query.id, None).await?;
    if connection.version != query.version {
      return Err(ApplicationError::PreconditionFailed);
    }
    let resource = connection
      .definition
      .resources
      .iter()
      .find(|r| r.reference == query.resource)
      .ok_or_else(ApplicationError::invalid)?;
    let readiness = if let Some(capability) = resource.capabilities.iter().find(|c| c.purpose == query.purpose) {
      let state = self.store.integration_administration(query.id).await?;
      if state
        .credentials
        .iter()
        .any(|c| c.reference == capability.credential && c.configured)
      {
        self.checker.check(&connection, resource, query.purpose).await?
      } else {
        let mut readiness = IntegrationCheckReadiness::unverified();
        readiness.authentication = IntegrationCheckState::Missing;
        readiness
      }
    } else {
      let mut readiness = IntegrationCheckReadiness::unverified();
      readiness.capability = IntegrationCheckState::Unsupported;
      readiness
    };
    Ok(IntegrationConnectionCheckResult {
      connection_id: query.id,
      connection_version: query.version,
      resource: resource.reference.clone(),
      native_id: resource.native_id.clone(),
      locator: resource.locator.clone(),
      purpose: query.purpose,
      readiness,
    })
  }
}
