use async_trait::async_trait;

use crate::{
  ApplicationError, ManagementAction, ManagementAuthorizationGrant, ManagementAuthorizationMapping,
  ManagementAuthorizationTarget, ManagementQueryUseCase, ManagementRequestContext, ManagementResource,
  ManagementResourceKind, ManagementResourceResult, Query,
};

/// Reads safe deployment metadata for the running management control plane.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetOperationalMetadataQuery;

impl Query for GetOperationalMetadataQuery {
  type Outcome = ManagementOperationalMetadataProjection;
}

impl ManagementAuthorizationTarget for GetOperationalMetadataQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::collection(ManagementAction::View, ManagementResourceKind::ControlPlane);

  fn management_resource(&self) -> ManagementResourceResult {
    ManagementResource::collection(ManagementResourceKind::ControlPlane)
  }
}

/// Safe composition facts exposed by the management metadata query.
///
/// The REST adapter enriches these facts with stable wire vocabulary. Bound
/// addresses, credentials, and provider configuration are intentionally absent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ManagementOperationalMetadataProjection {
  /// Whether the management listener is reachable beyond loopback.
  pub management_externally_reachable: bool,
  /// Whether the operator acknowledged unauthenticated external reachability.
  pub external_access_acknowledged: bool,
  /// Whether independently authenticated Agent ingress is enabled.
  pub agent_ingress_enabled: bool,
  /// Whether independently verified webhook ingress is enabled.
  pub webhook_ingress_enabled: bool,
}

/// Immutable application query service for safe deployment metadata.
pub struct OperationalMetadataQueries {
  metadata: ManagementOperationalMetadataProjection,
}

impl OperationalMetadataQueries {
  /// Creates a query service from composition-owned deployment facts.
  #[must_use]
  pub const fn new(metadata: ManagementOperationalMetadataProjection) -> Self {
    Self { metadata }
  }
}

#[async_trait]
impl ManagementQueryUseCase<GetOperationalMetadataQuery> for OperationalMetadataQueries {
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    _query: GetOperationalMetadataQuery,
  ) -> Result<ManagementOperationalMetadataProjection, Self::Error> {
    Ok(self.metadata)
  }
}
