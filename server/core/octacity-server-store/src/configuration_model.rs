use std::collections::{BTreeMap, BTreeSet};

use octacity_protocol::{PlatformArchitecture, PlatformOs};
use octacity_server_domain::{
  ArtifactPolicy, BuildConfigurationId, BuildConfigurationName, BuildConfigurationVersion, IntegrationId, NetworkHost,
  PipelineId, PipelineVersion, PoolId, ProjectId, RepositoryId, RepositoryLocator, RepositoryName, RepositoryVersion,
  RuntimeClass, SourceReference, Timestamp,
};
use octacity_server_pipeline::ExecutionCapability;
use octacity_server_trigger::TriggerKind;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{IdempotencyKey, MAX_STRUCTURED_DOCUMENT_BYTES, MutationDisposition, StoreInputError};

/// Maximum encoded bytes in one Repository definition.
pub const MAX_REPOSITORY_DEFINITION_BYTES: usize = 256 * 1_024;
/// Maximum encoded bytes in one Build Configuration definition.
pub const MAX_BUILD_CONFIGURATION_BYTES: usize = 1024 * 1_024;
/// Maximum parameters declared by one Build Configuration.
pub const MAX_CONFIGURATION_PARAMETERS: usize = 128;
/// Maximum mutable VCS references allowed by one Repository definition.
pub const MAX_REPOSITORY_REFERENCES: usize = 128;
/// Maximum allowed Pools referenced by one Build Configuration.
pub const MAX_CONFIGURATION_POOLS: usize = 64;
/// Maximum required Agent labels in one Build Configuration.
pub const MAX_AGENT_REQUIREMENT_LABELS: usize = 64;
/// Maximum UTF-8 bytes in a configuration-owned identifier or label.
pub const MAX_CONFIGURATION_LABEL_BYTES: usize = 128;
/// Maximum attempts, including the first, allowed by one retry policy.
pub const MAX_RETRY_ATTEMPTS: u16 = 100;

/// Provider-neutral rules for selecting source revisions from a Repository.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RepositorySelectionPolicy {
  /// Exact mutable references that may be resolved for a Build.
  pub allowed_references: BTreeSet<SourceReference>,
  /// Optional reference selected when a Trigger supplies none.
  pub default_reference: Option<SourceReference>,
  /// Whether callers may supply an already immutable revision.
  pub allow_exact_revision: bool,
}

impl RepositorySelectionPolicy {
  fn validate(&self) -> Result<(), StoreInputError> {
    if self.allowed_references.len() > MAX_REPOSITORY_REFERENCES
      || self
        .default_reference
        .as_ref()
        .is_some_and(|reference| !self.allowed_references.contains(reference))
      || (self.allowed_references.is_empty() && !self.allow_exact_revision)
    {
      return Err(StoreInputError::InvalidRepositoryDefinition);
    }
    Ok(())
  }
}

/// Versioned connection-independent Repository definition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryDefinition {
  /// VCS integration selected through the provider-neutral VCS seam.
  pub vcs_integration_id: IntegrationId,
  /// Provider-owned bounded repository locator, treated only as data.
  pub repository_locator: RepositoryLocator,
  /// Rules for mutable-reference and exact-revision selection.
  pub selection: RepositorySelectionPolicy,
}

impl RepositoryDefinition {
  /// Revalidates the complete immutable definition at an adapter seam.
  pub fn validate(&self) -> Result<(), StoreInputError> {
    self.selection.validate()?;
    validate_encoded(
      self,
      MAX_REPOSITORY_DEFINITION_BYTES,
      StoreInputError::InvalidRepositoryDefinition,
    )
  }
}

/// One immutable published Repository version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedRepository {
  /// Stable Repository identity shared by all versions.
  pub id: RepositoryId,
  /// Project that owns the Repository identity.
  pub project_id: ProjectId,
  /// Stable sibling-unique Repository name.
  pub name: RepositoryName,
  /// Positive immutable version number.
  pub version: RepositoryVersion,
  /// Exact immutable Repository definition.
  pub definition: RepositoryDefinition,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

/// Atomic request to create a Repository and immutable version one.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CreateRepository {
  /// Stable Repository identity selected by the application.
  pub id: RepositoryId,
  /// Existing owning Project.
  pub project_id: ProjectId,
  /// Name unique among Repository identities in the Project.
  pub name: RepositoryName,
  /// Initial immutable definition.
  pub definition: RepositoryDefinition,
  /// Stable replay identity for this command.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

/// Atomic request to append exactly the next Repository version.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PublishRepositoryVersion {
  /// Existing Repository identity.
  pub id: RepositoryId,
  /// Version that must still be current.
  pub expected_current_version: RepositoryVersion,
  /// Complete next immutable definition.
  pub definition: RepositoryDefinition,
  /// Stable replay identity for this command.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

/// Result of one Repository create or publish command.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RepositoryMutationOutcome {
  /// Whether the command was newly applied or exactly replayed.
  pub disposition: MutationDisposition,
  /// Immutable Repository version committed by the original command.
  pub repository: PublishedRepository,
}

/// Primitive value accepted by one declared Build parameter.
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

impl ParameterType {
  fn accepts(self, value: &Value) -> bool {
    match self {
      Self::String => value.is_string(),
      Self::Integer => value.as_i64().is_some() || value.as_u64().is_some(),
      Self::Boolean => value.is_boolean(),
    }
  }
}

/// Failure while resolving one Trigger's parameter values against a schema.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ParameterResolutionError {
  /// The supplied map exceeds the configured item or encoded-byte bound.
  #[error("build parameters exceed their item or encoded-byte bound")]
  TooLarge,
  /// A parameter is absent from a closed schema.
  #[error("unknown build parameter: {0}")]
  Unknown(String),
  /// A required parameter has neither a supplied value nor a default.
  #[error("required build parameter is missing: {0}")]
  Missing(String),
  /// A declared parameter does not match its configured primitive type.
  #[error("build parameter has the wrong type: {0}")]
  InvalidType(String),
  /// An open-schema parameter cannot be represented as an execution variable.
  #[error("build parameter cannot be represented as an execution variable: {0}")]
  UnsupportedValue(String),
}

/// Type, presence rule, and optional default for one Build parameter.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterDefinition {
  /// Value type accepted from Trigger input.
  pub value_type: ParameterType,
  /// Whether every Trigger must supply the value when no default exists.
  pub required: bool,
  /// Optional immutable default applied before Build acceptance.
  pub default: Option<Value>,
}

/// Closed bounded parameter schema for one Build Configuration version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterSchema {
  /// Named parameter declarations in canonical lexical order.
  pub parameters: BTreeMap<String, ParameterDefinition>,
  /// Whether parameters absent from this map are rejected.
  pub deny_unknown: bool,
}

impl ParameterSchema {
  fn validate(&self) -> Result<(), StoreInputError> {
    if self.parameters.len() > MAX_CONFIGURATION_PARAMETERS
      || self.parameters.iter().any(|(name, definition)| {
        !valid_identifier(name)
          || definition
            .default
            .as_ref()
            .is_some_and(|value| !definition.value_type.accepts(value))
      })
    {
      return Err(StoreInputError::InvalidBuildConfiguration);
    }
    Ok(())
  }

  /// Applies defaults and validates a bounded Trigger-supplied parameter map.
  ///
  /// Open schemas still accept only named primitive values because every
  /// resolved parameter is projected into the signed execution environment.
  pub fn resolve(
    &self,
    supplied: &BTreeMap<String, Value>,
  ) -> Result<BTreeMap<String, Value>, ParameterResolutionError> {
    if supplied.len() > MAX_CONFIGURATION_PARAMETERS
      || !serde_json::to_vec(supplied).is_ok_and(|encoded| encoded.len() <= MAX_STRUCTURED_DOCUMENT_BYTES)
    {
      return Err(ParameterResolutionError::TooLarge);
    }
    if self.deny_unknown
      && let Some(name) = supplied.keys().find(|name| !self.parameters.contains_key(*name))
    {
      return Err(ParameterResolutionError::Unknown(name.clone()));
    }
    if !self.deny_unknown
      && let Some((name, _)) = supplied
        .iter()
        .find(|(name, value)| !valid_identifier(name) || !is_execution_variable(value))
    {
      return Err(ParameterResolutionError::UnsupportedValue(name.clone()));
    }

    let mut resolved = if self.deny_unknown {
      BTreeMap::new()
    } else {
      supplied.clone()
    };
    for (name, definition) in &self.parameters {
      let value = supplied.get(name).or(definition.default.as_ref());
      let Some(value) = value else {
        if definition.required {
          return Err(ParameterResolutionError::Missing(name.clone()));
        }
        continue;
      };
      if !definition.value_type.accepts(value) {
        return Err(ParameterResolutionError::InvalidType(name.clone()));
      }
      resolved.insert(name.clone(), value.clone());
    }
    Ok(resolved)
  }
}

fn is_execution_variable(value: &Value) -> bool {
  matches!(value, Value::String(_) | Value::Bool(_) | Value::Number(_))
}

/// Trigger admission policy captured by one configuration version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationTriggerPolicy {
  /// Trigger origins allowed to target the configuration.
  pub allowed: BTreeSet<TriggerKind>,
}

impl ConfigurationTriggerPolicy {
  fn validate(&self) -> Result<(), StoreInputError> {
    if self.allowed.is_empty() {
      Err(StoreInputError::InvalidBuildConfiguration)
    } else {
      Ok(())
    }
  }
}

/// Placement requirements independent of a concrete Agent implementation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationAgentRequirements {
  /// Execution capabilities every eligible Agent must advertise.
  pub capabilities: BTreeSet<ExecutionCapability>,
  /// Exact normalized inventory labels required for placement.
  pub labels: BTreeMap<String, String>,
  /// Minimum CPU capacity in thousandths of one logical CPU.
  pub minimum_cpu_millis: u32,
  /// Minimum available memory in bytes.
  pub minimum_memory_bytes: u64,
  /// Minimum available workspace bytes.
  pub minimum_disk_bytes: u64,
}

impl ConfigurationAgentRequirements {
  fn validate(&self) -> Result<(), StoreInputError> {
    if self.labels.len() > MAX_AGENT_REQUIREMENT_LABELS
      || self
        .labels
        .iter()
        .any(|(key, value)| !valid_identifier(key) || !valid_text(value, MAX_CONFIGURATION_LABEL_BYTES))
    {
      return Err(StoreInputError::InvalidBuildConfiguration);
    }
    Ok(())
  }
}

/// Network access enforced by the selected execution backend.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConfigurationNetworkPolicy {
  /// Disable external network access.
  Disabled,
  /// Preserve normal network access.
  Unrestricted,
  /// Permit only the listed host names.
  Restricted {
    /// Stable host allowlist in lexical order.
    allowed_hosts: BTreeSet<NetworkHost>,
  },
}

impl ConfigurationNetworkPolicy {
  /// Borrows the canonical host allowlist for restricted policy.
  #[must_use]
  pub const fn allowed_hosts(&self) -> Option<&BTreeSet<NetworkHost>> {
    match self {
      Self::Restricted { allowed_hosts } => Some(allowed_hosts),
      Self::Disabled | Self::Unrestricted => None,
    }
  }

  fn validate(&self) -> Result<(), StoreInputError> {
    if let Self::Restricted { allowed_hosts } = self
      && (allowed_hosts.is_empty() || allowed_hosts.len() > MAX_AGENT_REQUIREMENT_LABELS)
    {
      return Err(StoreInputError::InvalidBuildConfiguration);
    }
    Ok(())
  }
}

/// Complete execution limits selected before JobSpec construction.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationRuntimePolicy {
  /// Required runtime and isolation class.
  pub class: RuntimeClass,
  /// Required operating system.
  pub operating_system: PlatformOs,
  /// Required CPU architecture.
  pub architecture: PlatformArchitecture,
  /// Immutable OCI image required for OCI classes and forbidden for Native.
  pub immutable_image: Option<String>,
  /// CPU execution limit in thousandths of one logical CPU.
  pub cpu_millis: u32,
  /// Memory execution limit in bytes.
  pub memory_bytes: u64,
  /// Writable workspace limit in bytes.
  pub writable_disk_bytes: u64,
  /// Complete Job deadline in seconds.
  pub timeout_seconds: u64,
  /// Network policy enforced by the execution backend.
  pub network: ConfigurationNetworkPolicy,
  /// Optional logical workload-identity profile; never a credential.
  pub workload_identity_profile: Option<String>,
}

impl ConfigurationRuntimePolicy {
  fn validate(&self) -> Result<(), StoreInputError> {
    let image_valid = match self.class {
      RuntimeClass::Native => self.immutable_image.is_none(),
      RuntimeClass::OciProcess | RuntimeClass::OciHypervisor => self
        .immutable_image
        .as_ref()
        .is_some_and(|image| valid_immutable_image(image)),
    };
    if !image_valid
      || self.cpu_millis == 0
      || self.memory_bytes == 0
      || self.writable_disk_bytes == 0
      || self.timeout_seconds == 0
      || self
        .workload_identity_profile
        .as_ref()
        .is_some_and(|profile| !valid_text(profile, MAX_CONFIGURATION_LABEL_BYTES))
    {
      return Err(StoreInputError::InvalidBuildConfiguration);
    }
    self.network.validate()
  }
}

/// Remote-cache authority requested by a Build Configuration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationCachePolicy {
  /// Optional logical namespace; absence disables remote cache.
  pub namespace: Option<String>,
  /// Permit cache reads.
  pub read: bool,
  /// Permit cache publication.
  pub write: bool,
}

impl ConfigurationCachePolicy {
  fn validate(&self) -> Result<(), StoreInputError> {
    let valid = match &self.namespace {
      Some(namespace) => valid_text(namespace, MAX_CONFIGURATION_LABEL_BYTES) && (self.read || self.write),
      None => !self.read && !self.write,
    };
    if valid {
      Ok(())
    } else {
      Err(StoreInputError::InvalidBuildConfiguration)
    }
  }
}

/// Stable terminal failure classes eligible for automatic retry.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryClass {
  /// Repository or build command reported a task failure.
  ExecutionFailure,
  /// Agent, runtime, transport, or storage infrastructure failed.
  InfrastructureFailure,
}

/// Bounded automatic retry policy captured with each Build.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationRetryPolicy {
  /// Maximum Attempts including the initial Attempt.
  pub max_attempts: u16,
  /// Terminal failure classes that may create another Attempt.
  pub retry_on: BTreeSet<RetryClass>,
}

impl ConfigurationRetryPolicy {
  fn validate(&self) -> Result<(), StoreInputError> {
    if self.max_attempts == 0
      || self.max_attempts > MAX_RETRY_ATTEMPTS
      || (self.max_attempts > 1 && self.retry_on.is_empty())
      || (self.max_attempts == 1 && !self.retry_on.is_empty())
    {
      return Err(StoreInputError::InvalidBuildConfiguration);
    }
    Ok(())
  }
}

/// Complete immutable Build Configuration definition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildConfigurationDefinition {
  /// Whether Triggers may create new Builds from this version.
  pub enabled: bool,
  /// Exact immutable Repository version selected by this version.
  pub repository_id: RepositoryId,
  /// Exact immutable Repository version number.
  pub repository_version: RepositoryVersion,
  /// Exact immutable Pipeline selected by this version.
  pub pipeline_id: PipelineId,
  /// Exact immutable Pipeline version number.
  pub pipeline_version: PipelineVersion,
  /// Parameter declarations and defaults.
  pub parameters: ParameterSchema,
  /// Permitted normalized Trigger origins.
  pub triggers: ConfigurationTriggerPolicy,
  /// Placement requirements applied to every materialized Job.
  pub agent_requirements: ConfigurationAgentRequirements,
  /// Configuration-local restriction on eligible Agent Pools.
  pub allowed_pools: BTreeSet<PoolId>,
  /// Runtime, resources, isolation, network, and workload identity.
  pub runtime: ConfigurationRuntimePolicy,
  /// Remote-cache namespace and permissions.
  pub cache: ConfigurationCachePolicy,
  /// Artifact and report ceilings.
  pub artifacts: ArtifactPolicy,
  /// Automatic retry policy.
  pub retry: ConfigurationRetryPolicy,
}

impl BuildConfigurationDefinition {
  /// Revalidates the complete immutable definition at an adapter seam.
  pub fn validate(&self) -> Result<(), StoreInputError> {
    self.parameters.validate()?;
    self.triggers.validate()?;
    self.agent_requirements.validate()?;
    if self.allowed_pools.is_empty() || self.allowed_pools.len() > MAX_CONFIGURATION_POOLS {
      return Err(StoreInputError::InvalidBuildConfiguration);
    }
    self.runtime.validate()?;
    self.cache.validate()?;
    self
      .artifacts
      .validate()
      .map_err(|_| StoreInputError::InvalidBuildConfiguration)?;
    self.retry.validate()?;
    validate_encoded(
      self,
      MAX_BUILD_CONFIGURATION_BYTES,
      StoreInputError::InvalidBuildConfiguration,
    )
  }
}

/// One immutable published Build Configuration version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedBuildConfiguration {
  /// Stable Build Configuration identity shared by all versions.
  pub id: BuildConfigurationId,
  /// Project that owns the identity.
  pub project_id: ProjectId,
  /// Stable sibling-unique name.
  pub name: BuildConfigurationName,
  /// Positive immutable version number.
  pub version: BuildConfigurationVersion,
  /// Complete immutable configuration definition.
  pub definition: BuildConfigurationDefinition,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

/// Atomic request to create a Build Configuration and version one.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CreateBuildConfiguration {
  /// Stable Build Configuration identity selected by the application.
  pub id: BuildConfigurationId,
  /// Existing owning Project.
  pub project_id: ProjectId,
  /// Name unique among Build Configurations in the Project.
  pub name: BuildConfigurationName,
  /// Complete initial immutable definition.
  pub definition: BuildConfigurationDefinition,
  /// Stable replay identity for this command.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

/// Atomic request to append exactly the next Build Configuration version.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PublishBuildConfigurationVersion {
  /// Existing Build Configuration identity.
  pub id: BuildConfigurationId,
  /// Version that must still be current.
  pub expected_current_version: BuildConfigurationVersion,
  /// Complete next immutable definition.
  pub definition: BuildConfigurationDefinition,
  /// Stable replay identity for this command.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

/// Result of one Build Configuration create or publish command.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BuildConfigurationMutationOutcome {
  /// Whether the command was newly applied or exactly replayed.
  pub disposition: MutationDisposition,
  /// Immutable version committed by the original command.
  pub configuration: PublishedBuildConfiguration,
}

fn valid_identifier(value: &str) -> bool {
  let mut characters = value.chars();
  !value.is_empty()
    && value.len() <= 64
    && characters
      .next()
      .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
    && characters.all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

fn valid_text(value: &str, max_bytes: usize) -> bool {
  !value.is_empty() && value.len() <= max_bytes && value.trim() == value && !value.chars().any(char::is_control)
}

fn valid_immutable_image(value: &str) -> bool {
  let Some((name, digest)) = value.rsplit_once("@sha256:") else {
    return false;
  };
  valid_text(name, 1_024)
    && !name.contains(['@', '?', '#', '\\'])
    && !name.contains("://")
    && digest.len() == 64
    && digest
      .bytes()
      .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_encoded<T: Serialize>(value: &T, maximum: usize, error: StoreInputError) -> Result<(), StoreInputError> {
  if serde_json::to_vec(value).is_ok_and(|encoded| encoded.len() <= maximum) {
    Ok(())
  } else {
    Err(error)
  }
}
