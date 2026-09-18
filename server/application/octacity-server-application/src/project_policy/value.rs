use std::{collections::BTreeSet, fmt, str::FromStr};

pub use octacity_server_domain::{ArtifactPolicy, RuntimeClass};
use octacity_server_domain::{PoolId, ProjectId, ProjectPolicyVersion, RepositoryId};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

/// Maximum UTF-8 bytes in a logical policy reference.
pub const MAX_POLICY_REFERENCE_BYTES: usize = 128;

fn validate_reference(value: &str) -> bool {
  !value.is_empty()
    && value.len() <= MAX_POLICY_REFERENCE_BYTES
    && value.trim() == value
    && !value.chars().any(char::is_control)
}

macro_rules! bounded_reference {
  ($name:ident, $documentation:literal, $error:literal) => {
    #[doc = $documentation]
    #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
    pub struct $name(String);

    impl $name {
      /// Constructs a bounded, visible logical reference.
      pub fn new(value: impl Into<String>) -> Result<Self, &'static str> {
        let value = value.into();
        if !validate_reference(&value) {
          return Err($error);
        }
        Ok(Self(value))
      }

      /// Borrows the logical reference.
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
      type Err = &'static str;

      fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
      }
    }

    impl Serialize for $name {
      fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
      where
        S: Serializer,
      {
        serializer.serialize_str(&self.0)
      }
    }

    impl<'de> Deserialize<'de> for $name {
      fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
      where
        D: Deserializer<'de>,
      {
        String::deserialize(deserializer).and_then(|value| Self::new(value).map_err(D::Error::custom))
      }
    }
  };
}

bounded_reference!(
  SecretProfileName,
  "Logical secret profile selectable by builds in a Project.",
  "invalid secret profile name"
);
bounded_reference!(
  IdentityProfileName,
  "Logical workload-identity profile selectable by builds in a Project.",
  "invalid workload identity profile name"
);
bounded_reference!(
  CacheNamespace,
  "Logical remote-cache namespace selectable by builds in a Project.",
  "invalid cache namespace"
);

/// Cache authority and quota inherited by a Project.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CachePolicy {
  /// Logical namespaces from which builds may request a scoped session.
  pub namespaces: BTreeSet<CacheNamespace>,
  /// Whether a scoped session may read cache entries.
  pub read: bool,
  /// Whether a scoped session may publish cache entries.
  pub write: bool,
  /// Maximum authoritative cache bytes attributable to the Project.
  pub max_bytes: u64,
}

/// Project-wide concurrency ceilings.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConcurrencyPolicy {
  /// Maximum simultaneously active Builds below this Project.
  pub active_builds: u32,
  /// Maximum simultaneously active Jobs below this Project.
  pub active_jobs: u32,
}

/// Maximum retention periods for Build Result components.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionPolicy {
  /// Maximum seconds to retain Build metadata and configuration snapshots.
  pub build_seconds: u64,
  /// Maximum seconds to retain archived Build logs.
  pub log_seconds: u64,
  /// Maximum seconds to retain produced artifacts and reports.
  pub artifact_seconds: u64,
  /// Maximum seconds to retain remote-cache entries.
  pub cache_seconds: u64,
}

/// Exact effective policy after resolving one Project lineage.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectPolicy {
  /// Agent Pools in which Jobs may be placed.
  pub pools: BTreeSet<PoolId>,
  /// Repositories that Build Configurations may select.
  pub repositories: BTreeSet<RepositoryId>,
  /// Logical secret profiles that may be requested.
  pub secret_profiles: BTreeSet<SecretProfileName>,
  /// Logical workload-identity profiles that may be requested.
  pub identity_profiles: BTreeSet<IdentityProfileName>,
  /// Runtime and isolation classes that may be requested.
  pub runtimes: BTreeSet<RuntimeClass>,
  /// Remote-cache namespace, permission, and quota policy.
  pub cache: CachePolicy,
  /// Artifact and report output ceilings.
  pub artifacts: ArtifactPolicy,
  /// Project-wide Build and Job concurrency ceilings.
  pub concurrency: ConcurrencyPolicy,
  /// Maximum retention periods for durable Build Result data.
  pub retention: RetentionPolicy,
}

/// Explicit operation applied to one inherited policy category.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "mode", content = "value", rename_all = "snake_case", deny_unknown_fields)]
pub enum PolicyDirective<T> {
  /// Preserve the exact effective value inherited from the direct parent.
  Inherit,
  /// Use the supplied exact value, provided it does not broaden the parent.
  Replace(T),
  /// Intersect permissions and lower ceilings using the supplied restriction.
  Narrow(T),
}

/// One immutable Project policy version in a root-to-leaf lineage.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectPolicyLayer {
  /// Project that owns this policy version.
  pub project_id: ProjectId,
  /// Direct parent expected immediately before this layer.
  pub parent_id: Option<ProjectId>,
  /// Immutable policy version selected for resolution.
  pub version: ProjectPolicyVersion,
  /// Agent Pool inheritance instruction.
  pub pools: PolicyDirective<BTreeSet<PoolId>>,
  /// Repository inheritance instruction.
  pub repositories: PolicyDirective<BTreeSet<RepositoryId>>,
  /// Secret-profile inheritance instruction.
  pub secret_profiles: PolicyDirective<BTreeSet<SecretProfileName>>,
  /// Workload-identity-profile inheritance instruction.
  pub identity_profiles: PolicyDirective<BTreeSet<IdentityProfileName>>,
  /// Runtime inheritance instruction.
  pub runtimes: PolicyDirective<BTreeSet<RuntimeClass>>,
  /// Cache inheritance instruction.
  pub cache: PolicyDirective<CachePolicy>,
  /// Artifact inheritance instruction.
  pub artifacts: PolicyDirective<ArtifactPolicy>,
  /// Concurrency inheritance instruction.
  pub concurrency: PolicyDirective<ConcurrencyPolicy>,
  /// Retention inheritance instruction.
  pub retention: PolicyDirective<RetentionPolicy>,
}

/// Project and immutable policy version contributing to an effective snapshot.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PolicySource {
  /// Project that supplied one layer.
  pub project_id: ProjectId,
  /// Immutable policy version selected from that Project.
  pub version: ProjectPolicyVersion,
}

/// Build-ready immutable effective policy and its exact source versions.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EffectiveProjectPolicy {
  /// Source versions ordered from root Project to leaf Project.
  pub sources: Vec<PolicySource>,
  /// Exact resolved permissions and ceilings.
  pub policy: ProjectPolicy,
}
