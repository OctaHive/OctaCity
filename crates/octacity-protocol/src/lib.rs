//! Versioned wire types shared by the OctaCity agent and server.
//!
//! This crate deliberately contains no HTTP, persistence, scheduler, or agent
//! implementation details.

use std::{collections::BTreeMap, num::NonZeroUsize};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const AGENT_PROTOCOL_VERSION: u16 = 1;
pub const SIGNATURE_ALGORITHM: &str = "ed25519";
pub const MAX_SIGNED_JOB_SPEC_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SignedEnvelope {
  pub key_id: String,
  pub algorithm: String,
  pub payload: String,
  pub signature: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JobSpecV1 {
  pub protocol_version: u16,
  pub job_id: String,
  pub attempt: u32,
  pub issued_at: u64,
  pub expires_at: u64,
  pub source: SourceSpec,
  pub octa: OctaSpec,
  pub execution: ExecutionSpec,
  pub runtime: RuntimeSpec,
  pub outputs: OutputLimits,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSpec {
  pub provider: String,
  pub plugin_version: String,
  pub plugin_sha256: String,
  pub revision: String,
  #[serde(default)]
  pub reference: Option<String>,
  #[serde(default)]
  pub parameters: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OctaSpec {
  pub version: String,
  pub runner_sha256: String,
  pub runner_protocol: u16,
  pub event_schema: u16,
  pub plugin_protocol: u16,
  #[serde(default)]
  pub plugin_digests: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionSpec {
  #[serde(default)]
  pub octafile: Option<String>,
  pub commands: Vec<String>,
  #[serde(default)]
  pub variables: BTreeMap<String, String>,
  #[serde(default)]
  pub arguments: Vec<String>,
  #[serde(default)]
  pub concurrency: Option<NonZeroUsize>,
  #[serde(default)]
  pub parallel: bool,
  #[serde(default)]
  pub failfast: bool,
  #[serde(default)]
  pub secrets_profile: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
  Native,
  Microsandbox,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeSpec {
  pub backend: BackendKind,
  #[serde(default)]
  pub image: Option<String>,
  pub cpu_millis: u32,
  pub memory_bytes: u64,
  pub writable_disk_bytes: u64,
  pub timeout_seconds: u64,
  pub network: NetworkPolicy,
  pub workload_identity_profile: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum NetworkPolicy {
  Disabled,
  Restricted { allowed_hosts: Vec<String> },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OutputLimits {
  pub artifact_count: u32,
  pub artifact_bytes: u64,
  pub report_count: u32,
  pub report_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobBinding<'a> {
  pub job_id: &'a str,
  pub attempt: u32,
  pub now: u64,
}

#[derive(Debug, Error)]
pub enum JobSpecError {
  #[error("unsupported signature algorithm '{0}'")]
  Algorithm(String),
  #[error("unknown server signing key '{0}'")]
  UnknownKey(String),
  #[error("signed JobSpec payload is not valid base64: {0}")]
  PayloadEncoding(#[source] base64::DecodeError),
  #[error("signed JobSpec payload exceeds the {MAX_SIGNED_JOB_SPEC_BYTES}-byte limit")]
  PayloadTooLarge,
  #[error("JobSpec signature is not valid base64: {0}")]
  SignatureEncoding(#[source] base64::DecodeError),
  #[error("JobSpec signature has an invalid length")]
  SignatureLength,
  #[error("JobSpec signature verification failed")]
  InvalidSignature,
  #[error("signed JobSpec is not valid JSON: {0}")]
  Json(#[source] serde_json::Error),
  #[error("invalid JobSpec: {0}")]
  Validation(String),
}

/// Verifies the exact payload bytes before deserializing and validating them.
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
  let payload = BASE64
    .decode(&envelope.payload)
    .map_err(JobSpecError::PayloadEncoding)?;
  if payload.len() > MAX_SIGNED_JOB_SPEC_BYTES {
    return Err(JobSpecError::PayloadTooLarge);
  }
  let signature_bytes = BASE64
    .decode(&envelope.signature)
    .map_err(JobSpecError::SignatureEncoding)?;
  let signature = Signature::from_slice(&signature_bytes).map_err(|_| JobSpecError::SignatureLength)?;
  key
    .verify_strict(&payload, &signature)
    .map_err(|_| JobSpecError::InvalidSignature)?;

  let spec: JobSpecV1 = serde_json::from_slice(&payload).map_err(JobSpecError::Json)?;
  spec.validate(&binding).map_err(JobSpecError::Validation)?;
  Ok(spec)
}

impl JobSpecV1 {
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
  fn validate(&self) -> Result<(), String> {
    if self.cpu_millis == 0 || self.memory_bytes == 0 || self.writable_disk_bytes == 0 || self.timeout_seconds == 0 {
      return Err("runtime limits must be greater than zero".to_owned());
    }
    non_empty("runtime.workload_identity_profile", &self.workload_identity_profile)?;
    if let NetworkPolicy::Restricted { allowed_hosts } = &self.network
      && (allowed_hosts.is_empty() || allowed_hosts.iter().any(|host| host.trim().is_empty()))
    {
      return Err("a restricted network policy requires non-empty allowed_hosts".to_owned());
    }
    match (self.backend, self.image.as_deref()) {
      (BackendKind::Native, None) => Ok(()),
      (BackendKind::Native, Some(_)) => Err("runtime.image is not allowed for native execution".to_owned()),
      (BackendKind::Microsandbox, Some(image)) if image.starts_with("sha256:") => sha256("runtime.image", &image[7..]),
      (BackendKind::Microsandbox, _) => {
        Err("microsandbox execution requires an immutable sha256 image digest".to_owned())
      }
    }
  }
}

impl OutputLimits {
  fn validate(&self) -> Result<(), String> {
    if (self.artifact_count == 0) != (self.artifact_bytes == 0) {
      return Err("artifact count and byte limits must both be zero or both be greater than zero".to_owned());
    }
    if (self.report_count == 0) != (self.report_bytes == 0) {
      return Err("report count and byte limits must both be zero or both be greater than zero".to_owned());
    }
    Ok(())
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
mod tests {
  use super::*;
  use ed25519_dalek::{Signer as _, SigningKey};

  const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

  fn spec() -> JobSpecV1 {
    JobSpecV1 {
      protocol_version: AGENT_PROTOCOL_VERSION,
      job_id: "job-1".to_owned(),
      attempt: 1,
      issued_at: 100,
      expires_at: 200,
      source: SourceSpec {
        provider: "git".to_owned(),
        plugin_version: "0.1.0".to_owned(),
        plugin_sha256: DIGEST.to_owned(),
        revision: "abc123".to_owned(),
        reference: None,
        parameters: BTreeMap::new(),
      },
      octa: OctaSpec {
        version: "0.3.0".to_owned(),
        runner_sha256: DIGEST.to_owned(),
        runner_protocol: 1,
        event_schema: 1,
        plugin_protocol: 1,
        plugin_digests: BTreeMap::new(),
      },
      execution: ExecutionSpec {
        octafile: Some("ci/Octafile.yml".to_owned()),
        commands: vec!["test".to_owned()],
        variables: BTreeMap::new(),
        arguments: Vec::new(),
        concurrency: NonZeroUsize::new(2),
        parallel: true,
        failfast: true,
        secrets_profile: Some("ci/secrets.yml".to_owned()),
      },
      runtime: RuntimeSpec {
        backend: BackendKind::Microsandbox,
        image: Some(format!("sha256:{DIGEST}")),
        cpu_millis: 1000,
        memory_bytes: 512 * 1024 * 1024,
        writable_disk_bytes: 1024 * 1024 * 1024,
        timeout_seconds: 600,
        network: NetworkPolicy::Disabled,
        workload_identity_profile: "ci".to_owned(),
      },
      outputs: OutputLimits {
        artifact_count: 10,
        artifact_bytes: 1024,
        report_count: 10,
        report_bytes: 1024,
      },
    }
  }

  fn envelope(spec: &JobSpecV1, signing_key: &SigningKey) -> SignedEnvelope {
    let payload = serde_json::to_vec(spec).unwrap();
    SignedEnvelope {
      key_id: "test-key".to_owned(),
      algorithm: SIGNATURE_ALGORITHM.to_owned(),
      payload: BASE64.encode(&payload),
      signature: BASE64.encode(signing_key.sign(&payload).to_bytes()),
    }
  }

  #[test]
  fn verifies_exact_signed_payload_and_lease_binding() {
    let signing_key = SigningKey::from_bytes(&[7; 32]);
    let keys = BTreeMap::from([("test-key".to_owned(), signing_key.verifying_key())]);
    let verified = verify_job_spec(
      &envelope(&spec(), &signing_key),
      &keys,
      JobBinding {
        job_id: "job-1",
        attempt: 1,
        now: 150,
      },
    )
    .unwrap();
    assert_eq!(verified, spec());
  }

  #[test]
  fn rejects_a_payload_changed_after_signing() {
    let signing_key = SigningKey::from_bytes(&[7; 32]);
    let keys = BTreeMap::from([("test-key".to_owned(), signing_key.verifying_key())]);
    let mut envelope = envelope(&spec(), &signing_key);
    let mut payload = BASE64.decode(&envelope.payload).unwrap();
    payload.push(b' ');
    envelope.payload = BASE64.encode(payload);

    assert!(matches!(
      verify_job_spec(
        &envelope,
        &keys,
        JobBinding {
          job_id: "job-1",
          attempt: 1,
          now: 150
        }
      ),
      Err(JobSpecError::InvalidSignature)
    ));
  }

  #[test]
  fn rejects_cross_platform_unsafe_paths() {
    for path in [
      "/Octafile",
      "../Octafile",
      "ci//Octafile",
      "ci\\Octafile",
      "C:/Octafile",
    ] {
      let mut value = spec();
      value.execution.octafile = Some(path.to_owned());
      assert!(
        value
          .validate(&JobBinding {
            job_id: "job-1",
            attempt: 1,
            now: 150,
          })
          .unwrap_err()
          .contains("normalized relative")
      );
    }
  }

  #[test]
  fn requires_an_image_only_for_microsandbox() {
    let mut value = spec();
    value.runtime.image = None;
    assert!(
      value
        .validate(&JobBinding {
          job_id: "job-1",
          attempt: 1,
          now: 150,
        })
        .unwrap_err()
        .contains("immutable sha256 image")
    );

    value.runtime.backend = BackendKind::Native;
    assert!(
      value
        .validate(&JobBinding {
          job_id: "job-1",
          attempt: 1,
          now: 150,
        })
        .is_ok()
    );
  }
}
