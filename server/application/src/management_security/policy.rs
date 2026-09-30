use async_trait::async_trait;

use super::{
  ManagementAction, ManagementAuthorizationDenial, ManagementAuthorizationGrant, ManagementRequestContext,
  ManagementResource, ManagementVisibility,
};

/// Application-layer seam for authorizing one typed management capability.
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
