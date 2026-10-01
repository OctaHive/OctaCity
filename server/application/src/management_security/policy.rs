use async_trait::async_trait;

use super::{
  ManagementAction, ManagementAuthorizationDenial, ManagementAuthorizationGrant, ManagementRequestContext,
  ManagementResource, ManagementVisibility,
};

/// Application-layer seam for authorizing one typed management capability.
///
/// Transport and persistence adapters must not implement this decision. Every
/// management command and query is expected to cross an authorized application
/// handler before its inner use case runs.
#[async_trait]
pub trait ManagementAuthorizationPolicy: Send + Sync {
  /// Returns a visibility-bearing grant or the opaque stable denial.
  async fn authorize(
    &self,
    context: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial>;
}

/// Initial policy for the explicitly acknowledged trusted-network deployment.
///
/// This policy accepts only the canonical anonymous trusted-network context and
/// grants unrestricted visibility. It is not login, identity verification,
/// RBAC, or ABAC; deployment network controls remain its trust boundary.
#[derive(Clone, Copy, Debug, Default)]
pub struct TrustedNetworkManagementPolicy;

#[async_trait]
impl ManagementAuthorizationPolicy for TrustedNetworkManagementPolicy {
  async fn authorize(
    &self,
    context: &ManagementRequestContext,
    _action: ManagementAction,
    _resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if !context.is_canonical_trusted_network() {
      return Err(ManagementAuthorizationDenial::forbidden());
    }
    Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
  }
}
