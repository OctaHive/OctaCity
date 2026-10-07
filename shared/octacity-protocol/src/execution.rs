//! Provider-neutral execution contract introduced after the legacy Native/OCI wire model.

use std::{collections::BTreeSet, fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize};

use crate::{NetworkPolicy, PlatformSpec, immutable_oci_reference, non_empty};

/// Legacy execution contract understood by [`crate::JobSpecV1`].
pub const EXECUTION_CONTRACT_V1: u16 = 1;
/// Provider-neutral execution contract understood by [`crate::JobSpecV2`].
pub const EXECUTION_CONTRACT_V2: u16 = 2;
/// Protected managed-execution contract understood by [`crate::JobSpecV3`].
pub const EXECUTION_CONTRACT_V3: u16 = 3;
/// Execution-contract revisions implemented by this protocol release.
pub const SUPPORTED_EXECUTION_CONTRACTS: ExecutionContractRange = ExecutionContractRange {
  min: EXECUTION_CONTRACT_V1,
  max: EXECUTION_CONTRACT_V3,
};
/// Maximum UTF-8 bytes in one execution provider or environment identity.
pub const MAX_EXECUTION_IDENTITY_BYTES: usize = 256;

macro_rules! execution_identity {
  ($(#[$meta:meta])* $name:ident, $label:literal) => {
    $(#[$meta])*
    #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
    #[serde(transparent)]
    pub struct $name(String);

    impl $name {
      /// Creates a bounded identity safe for protocol and diagnostic use.
      pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        validate_execution_identity($label, &value)?;
        Ok(Self(value))
      }

      /// Borrows the validated wire value.
      #[must_use]
      pub fn as_str(&self) -> &str {
        &self.0
      }
    }

    impl fmt::Display for $name {
      fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
      }
    }

    impl FromStr for $name {
      type Err = String;

      fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
      }
    }

    impl<'de> Deserialize<'de> for $name {
      fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
      where
        D: Deserializer<'de>,
      {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
      }
    }
  };
}

execution_identity!(
  /// Bounded operator-selected execution-provider identity.
  ExecutionProviderId,
  "execution provider"
);
execution_identity!(
  /// Bounded stable identity of a qualified execution environment.
  ExecutionEnvironmentId,
  "execution environment"
);

fn validate_execution_identity(name: &str, value: &str) -> Result<(), String> {
  if value.is_empty()
    || value.len() > MAX_EXECUTION_IDENTITY_BYTES
    || value.trim() != value
    || value.chars().any(char::is_control)
  {
    Err(format!(
      "{name} must contain 1 to {MAX_EXECUTION_IDENTITY_BYTES} UTF-8 bytes without surrounding whitespace or control characters"
    ))
  } else {
    Ok(())
  }
}

/// Inclusive range of execution-contract revisions supported by one peer.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionContractRange {
  /// Oldest supported revision.
  pub min: u16,
  /// Newest supported revision.
  pub max: u16,
}

impl ExecutionContractRange {
  /// Selects the newest mutually supported revision.
  #[must_use]
  pub const fn negotiate(self, peer: Self) -> Option<u16> {
    let minimum = if self.min > peer.min { self.min } else { peer.min };
    let maximum = if self.max < peer.max { self.max } else { peer.max };
    if minimum == 0 || minimum > maximum {
      None
    } else {
      Some(maximum)
    }
  }

  /// Reports whether the inclusive range contains one revision.
  #[must_use]
  pub const fn contains(self, version: u16) -> bool {
    version >= self.min && version <= self.max
  }

  /// Validates a non-empty ordered revision range.
  pub fn validate(self) -> Result<(), String> {
    if self.min == 0 || self.min > self.max {
      Err("execution-contract range is invalid".to_owned())
    } else {
      Ok(())
    }
  }
}

/// Provider-neutral execution boundary requested by policy and signed JobSpec intent.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
  /// Execute directly on the Agent host without claiming workload isolation.
  Host,
  /// Execute inside a bounded workload environment without promising a guest-machine boundary.
  Isolation,
  /// Execute behind a hardware-virtualized guest boundary.
  Virtualization,
}

/// Observable guarantee required from an execution provider.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionGuarantee {
  /// Workload paths are projected through an isolated filesystem boundary.
  FilesystemIsolation,
  /// Workload processes cannot join or control unrelated host processes.
  ProcessIsolation,
  /// Network policy is enforced at the workload boundary.
  NetworkIsolation,
  /// CPU, memory, and writable storage are enforced and accounted.
  ResourceIsolation,
  /// Workload execution is separated by a hardware-virtualized guest boundary.
  HardwareVirtualization,
}

/// Provider-neutral platform and guarantees requested by a v2 JobSpec.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionTargetV2 {
  /// Requested execution mode.
  pub mode: ExecutionMode,
  /// Exact platform on which the Agent process and provider run.
  pub host_platform: PlatformSpec,
  /// Exact platform observed by the runner, whether host, workload, or guest.
  pub target_platform: PlatformSpec,
  /// Complete set of guarantees the selected provider must enforce.
  pub required_guarantees: BTreeSet<ExecutionGuarantee>,
  /// Optional immutable workload or guest image, never a provider identity.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub immutable_image: Option<String>,
}

impl ExecutionTargetV2 {
  /// Revalidates the mode, platform, guarantee, and image relationship.
  pub fn validate(&self) -> Result<(), String> {
    let required = guarantees_for(self.mode);
    if self.required_guarantees != required {
      return Err("execution guarantees do not match the requested execution mode".to_owned());
    }
    match self.mode {
      ExecutionMode::Host => {
        if self.host_platform != self.target_platform || self.immutable_image.is_some() {
          return Err("host execution requires one identical host/target platform and no image".to_owned());
        }
      }
      ExecutionMode::Isolation => {
        if self.host_platform != self.target_platform && self.immutable_image.is_none() {
          return Err("cross-platform isolation requires an immutable image".to_owned());
        }
      }
      ExecutionMode::Virtualization if self.immutable_image.is_none() => {
        return Err("virtualization requires an immutable guest image".to_owned());
      }
      ExecutionMode::Virtualization => {}
    }
    if let Some(image) = &self.immutable_image {
      immutable_oci_reference("runtime.target.immutable_image", image)?;
    }
    Ok(())
  }
}

/// Resource and network limits paired with a provider-neutral execution target.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeSpecV2 {
  /// Requested provider-neutral execution target.
  pub target: ExecutionTargetV2,
  /// CPU allocation in thousandths of one logical CPU.
  pub cpu_millis: u32,
  /// Maximum memory in bytes.
  pub memory_bytes: u64,
  /// Maximum writable workspace capacity in bytes.
  pub writable_disk_bytes: u64,
  /// Complete preparation and execution deadline in seconds.
  pub timeout_seconds: u64,
  /// Network access the selected provider must enforce.
  pub network: NetworkPolicy,
  /// Optional operator-provisioned workload identity profile.
  #[serde(default)]
  pub workload_identity_profile: Option<String>,
}

impl RuntimeSpecV2 {
  /// Revalidates all provider-neutral runtime invariants.
  pub fn validate(&self) -> Result<(), String> {
    self.target.validate()?;
    if self.cpu_millis == 0 || self.memory_bytes == 0 || self.writable_disk_bytes == 0 || self.timeout_seconds == 0 {
      return Err("runtime limits must be greater than zero".to_owned());
    }
    if let Some(profile) = &self.workload_identity_profile {
      non_empty("runtime.workload_identity_profile", profile)?;
    }
    if let NetworkPolicy::Restricted { allowed_hosts } = &self.network
      && (allowed_hosts.is_empty() || allowed_hosts.iter().any(|host| host.trim().is_empty()))
    {
      return Err("a restricted network policy requires non-empty allowed_hosts".to_owned());
    }
    Ok(())
  }
}

/// One provider-backed execution contract advertised by an Agent.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionCapabilityV2 {
  /// Operator-configured provider identity used only for inventory and diagnostics.
  pub provider: ExecutionProviderId,
  /// Provider-neutral execution mode.
  pub mode: ExecutionMode,
  /// Exact platform hosting the Agent and provider.
  pub host_platform: PlatformSpec,
  /// Exact platform exposed to the runner.
  pub target_platform: PlatformSpec,
  /// Guarantees enforced by this provider route.
  pub guarantees: BTreeSet<ExecutionGuarantee>,
  /// Whether the route accepts immutable workload or guest images.
  pub immutable_images: bool,
}

impl ExecutionCapabilityV2 {
  /// Validates provider evidence without making its identity a placement requirement.
  pub fn validate(&self) -> Result<(), String> {
    if self.guarantees != guarantees_for(self.mode) {
      return Err("execution capability guarantees do not match its mode".to_owned());
    }
    match self.mode {
      ExecutionMode::Host if self.host_platform != self.target_platform || self.immutable_images => {
        Err("host capability requires one identical platform and no image support".to_owned())
      }
      ExecutionMode::Isolation if self.host_platform != self.target_platform && !self.immutable_images => {
        Err("cross-platform isolation must support immutable images".to_owned())
      }
      ExecutionMode::Virtualization if !self.immutable_images => {
        Err("virtualization capability must support immutable guest images".to_owned())
      }
      ExecutionMode::Host | ExecutionMode::Isolation | ExecutionMode::Virtualization => Ok(()),
    }
  }

  /// Reports whether this provider route satisfies one signed provider-neutral target.
  #[must_use]
  pub fn satisfies(&self, target: &ExecutionTargetV2) -> bool {
    self.validate().is_ok()
      && target.validate().is_ok()
      && self.mode == target.mode
      && self.host_platform == target.host_platform
      && self.target_platform == target.target_platform
      && target.required_guarantees.is_subset(&self.guarantees)
      && (target.immutable_image.is_none() || self.immutable_images)
  }
}

/// Concrete provider evidence attached to cache, telemetry, and diagnostic records.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionEvidenceV2 {
  /// Operator-configured provider identity; never part of JobSpec intent or Project policy.
  pub provider: ExecutionProviderId,
  /// Provider-neutral execution target that was actually enforced.
  pub target: ExecutionTargetV2,
}

impl ExecutionEvidenceV2 {
  /// Revalidates the bounded provider name and semantic execution target.
  pub fn validate(&self) -> Result<(), String> {
    self.target.validate()
  }
}

/// Physical cache identity for one qualified execution environment.
///
/// Provider identity belongs here because cache reuse is evidence about the
/// concrete environment that produced bytes. Signed execution intent remains
/// provider-neutral.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionCacheIdentityV2 {
  /// Concrete provider and semantic target actually used.
  pub execution: ExecutionEvidenceV2,
  /// Stable operator-owned host, image, or guest environment identity.
  pub environment: ExecutionEnvironmentId,
}

impl ExecutionCacheIdentityV2 {
  /// Revalidates execution evidence and the environment identity.
  pub fn validate(&self) -> Result<(), String> {
    self.execution.validate()
  }
}

/// Returns the exact baseline guarantees promised by one provider-neutral mode.
#[must_use]
pub fn guarantees_for(mode: ExecutionMode) -> BTreeSet<ExecutionGuarantee> {
  use ExecutionGuarantee::{
    FilesystemIsolation, HardwareVirtualization, NetworkIsolation, ProcessIsolation, ResourceIsolation,
  };
  match mode {
    ExecutionMode::Host => BTreeSet::new(),
    ExecutionMode::Isolation => BTreeSet::from([
      FilesystemIsolation,
      ProcessIsolation,
      NetworkIsolation,
      ResourceIsolation,
    ]),
    ExecutionMode::Virtualization => BTreeSet::from([
      FilesystemIsolation,
      ProcessIsolation,
      NetworkIsolation,
      ResourceIsolation,
      HardwareVirtualization,
    ]),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{PlatformArchitecture, PlatformOs};

  const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

  fn linux() -> PlatformSpec {
    PlatformSpec {
      os: PlatformOs::Linux,
      architecture: PlatformArchitecture::Amd64,
    }
  }

  #[test]
  fn negotiation_selects_the_latest_common_revision() {
    assert_eq!(
      ExecutionContractRange { min: 1, max: 3 }.negotiate(ExecutionContractRange { min: 1, max: 2 }),
      Some(2)
    );
    assert_eq!(
      ExecutionContractRange { min: 1, max: 3 }.negotiate(ExecutionContractRange { min: 3, max: 3 }),
      Some(3)
    );
    assert_eq!(
      ExecutionContractRange { min: 1, max: 2 }.negotiate(ExecutionContractRange { min: 1, max: 1 }),
      Some(1)
    );
    assert_eq!(
      ExecutionContractRange { min: 2, max: 2 }.negotiate(ExecutionContractRange { min: 1, max: 1 }),
      None
    );
  }

  #[test]
  fn modes_require_their_exact_observable_guarantees() {
    let mut target = ExecutionTargetV2 {
      mode: ExecutionMode::Virtualization,
      host_platform: linux(),
      target_platform: linux(),
      required_guarantees: guarantees_for(ExecutionMode::Virtualization),
      immutable_image: Some(format!("example.invalid/guest@sha256:{DIGEST}")),
    };
    assert!(target.validate().is_ok());
    target
      .required_guarantees
      .remove(&ExecutionGuarantee::HardwareVirtualization);
    assert!(target.validate().is_err());
  }

  #[test]
  fn provider_identity_is_evidence_not_signed_target_intent() {
    let target = ExecutionTargetV2 {
      mode: ExecutionMode::Isolation,
      host_platform: linux(),
      target_platform: linux(),
      required_guarantees: guarantees_for(ExecutionMode::Isolation),
      immutable_image: None,
    };
    let capability = ExecutionCapabilityV2 {
      provider: ExecutionProviderId::new("containerd").unwrap(),
      mode: ExecutionMode::Isolation,
      host_platform: linux(),
      target_platform: linux(),
      guarantees: guarantees_for(ExecutionMode::Isolation),
      immutable_images: true,
    };
    assert!(capability.satisfies(&target));
    assert!(serde_json::to_value(target).unwrap().get("provider").is_none());
  }

  #[test]
  fn cache_identity_distinguishes_providers_for_the_same_semantic_target() {
    let target = ExecutionTargetV2 {
      mode: ExecutionMode::Isolation,
      host_platform: linux(),
      target_platform: linux(),
      required_guarantees: guarantees_for(ExecutionMode::Isolation),
      immutable_image: None,
    };
    let first = ExecutionCacheIdentityV2 {
      execution: ExecutionEvidenceV2 {
        provider: ExecutionProviderId::new("containerd").unwrap(),
        target: target.clone(),
      },
      environment: ExecutionEnvironmentId::new("qualified-linux-host-v1").unwrap(),
    };
    let mut second = first.clone();
    second.execution.provider = ExecutionProviderId::new("apple-vf-isolation").unwrap();
    first.validate().unwrap();
    second.validate().unwrap();
    assert_ne!(first, second);
    assert_eq!(first.execution.target, second.execution.target);
  }

  #[test]
  fn execution_identities_reject_unbounded_or_diagnostic_unsafe_values() {
    assert!(ExecutionProviderId::new("").is_err());
    assert!(ExecutionProviderId::new(" provider").is_err());
    assert!(ExecutionProviderId::new("provider\nforged").is_err());
    assert!(ExecutionEnvironmentId::new("x".repeat(MAX_EXECUTION_IDENTITY_BYTES + 1)).is_err());
    assert!(serde_json::from_value::<ExecutionProviderId>(serde_json::json!("provider\u{0000}")).is_err());
  }
}
