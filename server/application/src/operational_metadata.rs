use crate::{ManagementAction, ManagementAuthorizationTarget, ManagementResource, ManagementResourceKind, Query};

/// Reads safe deployment metadata for the running management control plane.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetOperationalMetadataQuery;

impl Query for GetOperationalMetadataQuery {
  type Outcome = ManagementOperationalMetadataProjection;
}

impl ManagementAuthorizationTarget for GetOperationalMetadataQuery {
  fn management_action(&self) -> ManagementAction {
    ManagementAction::View
  }

  fn management_resource(&self) -> ManagementResource {
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
