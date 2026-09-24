use std::{collections::BTreeMap, num::NonZeroU64};

use octacity_protocol::{CachePolicy, OctaSpec, OutputLimits, RuntimeSpec, SignedEnvelope};
use octacity_server_domain::{
  BuildId, ImmutableRevision, MAX_TIMESTAMP_MILLIS, PipelineNodeId, RepositoryLocator, SourceReference,
};
use octacity_server_secrets::SecretProfileName;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use serde_json::Value;

use super::JobSpecDerivationError;

const MAX_SOURCE_PARAMETER_NAME_BYTES: usize = 64;

/// Greatest validity interval accepted by the signed JobSpec model.
///
/// The bound spans the server's complete positive timestamp vocabulary and
/// guarantees that adding it to any supported issue time cannot overflow the
/// unsigned JobSpec wire timestamp.
pub const MAX_JOB_SPEC_VALIDITY_SECONDS: u64 = MAX_TIMESTAMP_MILLIS as u64 / 1_000;

/// Positive bounded validity interval for one signed JobSpec.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobSpecValidity(NonZeroU64);

impl JobSpecValidity {
  /// Creates a validity interval representable for every server timestamp.
  pub fn new(seconds: u64) -> Result<Self, JobSpecDerivationError> {
    NonZeroU64::new(seconds)
      .filter(|seconds| seconds.get() <= MAX_JOB_SPEC_VALIDITY_SECONDS)
      .map(Self)
      .ok_or(JobSpecDerivationError::InvalidPolicy)
  }

  /// Returns the interval in whole seconds.
  #[must_use]
  pub const fn get(self) -> u64 {
    self.0.get()
  }
}

impl Serialize for JobSpecValidity {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_u64(self.get())
  }
}

impl<'de> Deserialize<'de> for JobSpecValidity {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    Self::new(u64::deserialize(deserializer)?).map_err(D::Error::custom)
  }
}

/// Operator-controlled identity of the source plugin used by one Build.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourcePluginPolicy {
  pub(super) provider: String,
  pub(super) plugin_version: String,
  pub(super) plugin_sha256: String,
  pub(super) repository_parameter: String,
}

impl SourcePluginPolicy {
  /// Constructs an immutable source-plugin policy.
  ///
  /// `repository_parameter` is the provider-defined public parameter receiving
  /// the selected Repository locator, for example `url` for the Git plugin.
  pub fn new(
    provider: impl Into<String>,
    plugin_version: impl Into<String>,
    plugin_sha256: impl Into<String>,
    repository_parameter: impl Into<String>,
  ) -> Result<Self, JobSpecDerivationError> {
    let value = Self {
      provider: provider.into(),
      plugin_version: plugin_version.into(),
      plugin_sha256: plugin_sha256.into(),
      repository_parameter: repository_parameter.into(),
    };
    if !visible(&value.provider, 128)
      || !visible(&value.plugin_version, 128)
      || !sha256(&value.plugin_sha256)
      || !identifier(&value.repository_parameter)
    {
      return Err(JobSpecDerivationError::InvalidPolicy);
    }
    Ok(value)
  }

  /// Revalidates a policy restored from configuration or durable state.
  pub fn validate(&self) -> Result<(), JobSpecDerivationError> {
    Self::new(
      self.provider.clone(),
      self.plugin_version.clone(),
      self.plugin_sha256.clone(),
      self.repository_parameter.clone(),
    )
    .map(|_| ())
  }
}

/// Immutable server policy needed to construct an Agent execution intent.
///
/// None of these protocol values can carry lease fences, transfer targets,
/// host filesystem paths, or raw secret values. Secret and transfer grants are
/// negotiated separately under the current lease.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JobSpecPolicySnapshot {
  pub(super) source: SourcePluginPolicy,
  pub(super) octa: OctaSpec,
  pub(super) runtime: RuntimeSpec,
  pub(super) secrets_profile: Option<SecretProfileName>,
  pub(super) cache: Option<CachePolicy>,
  pub(super) outputs: OutputLimits,
  pub(super) validity: JobSpecValidity,
}

impl JobSpecPolicySnapshot {
  /// Captures the exact policy used for deterministic JobSpec derivation.
  pub fn new(
    source: SourcePluginPolicy,
    octa: OctaSpec,
    runtime: RuntimeSpec,
    secrets_profile: Option<SecretProfileName>,
    cache: Option<CachePolicy>,
    outputs: OutputLimits,
    validity: JobSpecValidity,
  ) -> Result<Self, JobSpecDerivationError> {
    if outputs.validate().is_err() || cache.as_ref().is_some_and(|value| value.validate().is_err()) {
      return Err(JobSpecDerivationError::InvalidPolicy);
    }
    Ok(Self {
      source,
      octa,
      runtime,
      secrets_profile,
      cache,
      outputs,
      validity,
    })
  }
}

/// Immutable Build facts consumed by JobSpec derivation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobSpecBuildSnapshot {
  pub(super) build_id: BuildId,
  pub(super) immutable_revision: ImmutableRevision,
  pub(super) source_reference: Option<SourceReference>,
  pub(super) repository_locator: RepositoryLocator,
  pub(super) parameters: BTreeMap<String, Value>,
}

impl JobSpecBuildSnapshot {
  /// Captures source and parameter facts already validated during Build acceptance.
  pub fn new(
    build_id: BuildId,
    immutable_revision: ImmutableRevision,
    source_reference: Option<SourceReference>,
    repository_locator: RepositoryLocator,
    parameters: BTreeMap<String, Value>,
  ) -> Result<Self, JobSpecDerivationError> {
    Ok(Self {
      build_id,
      immutable_revision,
      source_reference,
      repository_locator,
      parameters,
    })
  }
}

/// Stable server-owned execution intent awaiting a concrete ready transition.
///
/// The template contains every immutable Build, Pipeline, and policy input but
/// deliberately excludes Attempt identity, Job identity, and validity times.
/// It is therefore safe to persist for blocked Jobs and to reuse for a retry;
/// a short-lived signed envelope is derived only when the Job becomes ready.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JobSpecTemplate {
  build_id: BuildId,
  pipeline_node_id: PipelineNodeId,
  immutable_revision: ImmutableRevision,
  source_reference: Option<SourceReference>,
  repository_locator: RepositoryLocator,
  parameters: BTreeMap<String, Value>,
  execution: JobExecutionTemplate,
  policy: JobSpecPolicySnapshot,
}

/// Immutable toolchain and runtime facts used before a ready Job is leased.
///
/// The view deliberately omits repository parameters and other execution
/// payload. Placement needs exact installed identities, while the signed
/// [`octacity_protocol::JobSpecV1`] remains the execution authority.
#[derive(Clone, Copy, Debug)]
pub struct JobPlacementPolicy<'a> {
  /// Logical source-plugin identity.
  pub source_provider: &'a str,
  /// Exact source-plugin package version.
  pub source_plugin_version: &'a str,
  /// Exact source-plugin executable digest.
  pub source_plugin_sha256: &'a str,
  /// Exact Octa and task-plugin requirements.
  pub octa: &'a octacity_protocol::OctaSpec,
  /// Exact runtime, isolation, platform, and resource policy.
  pub runtime: &'a octacity_protocol::RuntimeSpec,
  /// Whether this Job requires the registered Octa cache capability.
  pub requires_cache: bool,
}

impl JobSpecTemplate {
  pub(super) fn new(
    build: &JobSpecBuildSnapshot,
    pipeline_node_id: PipelineNodeId,
    execution: JobExecutionTemplate,
    policy: &JobSpecPolicySnapshot,
  ) -> Result<Self, JobSpecDerivationError> {
    let value = Self {
      build_id: build.build_id,
      pipeline_node_id,
      immutable_revision: build.immutable_revision.clone(),
      source_reference: build.source_reference.clone(),
      repository_locator: build.repository_locator.clone(),
      parameters: build.parameters.clone(),
      execution,
      policy: policy.clone(),
    };
    value.validate()?;
    Ok(value)
  }

  /// Returns the Build whose immutable snapshots produced this template.
  #[must_use]
  pub const fn build_id(&self) -> BuildId {
    self.build_id
  }

  /// Borrows the immutable Pipeline node represented by this template.
  #[must_use]
  pub const fn pipeline_node_id(&self) -> &PipelineNodeId {
    &self.pipeline_node_id
  }

  /// Revalidates a template decoded at a persistence or API boundary.
  pub fn validate(&self) -> Result<(), JobSpecDerivationError> {
    if self.policy.source.validate().is_err()
      || self.policy.outputs.validate().is_err()
      || self
        .policy
        .cache
        .as_ref()
        .is_some_and(|cache| cache.validate().is_err())
    {
      return Err(JobSpecDerivationError::InvalidPolicy);
    }
    super::derive::validate_execution_template(&self.execution)?;
    super::derive::execution_variables(&self.parameters)?;
    super::derive::validate_protocol_template(self)
  }

  /// Borrows the exact facts required for scheduler compatibility checks.
  #[must_use]
  pub fn placement_policy(&self) -> JobPlacementPolicy<'_> {
    JobPlacementPolicy {
      source_provider: &self.policy.source.provider,
      source_plugin_version: &self.policy.source.plugin_version,
      source_plugin_sha256: &self.policy.source.plugin_sha256,
      octa: &self.policy.octa,
      runtime: &self.policy.runtime,
      requires_cache: self.policy.cache.is_some(),
    }
  }

  /// Borrows the exact signed cache policy, when this Job may use remote cache.
  #[must_use]
  pub fn cache_policy(&self) -> Option<&CachePolicy> {
    self.policy.cache.as_ref()
  }

  pub(super) const fn execution(&self) -> &JobExecutionTemplate {
    &self.execution
  }

  pub(super) const fn policy(&self) -> &JobSpecPolicySnapshot {
    &self.policy
  }

  pub(super) fn immutable_revision(&self) -> &ImmutableRevision {
    &self.immutable_revision
  }

  pub(super) fn source_reference(&self) -> Option<&SourceReference> {
    self.source_reference.as_ref()
  }

  pub(super) fn repository_locator(&self) -> &str {
    self.repository_locator.as_str()
  }

  pub(super) fn parameters(&self) -> &BTreeMap<String, Value> {
    &self.parameters
  }
}

/// Repository-controlled execution fields permitted inside a Pipeline node.
///
/// Strict decoding makes server-owned fields such as secret profiles, host
/// paths, fences, transfer targets, and pre-signed payloads unrepresentable.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JobExecutionTemplate {
  /// Optional workspace-relative Octafile path.
  #[serde(default)]
  pub octafile: Option<String>,
  /// Non-empty Octa task names to execute.
  pub commands: Vec<String>,
  /// Positional values supplied to runtime task templates.
  #[serde(default)]
  pub arguments: Vec<String>,
  /// Optional maximum task concurrency.
  #[serde(default)]
  pub concurrency: Option<std::num::NonZeroUsize>,
  /// Whether independent root commands may run concurrently.
  #[serde(default)]
  pub parallel: bool,
  /// Whether the runner stops scheduling after the first failure.
  #[serde(default)]
  pub failfast: bool,
}

/// A signed JobSpec produced only from validated server-owned intent.
///
/// This proof type has no constructor accepting a `SignedEnvelope`; callers
/// must use [`super::sign_ready_job_spec`]. Serialization deliberately emits
/// only the shared wire envelope for persistence and later lease delivery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DerivedJobSpec {
  pub(super) envelope: SignedEnvelope,
}

impl DerivedJobSpec {
  /// Borrows the exact signed wire envelope persisted for lease delivery.
  #[must_use]
  pub const fn envelope(&self) -> &SignedEnvelope {
    &self.envelope
  }
}

impl Serialize for DerivedJobSpec {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    self.envelope.serialize(serializer)
  }
}

fn visible(value: &str, maximum: usize) -> bool {
  !value.is_empty() && value.len() <= maximum && value.trim() == value && !value.chars().any(char::is_control)
}

fn identifier(value: &str) -> bool {
  let mut characters = value.chars();
  !value.is_empty()
    && value.len() <= MAX_SOURCE_PARAMETER_NAME_BYTES
    && characters
      .next()
      .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
    && characters.all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

fn sha256(value: &str) -> bool {
  value.len() == 64
    && value
      .bytes()
      .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
