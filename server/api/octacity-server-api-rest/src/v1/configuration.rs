use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Provider-neutral source-selection policy for one Repository version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RepositorySelectionPolicy {
  /// Mutable references that may be resolved for a Build.
  pub allowed_references: BTreeSet<String>,
  /// Reference selected when a Trigger supplies none.
  pub default_reference: Option<String>,
  /// Whether callers may supply an immutable revision directly.
  pub allow_exact_revision: bool,
}

/// Provider-neutral immutable Repository definition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryDefinition {
  /// Server-owned VCS integration identity.
  pub vcs_integration_id: String,
  /// Bounded credential-free provider locator.
  pub repository_locator: String,
  /// Allowed mutable-reference and exact-revision policy.
  pub selection: RepositorySelectionPolicy,
}

/// Request body for creating a Repository and version one.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateRepositoryRequest {
  /// Existing owning Project identity.
  pub project_id: String,
  /// Project-local Repository name.
  pub name: String,
  /// Initial immutable provider-neutral definition.
  pub definition: RepositoryDefinition,
}

/// Request body for appending the next immutable Repository version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublishRepositoryVersionRequest {
  /// Complete definition for the new immutable version.
  pub definition: RepositoryDefinition,
}

/// REST representation of one exact immutable Repository version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryResource {
  /// Stable Repository identity shared by all versions.
  pub id: String,
  /// Owning Project identity.
  pub project_id: String,
  /// Project-local Repository name.
  pub name: String,
  /// Exact positive immutable version.
  pub version: u64,
  /// Provider-neutral immutable definition.
  pub definition: RepositoryDefinition,
  /// Authoritative publication time as Unix milliseconds.
  pub published_at_unix_ms: i64,
}

/// Primitive value type accepted by one Build parameter.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParameterType {
  /// UTF-8 string value.
  String,
  /// Signed or unsigned integral JSON number.
  Integer,
  /// Boolean value.
  Boolean,
}

/// Type, presence rule, and default for one Build parameter.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterDefinition {
  /// Primitive type accepted from Trigger input.
  pub value_type: ParameterType,
  /// Whether input is required when no default exists.
  pub required: bool,
  /// Optional immutable default value.
  pub default: Option<Value>,
}

/// Bounded Build parameter schema.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterSchema {
  /// Parameter declarations indexed by stable name.
  pub parameters: BTreeMap<String, ParameterDefinition>,
  /// Whether undeclared parameters are rejected.
  pub deny_unknown: bool,
}

/// Normalized Trigger origin allowed by a Build Configuration.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
  /// Trusted-network management command.
  Manual,
  /// Persisted schedule occurrence.
  Scheduled,
  /// Authenticated external integration event.
  External,
  /// Server-generated internal event.
  Internal,
}

/// Placement requirements independent of one concrete Agent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentRequirements {
  /// Execution capabilities every eligible Agent must advertise.
  pub capabilities: BTreeSet<String>,
  /// Exact normalized inventory labels.
  pub labels: BTreeMap<String, String>,
  /// Minimum CPU capacity in thousandths of one logical CPU.
  pub minimum_cpu_millis: u32,
  /// Minimum available memory in bytes.
  pub minimum_memory_bytes: u64,
  /// Minimum available workspace bytes.
  pub minimum_disk_bytes: u64,
}

/// Runtime and isolation class requested for every materialized Job.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeClass {
  /// Execute directly on the Agent host.
  Native,
  /// Execute in an OCI container sharing the Agent kernel.
  OciProcess,
  /// Execute an OCI workload behind a hypervisor boundary.
  OciHypervisor,
}

/// Required operating system.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformOs {
  /// Linux.
  Linux,
  /// Windows.
  Windows,
  /// macOS.
  Macos,
}

/// Required CPU architecture using OCI wire names.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformArchitecture {
  /// 64-bit x86.
  Amd64,
  /// 64-bit ARM.
  Arm64,
}

/// Network access enforced by the selected execution backend.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum NetworkPolicy {
  /// Disable external network access.
  Disabled,
  /// Preserve normal network access.
  Unrestricted,
  /// Permit only explicit host names.
  Restricted {
    /// Stable host allowlist.
    allowed_hosts: BTreeSet<String>,
  },
}

/// Runtime, resource, network, and logical identity policy.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimePolicy {
  /// Required runtime and isolation class.
  pub class: RuntimeClass,
  /// Required operating system.
  pub operating_system: PlatformOs,
  /// Required CPU architecture.
  pub architecture: PlatformArchitecture,
  /// Immutable OCI image identity when required.
  pub immutable_image: Option<String>,
  /// CPU limit in thousandths of one logical CPU.
  pub cpu_millis: u32,
  /// Memory limit in bytes.
  pub memory_bytes: u64,
  /// Writable workspace limit in bytes.
  pub writable_disk_bytes: u64,
  /// Complete Job deadline in seconds.
  pub timeout_seconds: u64,
  /// Backend-neutral network policy.
  pub network: NetworkPolicy,
  /// Logical workload-identity profile; never provider credentials.
  pub workload_identity_profile: Option<String>,
}

/// Logical remote-cache namespace and permissions.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CachePolicy {
  /// Logical namespace; absence disables remote cache.
  pub namespace: Option<String>,
  /// Whether Jobs may read remote entries.
  pub read: bool,
  /// Whether Jobs may publish remote entries.
  pub write: bool,
}

/// Aggregate artifact and report ceilings.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactPolicy {
  /// Maximum artifacts produced by one Job.
  pub artifact_count: u32,
  /// Maximum aggregate artifact bytes.
  pub artifact_bytes: u64,
  /// Maximum reports produced by one Job.
  pub report_count: u32,
  /// Maximum aggregate report bytes.
  pub report_bytes: u64,
  /// Maximum bytes in one output.
  pub single_output_bytes: u64,
}

/// Terminal failure class eligible for automatic retry.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryClass {
  /// Repository-controlled execution failure.
  ExecutionFailure,
  /// Agent or server infrastructure failure.
  InfrastructureFailure,
}

/// Bounded automatic retry policy.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RetryPolicy {
  /// Maximum Attempts including the first.
  pub max_attempts: u16,
  /// Failure classes that may create another Attempt.
  pub retry_on: BTreeSet<RetryClass>,
}

/// Complete immutable Build Configuration definition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildConfigurationDefinition {
  /// Whether new Triggers may target this version.
  pub enabled: bool,
  /// Maximum active Jobs across Builds of this Configuration version.
  pub job_concurrency_limit: u32,
  /// Exact Repository identity.
  pub repository_id: String,
  /// Exact immutable Repository version.
  pub repository_version: u64,
  /// Exact Pipeline identity.
  pub pipeline_id: String,
  /// Exact immutable Pipeline version.
  pub pipeline_version: u64,
  /// Parameter declarations and defaults.
  pub parameters: ParameterSchema,
  /// Permitted normalized Trigger origins.
  pub triggers: BTreeSet<TriggerKind>,
  /// Placement requirements.
  pub agent_requirements: AgentRequirements,
  /// Configuration-local Pool restriction.
  pub allowed_pools: BTreeSet<String>,
  /// Runtime, resources, isolation, network, and identity policy.
  pub runtime: RuntimePolicy,
  /// Logical Octa secret profile; never secret material.
  #[serde(default)]
  pub secrets_profile: Option<String>,
  /// Remote-cache policy.
  pub cache: CachePolicy,
  /// Artifact and report ceilings.
  pub artifacts: ArtifactPolicy,
  /// Automatic retry policy.
  pub retry: RetryPolicy,
}

/// Request body for creating a Build Configuration and version one.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateBuildConfigurationRequest {
  /// Existing owning Project identity.
  pub project_id: String,
  /// Project-local configuration name.
  pub name: String,
  /// Initial immutable definition.
  pub definition: BuildConfigurationDefinition,
}

/// Request body for appending the next Build Configuration version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublishBuildConfigurationVersionRequest {
  /// Complete definition for the new immutable version.
  pub definition: BuildConfigurationDefinition,
}

/// REST representation of one exact immutable Build Configuration version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildConfigurationResource {
  /// Stable Build Configuration identity shared by all versions.
  pub id: String,
  /// Owning Project identity.
  pub project_id: String,
  /// Project-local configuration name.
  pub name: String,
  /// Exact positive immutable version.
  pub version: u64,
  /// Complete immutable definition.
  pub definition: BuildConfigurationDefinition,
  /// Authoritative publication time as Unix milliseconds.
  pub published_at_unix_ms: i64,
}
