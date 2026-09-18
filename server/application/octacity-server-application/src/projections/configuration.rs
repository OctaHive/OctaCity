use std::collections::{BTreeMap, BTreeSet};

use octacity_protocol::{PlatformArchitecture, PlatformOs};
use octacity_server_domain::{
  BuildConfigurationId, BuildConfigurationName, BuildConfigurationVersion, IntegrationId, PipelineId, PipelineVersion,
  PoolId, ProjectId, RepositoryId, RepositoryLocator, RepositoryName, RepositoryVersion, RuntimeClass, Timestamp,
};
use octacity_server_store::{ConfigurationNetworkPolicy, PublishedBuildConfiguration, PublishedRepository};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{CacheNamespace, IdentityProfileName};

use super::ProjectionError;

/// Source-selection policy exposed by the application boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RepositorySelectionProjection {
  /// Allowed mutable source references.
  pub allowed_references: BTreeSet<String>,
  /// Default mutable reference.
  pub default_reference: Option<String>,
  /// Whether callers may select an exact revision.
  pub allow_exact_revision: bool,
}

/// Primitive parameter type exposed by the application boundary.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParameterTypeProjection {
  /// UTF-8 string.
  String,
  /// Integral JSON number.
  Integer,
  /// Boolean.
  Boolean,
}

/// One parameter declaration exposed by the application boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ParameterDefinitionProjection {
  /// Accepted primitive type.
  pub value_type: ParameterTypeProjection,
  /// Whether a value is required.
  pub required: bool,
  /// Optional immutable default.
  pub default: Option<Value>,
}

/// Bounded parameter schema exposed by the application boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ParameterSchemaProjection {
  /// Declarations indexed by stable name.
  pub parameters: BTreeMap<String, ParameterDefinitionProjection>,
  /// Whether undeclared parameters are rejected.
  pub deny_unknown: bool,
}

/// Trigger origin exposed by the application boundary.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKindProjection {
  /// Trusted-network management command.
  Manual,
  /// Durable schedule occurrence.
  Scheduled,
  /// Authenticated external event.
  External,
  /// Server-generated internal event.
  Internal,
}

/// Placement requirements exposed by the application boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentRequirementsProjection {
  /// Required capabilities.
  pub capabilities: BTreeSet<String>,
  /// Required inventory labels.
  pub labels: BTreeMap<String, String>,
  /// Minimum CPU capacity.
  pub minimum_cpu_millis: u32,
  /// Minimum memory bytes.
  pub minimum_memory_bytes: u64,
  /// Minimum workspace bytes.
  pub minimum_disk_bytes: u64,
}

/// Runtime isolation class exposed by the application boundary.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeClassProjection {
  /// Native host execution.
  Native,
  /// OCI process isolation.
  OciProcess,
  /// OCI hypervisor isolation.
  OciHypervisor,
}

/// Operating system exposed by the application boundary.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformOsProjection {
  /// Linux.
  Linux,
  /// Windows.
  Windows,
  /// macOS.
  Macos,
}

/// CPU architecture exposed by the application boundary.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformArchitectureProjection {
  /// 64-bit x86.
  Amd64,
  /// 64-bit ARM.
  Arm64,
}

/// Network policy exposed by the application boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "mode", content = "allowed_hosts", rename_all = "snake_case")]
pub enum NetworkPolicyProjection {
  /// External network access is disabled.
  Disabled,
  /// Normal network access is preserved.
  Unrestricted,
  /// Only the named hosts are permitted.
  Restricted(BTreeSet<String>),
}

/// Artifact ceilings exposed by the application boundary.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArtifactPolicyProjection {
  /// Maximum artifact count.
  pub artifact_count: u32,
  /// Maximum aggregate artifact bytes.
  pub artifact_bytes: u64,
  /// Maximum report count.
  pub report_count: u32,
  /// Maximum aggregate report bytes.
  pub report_bytes: u64,
  /// Maximum bytes in one output.
  pub single_output_bytes: u64,
}

/// Retryable terminal failure class exposed by the application boundary.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryClassProjection {
  /// Repository-controlled execution failure.
  ExecutionFailure,
  /// Runtime or infrastructure failure.
  InfrastructureFailure,
}

/// Retry policy exposed by the application boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RetryPolicyProjection {
  /// Maximum Attempts including the first.
  pub max_attempts: u16,
  /// Failure classes eligible for retry.
  pub retry_on: BTreeSet<RetryClassProjection>,
}

/// Safe application projection of one immutable Repository version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RepositoryProjection {
  /// Stable Repository identity.
  pub id: RepositoryId,
  /// Owning Project.
  pub project_id: ProjectId,
  /// Project-local Repository name.
  pub name: RepositoryName,
  /// Exact immutable version.
  pub version: RepositoryVersion,
  /// Provider-neutral VCS integration identity.
  pub vcs_integration_id: IntegrationId,
  /// Bounded credential-free provider locator.
  pub repository_locator: RepositoryLocator,
  /// Allowed mutable references and exact-revision policy.
  pub selection: RepositorySelectionProjection,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

impl From<PublishedRepository> for RepositoryProjection {
  fn from(repository: PublishedRepository) -> Self {
    Self {
      id: repository.id,
      project_id: repository.project_id,
      name: repository.name,
      version: repository.version,
      vcs_integration_id: repository.definition.vcs_integration_id,
      repository_locator: repository.definition.repository_locator,
      selection: RepositorySelectionProjection {
        allowed_references: repository
          .definition
          .selection
          .allowed_references
          .into_iter()
          .map(|reference| reference.to_string())
          .collect(),
        default_reference: repository
          .definition
          .selection
          .default_reference
          .map(|reference| reference.to_string()),
        allow_exact_revision: repository.definition.selection.allow_exact_revision,
      },
      published_at: repository.published_at,
    }
  }
}

/// Safe runtime and resource policy exposed for one Build Configuration version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ConfigurationRuntimeProjection {
  /// Required runtime and isolation class.
  pub class: RuntimeClassProjection,
  /// Required operating system.
  pub operating_system: PlatformOsProjection,
  /// Required CPU architecture.
  pub architecture: PlatformArchitectureProjection,
  /// Immutable OCI image identity, when required by the runtime class.
  pub immutable_image: Option<String>,
  /// CPU execution limit in thousandths of one logical CPU.
  pub cpu_millis: u32,
  /// Memory execution limit in bytes.
  pub memory_bytes: u64,
  /// Writable workspace limit in bytes.
  pub writable_disk_bytes: u64,
  /// Complete Job deadline in seconds.
  pub timeout_seconds: u64,
  /// Backend-neutral network policy.
  pub network: NetworkPolicyProjection,
  /// Logical workload-identity profile; never provider credentials.
  pub workload_identity_profile: Option<IdentityProfileName>,
}

/// Safe remote-cache policy exposed for one Build Configuration version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ConfigurationCacheProjection {
  /// Logical cache namespace, or `None` when remote cache is disabled.
  pub namespace: Option<CacheNamespace>,
  /// Whether Jobs may read remote cache entries.
  pub read: bool,
  /// Whether Jobs may publish remote cache entries.
  pub write: bool,
}

/// Safe application projection of one immutable Build Configuration version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BuildConfigurationProjection {
  /// Stable Build Configuration identity.
  pub id: BuildConfigurationId,
  /// Owning Project.
  pub project_id: ProjectId,
  /// Sibling-unique name.
  pub name: BuildConfigurationName,
  /// Exact immutable version.
  pub version: BuildConfigurationVersion,
  /// Whether new Triggers may target this version.
  pub enabled: bool,
  /// Exact immutable Repository reference.
  pub repository_id: RepositoryId,
  /// Exact immutable Repository version.
  pub repository_version: RepositoryVersion,
  /// Exact immutable Pipeline reference.
  pub pipeline_id: PipelineId,
  /// Exact immutable Pipeline version.
  pub pipeline_version: PipelineVersion,
  /// Closed parameter schema and defaults.
  pub parameters: ParameterSchemaProjection,
  /// Permitted normalized Trigger origins.
  pub triggers: BTreeSet<TriggerKindProjection>,
  /// Backend-neutral Agent requirements.
  pub agent_requirements: AgentRequirementsProjection,
  /// Configuration-local Pool restriction.
  pub allowed_pools: BTreeSet<PoolId>,
  /// Runtime, resource, network, and logical identity policy.
  pub runtime: ConfigurationRuntimeProjection,
  /// Logical remote-cache policy.
  pub cache: ConfigurationCacheProjection,
  /// Artifact and report ceilings.
  pub artifacts: ArtifactPolicyProjection,
  /// Automatic retry policy.
  pub retry: RetryPolicyProjection,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

impl TryFrom<PublishedBuildConfiguration> for BuildConfigurationProjection {
  type Error = ProjectionError;

  fn try_from(configuration: PublishedBuildConfiguration) -> Result<Self, Self::Error> {
    let definition = configuration.definition;
    validate_configuration_projection(&definition)?;
    let workload_identity_profile = definition
      .runtime
      .workload_identity_profile
      .map(IdentityProfileName::new)
      .transpose()
      .map_err(|_| ProjectionError::InvalidConfigurationReference)?;
    let namespace = definition
      .cache
      .namespace
      .map(CacheNamespace::new)
      .transpose()
      .map_err(|_| ProjectionError::InvalidConfigurationReference)?;
    let parameters = ParameterSchemaProjection {
      parameters: definition
        .parameters
        .parameters
        .into_iter()
        .map(|(name, parameter)| {
          let value_type = match parameter.value_type {
            octacity_server_store::ParameterType::String => ParameterTypeProjection::String,
            octacity_server_store::ParameterType::Integer => ParameterTypeProjection::Integer,
            octacity_server_store::ParameterType::Boolean => ParameterTypeProjection::Boolean,
          };
          (
            name,
            ParameterDefinitionProjection {
              value_type,
              required: parameter.required,
              default: parameter.default,
            },
          )
        })
        .collect(),
      deny_unknown: definition.parameters.deny_unknown,
    };
    let triggers = definition
      .triggers
      .allowed
      .into_iter()
      .map(|kind| match kind {
        octacity_server_store::TriggerKind::Manual => TriggerKindProjection::Manual,
        octacity_server_store::TriggerKind::Scheduled => TriggerKindProjection::Scheduled,
        octacity_server_store::TriggerKind::External => TriggerKindProjection::External,
        octacity_server_store::TriggerKind::Internal => TriggerKindProjection::Internal,
      })
      .collect();
    let agent_requirements = AgentRequirementsProjection {
      capabilities: definition
        .agent_requirements
        .capabilities
        .into_iter()
        .map(|capability| capability.to_string())
        .collect(),
      labels: definition.agent_requirements.labels,
      minimum_cpu_millis: definition.agent_requirements.minimum_cpu_millis,
      minimum_memory_bytes: definition.agent_requirements.minimum_memory_bytes,
      minimum_disk_bytes: definition.agent_requirements.minimum_disk_bytes,
    };
    let runtime_class = match definition.runtime.class {
      RuntimeClass::Native => RuntimeClassProjection::Native,
      RuntimeClass::OciProcess => RuntimeClassProjection::OciProcess,
      RuntimeClass::OciHypervisor => RuntimeClassProjection::OciHypervisor,
    };
    let operating_system = match definition.runtime.operating_system {
      PlatformOs::Linux => PlatformOsProjection::Linux,
      PlatformOs::Windows => PlatformOsProjection::Windows,
      PlatformOs::Macos => PlatformOsProjection::Macos,
    };
    let architecture = match definition.runtime.architecture {
      PlatformArchitecture::Amd64 => PlatformArchitectureProjection::Amd64,
      PlatformArchitecture::Arm64 => PlatformArchitectureProjection::Arm64,
    };
    let network = match definition.runtime.network {
      ConfigurationNetworkPolicy::Disabled => NetworkPolicyProjection::Disabled,
      ConfigurationNetworkPolicy::Unrestricted => NetworkPolicyProjection::Unrestricted,
      ConfigurationNetworkPolicy::Restricted { allowed_hosts } => {
        NetworkPolicyProjection::Restricted(allowed_hosts.into_iter().map(|host| host.to_string()).collect())
      }
    };
    let artifacts = ArtifactPolicyProjection {
      artifact_count: definition.artifacts.artifact_count,
      artifact_bytes: definition.artifacts.artifact_bytes,
      report_count: definition.artifacts.report_count,
      report_bytes: definition.artifacts.report_bytes,
      single_output_bytes: definition.artifacts.single_output_bytes,
    };
    let retry = RetryPolicyProjection {
      max_attempts: definition.retry.max_attempts,
      retry_on: definition
        .retry
        .retry_on
        .into_iter()
        .map(|class| match class {
          octacity_server_store::RetryClass::ExecutionFailure => RetryClassProjection::ExecutionFailure,
          octacity_server_store::RetryClass::InfrastructureFailure => RetryClassProjection::InfrastructureFailure,
        })
        .collect(),
    };
    Ok(Self {
      id: configuration.id,
      project_id: configuration.project_id,
      name: configuration.name,
      version: configuration.version,
      enabled: definition.enabled,
      repository_id: definition.repository_id,
      repository_version: definition.repository_version,
      pipeline_id: definition.pipeline_id,
      pipeline_version: definition.pipeline_version,
      parameters,
      triggers,
      agent_requirements,
      allowed_pools: definition.allowed_pools,
      runtime: ConfigurationRuntimeProjection {
        class: runtime_class,
        operating_system,
        architecture,
        immutable_image: definition.runtime.immutable_image,
        cpu_millis: definition.runtime.cpu_millis,
        memory_bytes: definition.runtime.memory_bytes,
        writable_disk_bytes: definition.runtime.writable_disk_bytes,
        timeout_seconds: definition.runtime.timeout_seconds,
        network,
        workload_identity_profile,
      },
      cache: ConfigurationCacheProjection {
        namespace,
        read: definition.cache.read,
        write: definition.cache.write,
      },
      artifacts,
      retry,
      published_at: configuration.published_at,
    })
  }
}

pub(crate) fn validate_configuration_projection(
  definition: &octacity_server_store::BuildConfigurationDefinition,
) -> Result<(), ProjectionError> {
  definition
    .validate()
    .map_err(|_| ProjectionError::InvalidConfigurationSnapshot)?;
  definition
    .runtime
    .workload_identity_profile
    .as_deref()
    .map(IdentityProfileName::new)
    .transpose()
    .map_err(|_| ProjectionError::InvalidConfigurationReference)?;
  definition
    .cache
    .namespace
    .as_deref()
    .map(CacheNamespace::new)
    .transpose()
    .map_err(|_| ProjectionError::InvalidConfigurationReference)?;
  Ok(())
}
