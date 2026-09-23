use serde::{Deserialize, Serialize};

/// Deployment security and feature status exposed to management clients.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OperationalMetadata {
  /// Stable management API version described by this document.
  pub api_version: String,
  /// Security modes enforced at each independently configured ingress.
  pub security: DeploymentSecurity,
  /// Whether each independently configured ingress is enabled.
  pub ingress: IngressMetadata,
  /// Stable feature availability reported by this server build and composition.
  pub capabilities: Vec<CapabilityMetadata>,
}

impl OperationalMetadata {
  /// Creates metadata for the first trusted-network server composition.
  #[must_use]
  pub fn trusted_network(
    management_externally_reachable: bool,
    external_access_acknowledged: bool,
    agent_ingress_enabled: bool,
    webhook_ingress_enabled: bool,
  ) -> Self {
    Self {
      api_version: "v1".to_owned(),
      security: DeploymentSecurity {
        management: ManagementSecurity {
          mode: ManagementSecurityMode::TrustedNetworkUnauthenticated,
          operator_authentication: false,
          externally_reachable: management_externally_reachable,
          external_access_acknowledged,
        },
        agent: AuthenticatedIngressSecurity {
          mode: AuthenticatedIngressMode::RegistrationCredentials,
          authentication_required: true,
        },
        webhook: AuthenticatedIngressSecurity {
          mode: AuthenticatedIngressMode::ProviderVerification,
          authentication_required: true,
        },
      },
      ingress: IngressMetadata {
        management_enabled: true,
        agent_enabled: agent_ingress_enabled,
        webhook_enabled: webhook_ingress_enabled,
        listeners_separate: true,
      },
      capabilities: vec![
        CapabilityMetadata::available("operational_metadata"),
        CapabilityMetadata::unavailable("agent_coordination"),
        if webhook_ingress_enabled {
          CapabilityMetadata::available("webhook_ingestion")
        } else {
          CapabilityMetadata::unavailable("webhook_ingestion")
        },
        CapabilityMetadata::unavailable("dynamic_agent_provisioning"),
      ],
    }
  }
}

/// Security metadata for management, agent, and webhook ingress.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeploymentSecurity {
  /// Unauthenticated trusted-network management policy.
  pub management: ManagementSecurity,
  /// Agent credential policy, independent from management authentication.
  pub agent: AuthenticatedIngressSecurity,
  /// Webhook verification policy, independent from management authentication.
  pub webhook: AuthenticatedIngressSecurity,
}

/// Management listener security metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManagementSecurity {
  /// Stable management deployment mode.
  pub mode: ManagementSecurityMode,
  /// Whether operator identity is authenticated in this release.
  pub operator_authentication: bool,
  /// Whether the configured bind is reachable beyond loopback.
  pub externally_reachable: bool,
  /// Whether the operator explicitly acknowledged external unauthenticated access.
  pub external_access_acknowledged: bool,
}

/// Stable management security mode.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagementSecurityMode {
  /// Management requests are unauthenticated and require trusted-network isolation.
  TrustedNetworkUnauthenticated,
}

/// Security metadata for one authenticated non-management ingress.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuthenticatedIngressSecurity {
  /// Authentication mode enforced by the ingress contract.
  pub mode: AuthenticatedIngressMode,
  /// Whether the ingress accepts unauthenticated protocol operations.
  pub authentication_required: bool,
}

/// Stable authentication modes for non-management ingress.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthenticatedIngressMode {
  /// Agent registration and lease requests require agent credentials.
  RegistrationCredentials,
  /// Webhook deliveries require provider-specific verification.
  ProviderVerification,
}

/// Independent listener enablement without disclosing bound addresses.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IngressMetadata {
  /// Whether management ingress is enabled.
  pub management_enabled: bool,
  /// Whether agent ingress is configured.
  pub agent_enabled: bool,
  /// Whether webhook ingress is configured.
  pub webhook_enabled: bool,
  /// Whether management, agent, and webhook use distinct listener ownership.
  pub listeners_separate: bool,
}

/// Availability of one stable server capability.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityMetadata {
  /// Stable machine-readable capability name.
  pub name: String,
  /// Availability in the running composition.
  pub status: CapabilityStatus,
}

impl CapabilityMetadata {
  fn available(name: &str) -> Self {
    Self {
      name: name.to_owned(),
      status: CapabilityStatus::Available,
    }
  }

  fn unavailable(name: &str) -> Self {
    Self {
      name: name.to_owned(),
      status: CapabilityStatus::Unavailable,
    }
  }
}

/// Stable capability availability classification.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityStatus {
  /// The capability is available in the running composition.
  Available,
  /// The capability is intentionally unavailable in the running composition.
  Unavailable,
}
