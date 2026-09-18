use std::collections::BTreeSet;

use octacity_protocol::{PlatformArchitecture, PlatformOs};
use octacity_server_domain::{
  ArtifactPolicy, BuildConfigurationId, BuildConfigurationName, BuildConfigurationVersion, PipelineId, PipelineVersion,
  PoolId, ProjectId, RepositoryId, RepositoryVersion, RuntimeClass, Timestamp,
};
use octacity_server_store::{
  ConfigurationAgentRequirements, ConfigurationNetworkPolicy, ConfigurationRetryPolicy, ConfigurationTriggerPolicy,
  ParameterSchema, PublishedBuildConfiguration,
};
use serde::Serialize;

use crate::{CacheNamespace, IdentityProfileName};

use super::ProjectionError;

/// Safe runtime and resource policy exposed for one Build Configuration version.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ConfigurationRuntimeProjection {
  /// Required runtime and isolation class.
  pub class: RuntimeClass,
  /// Required operating system.
  pub operating_system: PlatformOs,
  /// Required CPU architecture.
  pub architecture: PlatformArchitecture,
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
  pub network: ConfigurationNetworkPolicy,
  /// Logical workload-identity profile; never provider credentials.
  pub workload_identity_profile: Option<IdentityProfileName>,
}

/// Safe remote-cache policy exposed for one Build Configuration version.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ConfigurationCacheProjection {
  /// Logical cache namespace, or `None` when remote cache is disabled.
  pub namespace: Option<CacheNamespace>,
  /// Whether Jobs may read remote cache entries.
  pub read: bool,
  /// Whether Jobs may publish remote cache entries.
  pub write: bool,
}

/// Safe application projection of one immutable Build Configuration version.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
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
  pub parameters: ParameterSchema,
  /// Permitted normalized Trigger origins.
  pub triggers: ConfigurationTriggerPolicy,
  /// Backend-neutral Agent requirements.
  pub agent_requirements: ConfigurationAgentRequirements,
  /// Configuration-local Pool restriction.
  pub allowed_pools: BTreeSet<PoolId>,
  /// Runtime, resource, network, and logical identity policy.
  pub runtime: ConfigurationRuntimeProjection,
  /// Logical remote-cache policy.
  pub cache: ConfigurationCacheProjection,
  /// Artifact and report ceilings.
  pub artifacts: ArtifactPolicy,
  /// Automatic retry policy.
  pub retry: ConfigurationRetryPolicy,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

impl TryFrom<PublishedBuildConfiguration> for BuildConfigurationProjection {
  type Error = ProjectionError;

  fn try_from(configuration: PublishedBuildConfiguration) -> Result<Self, Self::Error> {
    let definition = configuration.definition;
    definition
      .validate()
      .map_err(|_| ProjectionError::InvalidConfigurationSnapshot)?;
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
      parameters: definition.parameters,
      triggers: definition.triggers,
      agent_requirements: definition.agent_requirements,
      allowed_pools: definition.allowed_pools,
      runtime: ConfigurationRuntimeProjection {
        class: definition.runtime.class,
        operating_system: definition.runtime.operating_system,
        architecture: definition.runtime.architecture,
        immutable_image: definition.runtime.immutable_image,
        cpu_millis: definition.runtime.cpu_millis,
        memory_bytes: definition.runtime.memory_bytes,
        writable_disk_bytes: definition.runtime.writable_disk_bytes,
        timeout_seconds: definition.runtime.timeout_seconds,
        network: definition.runtime.network,
        workload_identity_profile,
      },
      cache: ConfigurationCacheProjection {
        namespace,
        read: definition.cache.read,
        write: definition.cache.write,
      },
      artifacts: definition.artifacts,
      retry: definition.retry,
      published_at: configuration.published_at,
    })
  }
}
