//! Versioned wire types shared by the OctaCity agent and server.
//!
//! This crate deliberately contains no HTTP, persistence, scheduler, or agent
//! implementation details. Signed payload validation is kept here so both ends
//! agree on one canonical security boundary before a job reaches an agent.
//!
//! The language-neutral wire specification is documented in
//! [Signed JobSpec protocol v1].
//!
//! [Signed JobSpec protocol v1]: https://github.com/OctaHive/OctaCity/blob/main/docs/protocols/signed-job-spec-v1.md

use std::{collections::BTreeMap, num::NonZeroUsize};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;

mod coordinator;

pub use coordinator::*;

/// Signed JobSpec wire version supported by this crate.
pub const AGENT_PROTOCOL_VERSION: u16 = 1;
/// Exact signature algorithm identifier accepted in a v1 envelope.
pub const SIGNATURE_ALGORITHM: &str = "ed25519";
/// Maximum decoded size of an authenticated JobSpec JSON payload.
pub const MAX_SIGNED_JOB_SPEC_BYTES: usize = 1024 * 1024;
const MAX_ENCODED_JOB_SPEC_BYTES: usize = MAX_SIGNED_JOB_SPEC_BYTES.div_ceil(3) * 4;
const ED25519_SIGNATURE_BYTES: usize = 64;
const MAX_ENCODED_SIGNATURE_BYTES: usize = ED25519_SIGNATURE_BYTES.div_ceil(3) * 4;

/// Detached signature and encoded canonical job payload received by an agent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SignedEnvelope {
  /// Identifier of an agent-configured server verification key.
  pub key_id: String,
  /// Signature algorithm; v1 accepts only [`SIGNATURE_ALGORITHM`].
  pub algorithm: String,
  /// Standard-base64 encoding of the exact signed JobSpec JSON bytes.
  pub payload: String,
  /// Standard-base64 encoding of the Ed25519 signature over `payload` bytes.
  pub signature: String,
}

/// Immutable job description covered by the server signature.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JobSpecV1 {
  /// Wire version; must equal [`AGENT_PROTOCOL_VERSION`].
  pub protocol_version: u16,
  /// Stable identity of the job bound to the surrounding lease.
  pub job_id: String,
  /// Positive execution attempt bound to the surrounding lease.
  pub attempt: u32,
  /// First Unix second in which this specification is valid.
  pub issued_at: u64,
  /// First Unix second in which this specification is no longer valid.
  pub expires_at: u64,
  /// Exact source provider and immutable revision to materialize.
  pub source: SourceSpec,
  /// Exact Octa release and plugin set authorized to run.
  pub octa: OctaSpec,
  /// Octafile tasks and values passed to the runner.
  pub execution: ExecutionSpec,
  /// Platform, isolation, resources, and network policy.
  pub runtime: RuntimeSpec,
  /// Optional logical cache authority; transport credentials remain out of band.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub cache: Option<CachePolicy>,
  /// Upper bounds for outputs accepted from the job.
  pub outputs: OutputLimits,
}

/// Repository-controlled cache permissions covered by the JobSpec signature.
///
/// The server supplies storage location and credentials separately under the
/// active lease fence. A repository can therefore opt out or narrow access,
/// but it cannot select an endpoint or enlarge operator policy.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CachePolicy {
  /// Server-authorized logical action namespace.
  pub namespace: String,
  /// Permit action and blob lookup.
  pub read: bool,
  /// Permit publication of successful action results.
  pub write: bool,
}

impl CachePolicy {
  /// Validates the semantic scope and requires at least one useful permission.
  pub fn validate(&self) -> Result<(), String> {
    octa_cache_protocol::validate_namespace(&self.namespace).map_err(|error| error.to_string())?;
    if !self.read && !self.write {
      return Err("cache policy must allow reading, writing, or both".to_owned());
    }
    Ok(())
  }

  /// Converts independent permissions to Octa's cache access mode.
  ///
  /// This method remains fallible because wire DTOs can be constructed by
  /// callers without first invoking [`Self::validate`].
  pub fn mode(&self) -> Result<octa_cache_protocol::CacheMode, String> {
    self.validate()?;
    Ok(match (self.read, self.write) {
      (true, true) => octa_cache_protocol::CacheMode::ReadWrite,
      (true, false) => octa_cache_protocol::CacheMode::ReadOnly,
      (false, true) => octa_cache_protocol::CacheMode::WriteOnly,
      (false, false) => return Err("cache policy must allow reading, writing, or both".to_owned()),
    })
  }
}

/// Exact source-plugin and revision requirement for a job.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSpec {
  /// Logical source-plugin name resolved by the agent registry.
  pub provider: String,
  /// Exact source-plugin package version.
  pub plugin_version: String,
  /// SHA-256 digest of the installed source-plugin executable.
  pub plugin_sha256: String,
  /// Immutable provider revision that must be materialized.
  pub revision: String,
  /// Optional mutable lookup hint, never accepted as the final revision.
  #[serde(default)]
  pub reference: Option<String>,
  /// Provider-specific values interpreted by the selected plugin.
  #[serde(default)]
  pub parameters: BTreeMap<String, serde_json::Value>,
}

/// Exact Octa release and plugin set authorized to execute a job.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OctaSpec {
  /// Exact Octa release version.
  pub version: String,
  /// SHA-256 digest of the `octa-runner` executable.
  pub runner_sha256: String,
  /// Required runner process-protocol version.
  pub runner_protocol: u16,
  /// Required structured event schema version.
  pub event_schema: u16,
  /// Required Octa task-plugin protocol version.
  pub plugin_protocol: u16,
  /// Authorized task plugins indexed by name and SHA-256 digest.
  #[serde(default)]
  pub plugin_digests: BTreeMap<String, String>,
}

/// Commands and runtime values passed to `octa-runner` after checkout.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionSpec {
  /// Optional workspace-relative Octafile path.
  #[serde(default)]
  pub octafile: Option<String>,
  /// Non-empty task names to execute.
  pub commands: Vec<String>,
  /// Explicit Octafile variable overrides.
  #[serde(default)]
  pub variables: BTreeMap<String, String>,
  /// Positional values supplied to runtime task templates.
  #[serde(default)]
  pub arguments: Vec<String>,
  /// Optional maximum task concurrency.
  #[serde(default)]
  pub concurrency: Option<NonZeroUsize>,
  /// Whether independent root commands may run concurrently.
  #[serde(default)]
  pub parallel: bool,
  /// Whether the runner stops scheduling after the first failure.
  #[serde(default)]
  pub failfast: bool,
  /// Optional workspace-relative secrets profile selected for this run.
  #[serde(default)]
  pub secrets_profile: Option<String>,
}

/// Top-level execution mode selected by the server and allowed by the agent.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMode {
  /// Execute against the agent host platform.
  Native,
  /// Execute an immutable OCI image.
  Oci,
}

/// Guest operating system required by a Native or OCI execution.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformOs {
  /// Linux platform.
  Linux,
  /// Windows platform.
  Windows,
  /// macOS platform.
  Macos,
}

/// Guest CPU architecture using OCI platform names on the wire.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformArchitecture {
  /// 64-bit x86 (`amd64`).
  Amd64,
  /// 64-bit ARM (`arm64`).
  Arm64,
}

/// Exact operating-system and architecture requirement for an execution.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformSpec {
  /// Required operating system.
  pub os: PlatformOs,
  /// Required CPU architecture.
  pub architecture: PlatformArchitecture,
}

/// Isolation boundary required around an OCI image.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OciIsolation {
  /// Share the agent kernel behind OCI namespaces and cgroups.
  Process,
  /// Run with a separate guest kernel behind a hypervisor boundary.
  Hypervisor,
}

/// Environment in which the released runner must execute.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuntimeTarget {
  /// Execute directly against a matching agent host.
  Native {
    /// Exact host platform required by the signed job.
    platform: PlatformSpec,
  },
  /// Execute an immutable OCI image using the requested isolation tier.
  Oci {
    /// Exact guest platform.
    platform: PlatformSpec,
    /// Minimum isolation tier that must be enforced.
    isolation: OciIsolation,
    /// Immutable `repository@sha256:digest` image reference.
    image: String,
  },
}

/// Resource and network boundaries enforced around one runner process.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeSpec {
  /// Native host or OCI guest selected for the execution.
  pub target: RuntimeTarget,
  /// CPU allocation in thousandths of one logical CPU.
  pub cpu_millis: u32,
  /// Maximum memory in bytes.
  pub memory_bytes: u64,
  /// Maximum writable workspace capacity in bytes.
  pub writable_disk_bytes: u64,
  /// Complete preparation and execution deadline in seconds.
  pub timeout_seconds: u64,
  /// Network access the selected backend must enforce.
  pub network: NetworkPolicy,
  /// Optional operator-provisioned workload identity profile.
  #[serde(default)]
  pub workload_identity_profile: Option<String>,
}

/// Network access granted to the job runtime.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum NetworkPolicy {
  /// Preserve normal network access inside the execution.
  Unrestricted,
  /// Disable external network access.
  Disabled,
  /// Permit connections only to explicit hosts.
  Restricted {
    /// Non-empty host names authorized by the server.
    allowed_hosts: Vec<String>,
  },
}

/// Bounds for report and artifact data returned by a job.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OutputLimits {
  /// Maximum number of registered artifacts.
  pub artifact_count: u32,
  /// Maximum aggregate artifact bytes.
  pub artifact_bytes: u64,
  /// Maximum number of registered reports.
  pub report_count: u32,
  /// Maximum aggregate report bytes.
  pub report_bytes: u64,
  /// Maximum bytes in one uploaded file or deterministic directory archive.
  pub single_output_bytes: u64,
}

/// Lease identity supplied out of band and bound to the signed payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobBinding<'a> {
  /// Job identity from the lease transport.
  pub job_id: &'a str,
  /// Attempt number from the lease transport.
  pub attempt: u32,
  /// Current Unix time used for validity checks.
  pub now: u64,
}

/// Authentication, decoding, or validation failure for a signed JobSpec.
#[derive(Debug, Error)]
pub enum JobSpecError {
  /// Envelope selected an unsupported signature algorithm.
  #[error("unsupported signature algorithm '{0}'")]
  Algorithm(String),
  /// Envelope refers to a key absent from agent configuration.
  #[error("unknown server signing key '{0}'")]
  UnknownKey(String),
  /// Payload is not valid standard base64.
  #[error("signed JobSpec payload is not valid base64: {0}")]
  PayloadEncoding(#[source] base64::DecodeError),
  /// Encoded or decoded payload exceeds the protocol limit.
  #[error("signed JobSpec payload exceeds the {MAX_SIGNED_JOB_SPEC_BYTES}-byte limit")]
  PayloadTooLarge,
  /// Signature is not valid standard base64.
  #[error("JobSpec signature is not valid base64: {0}")]
  SignatureEncoding(#[source] base64::DecodeError),
  /// Decoded signature length is not valid for Ed25519.
  #[error("JobSpec signature has an invalid length")]
  SignatureLength,
  /// Signature does not authenticate the exact payload bytes.
  #[error("JobSpec signature verification failed")]
  InvalidSignature,
  /// Authenticated payload is not a valid JobSpec JSON document.
  #[error("signed JobSpec is not valid JSON: {0}")]
  Json(#[source] serde_json::Error),
  /// Authenticated fields violate a JobSpec or lease invariant.
  #[error("invalid JobSpec: {0}")]
  Validation(String),
}

/// Verifies the signature over the exact payload bytes, then decodes and
/// validates the job and its lease binding.
pub fn verify_job_spec(
  envelope: &SignedEnvelope,
  keys: &BTreeMap<String, VerifyingKey>,
  binding: JobBinding<'_>,
) -> Result<JobSpecV1, JobSpecError> {
  if envelope.algorithm != SIGNATURE_ALGORITHM {
    return Err(JobSpecError::Algorithm(envelope.algorithm.clone()));
  }
  let key = keys
    .get(&envelope.key_id)
    .ok_or_else(|| JobSpecError::UnknownKey(envelope.key_id.clone()))?;
  // Reject by encoded length before decoding so the advertised payload bound
  // also bounds attacker-controlled allocation.
  if envelope.payload.len() > MAX_ENCODED_JOB_SPEC_BYTES {
    return Err(JobSpecError::PayloadTooLarge);
  }
  let payload = BASE64
    .decode(&envelope.payload)
    .map_err(JobSpecError::PayloadEncoding)?;
  if payload.len() > MAX_SIGNED_JOB_SPEC_BYTES {
    return Err(JobSpecError::PayloadTooLarge);
  }
  if envelope.signature.len() > MAX_ENCODED_SIGNATURE_BYTES {
    return Err(JobSpecError::SignatureLength);
  }
  let signature_bytes = BASE64
    .decode(&envelope.signature)
    .map_err(JobSpecError::SignatureEncoding)?;
  let signature = Signature::from_slice(&signature_bytes).map_err(|_| JobSpecError::SignatureLength)?;
  key
    .verify_strict(&payload, &signature)
    .map_err(|_| JobSpecError::InvalidSignature)?;

  // Parsing only authenticated bytes avoids acting on fields that were not
  // covered by the server signature.
  let spec: JobSpecV1 = serde_json::from_slice(&payload).map_err(JobSpecError::Json)?;
  spec.validate(&binding).map_err(JobSpecError::Validation)?;
  Ok(spec)
}

impl JobSpecV1 {
  /// Validates the authenticated job against its lease identity and current time.
  pub fn validate(&self, binding: &JobBinding<'_>) -> Result<(), String> {
    if self.protocol_version != AGENT_PROTOCOL_VERSION {
      return Err(format!("unsupported protocol version {}", self.protocol_version));
    }
    non_empty("job_id", &self.job_id)?;
    if self.job_id != binding.job_id || self.attempt != binding.attempt {
      return Err("JobSpec does not match its lease binding".to_owned());
    }
    if self.attempt == 0 {
      return Err("attempt must be greater than zero".to_owned());
    }
    if self.issued_at >= self.expires_at {
      return Err("expires_at must be later than issued_at".to_owned());
    }
    if binding.now < self.issued_at || binding.now >= self.expires_at {
      return Err("JobSpec is not valid at the current time".to_owned());
    }
    self.source.validate()?;
    self.octa.validate()?;
    self.execution.validate()?;
    self.runtime.validate()?;
    if let Some(cache) = &self.cache {
      cache.validate()?;
    }
    self.outputs.validate()
  }
}

impl SourceSpec {
  fn validate(&self) -> Result<(), String> {
    non_empty("source.provider", &self.provider)?;
    non_empty("source.plugin_version", &self.plugin_version)?;
    sha256("source.plugin_sha256", &self.plugin_sha256)?;
    non_empty("source.revision", &self.revision)?;
    if let Some(reference) = &self.reference {
      non_empty("source.reference", reference)?;
    }
    Ok(())
  }
}

impl OctaSpec {
  fn validate(&self) -> Result<(), String> {
    non_empty("octa.version", &self.version)?;
    sha256("octa.runner_sha256", &self.runner_sha256)?;
    if self.runner_protocol == 0 || self.event_schema == 0 || self.plugin_protocol == 0 {
      return Err("Octa protocol versions must be greater than zero".to_owned());
    }
    for (plugin, digest) in &self.plugin_digests {
      non_empty("octa.plugin_digests name", plugin)?;
      sha256("octa.plugin_digests value", digest)?;
    }
    Ok(())
  }
}

impl ExecutionSpec {
  fn validate(&self) -> Result<(), String> {
    if self.commands.is_empty() || self.commands.iter().any(|command| command.is_empty()) {
      return Err("execution.commands must contain non-empty task names".to_owned());
    }
    for (name, path) in [
      ("execution.octafile", self.octafile.as_deref()),
      ("execution.secrets_profile", self.secrets_profile.as_deref()),
    ] {
      if let Some(path) = path {
        relative_wire_path(name, path)?;
      }
    }
    Ok(())
  }
}

impl RuntimeSpec {
  /// Returns the top-level backend key without exposing engine details.
  pub const fn mode(&self) -> RuntimeMode {
    match &self.target {
      RuntimeTarget::Native { .. } => RuntimeMode::Native,
      RuntimeTarget::Oci { .. } => RuntimeMode::Oci,
    }
  }

  /// Returns the exact host or guest platform required by the job.
  pub const fn platform(&self) -> PlatformSpec {
    match &self.target {
      RuntimeTarget::Native { platform } | RuntimeTarget::Oci { platform, .. } => *platform,
    }
  }

  fn validate(&self) -> Result<(), String> {
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
    match &self.target {
      RuntimeTarget::Native { .. } => Ok(()),
      RuntimeTarget::Oci { platform, image, .. } => {
        if platform.os == PlatformOs::Macos {
          return Err("OCI execution does not support a macOS guest platform".to_owned());
        }
        immutable_oci_reference("runtime.target.image", image)
      }
    }
  }
}

impl OutputLimits {
  /// Validates the internally consistent wire shape of one output quota.
  ///
  /// This does not apply deployment policy. Agents and servers must separately
  /// compare a valid signed quota with their own configured maxima.
  pub fn validate(&self) -> Result<(), String> {
    if (self.artifact_count == 0) != (self.artifact_bytes == 0) {
      return Err("artifact count and byte limits must both be zero or both be greater than zero".to_owned());
    }
    if (self.report_count == 0) != (self.report_bytes == 0) {
      return Err("report count and byte limits must both be zero or both be greater than zero".to_owned());
    }
    let outputs_enabled = self.artifact_count > 0 || self.report_count > 0;
    if outputs_enabled != (self.single_output_bytes > 0) {
      return Err("single_output_bytes must be positive exactly when outputs are enabled".to_owned());
    }
    if (self.artifact_count > 0 && self.single_output_bytes > self.artifact_bytes)
      || (self.report_count > 0 && self.single_output_bytes > self.report_bytes)
    {
      return Err("single_output_bytes must not exceed an enabled aggregate output byte limit".to_owned());
    }
    Ok(())
  }

  /// Returns whether every requested dimension fits within `maximum`.
  ///
  /// The comparison is deliberately component-wise: aggregate byte limits do
  /// not compensate for excessive counts or an excessive single-output limit.
  pub fn is_within(&self, maximum: &Self) -> bool {
    self.artifact_count <= maximum.artifact_count
      && self.artifact_bytes <= maximum.artifact_bytes
      && self.report_count <= maximum.report_count
      && self.report_bytes <= maximum.report_bytes
      && self.single_output_bytes <= maximum.single_output_bytes
  }
}

fn non_empty(name: &str, value: &str) -> Result<(), String> {
  if value.trim().is_empty() {
    Err(format!("{name} must not be empty"))
  } else {
    Ok(())
  }
}

fn sha256(name: &str, value: &str) -> Result<(), String> {
  if value.len() == 64
    && value
      .bytes()
      .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
  {
    Ok(())
  } else {
    Err(format!("{name} must be a 64-character hexadecimal SHA-256 digest"))
  }
}

fn immutable_oci_reference(name: &str, value: &str) -> Result<(), String> {
  let Some((repository, digest)) = value.rsplit_once("@sha256:") else {
    return Err(format!(
      "{name} must be an immutable OCI reference ending in @sha256:<digest>"
    ));
  };
  if repository.is_empty()
    || repository.contains('@')
    || repository.contains("://")
    || repository.chars().any(char::is_whitespace)
  {
    return Err(format!("{name} contains an invalid OCI repository reference"));
  }
  sha256(name, digest)
}

/// Validates a platform-independent path represented with `/` separators.
fn relative_wire_path(name: &str, value: &str) -> Result<(), String> {
  if value.is_empty()
    || value.starts_with('/')
    || value.contains('\\')
    || value.contains(':')
    || value.chars().any(char::is_control)
    || value
      .split('/')
      .any(|part| part.is_empty() || part == "." || part == "..")
  {
    return Err(format!("{name} must be a normalized relative '/'-separated path"));
  }
  Ok(())
}

#[cfg(test)]
mod tests;
