use std::{collections::BTreeMap, str::FromStr};

use octacity_protocol::{
  JobBinding, NetworkPolicy, OctaSpec, OutputLimits, PlatformArchitecture, PlatformOs, PlatformSpec, RuntimeSpec,
  RuntimeTarget, verify_job_spec,
};
use octacity_server_domain::{
  AttemptNumber, BuildId, ImmutableRevision, JobId, PipelineNodeId, RepositoryLocator, SourceReference, Timestamp,
};
use octacity_server_job::{
  JobSpecBuildSnapshot, JobSpecDerivationError, JobSpecPolicySnapshot, JobSpecSigner, JobSpecTemplate, JobSpecValidity,
  MAX_JOB_SPEC_VALIDITY_SECONDS, SourcePluginPolicy, derive_job_spec_template, sign_ready_job_spec,
};
use octacity_server_secrets::SecretProfileName;
use serde_json::{Value, json};

const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

#[test]
fn derives_one_deterministic_protocol_valid_envelope() {
  let (template, signer) = fixture(json!({
    "commands": ["build", "test"],
    "octafile": "ci/Octafile.yml",
    "arguments": ["--locked"],
    "concurrency": 2,
    "parallel": true,
    "failfast": true
  }))
  .unwrap();

  let first = sign_ready_job_spec(&template, AttemptNumber::FIRST, job_id(), issued_at(), &signer).unwrap();
  let second = sign_ready_job_spec(&template, AttemptNumber::FIRST, job_id(), issued_at(), &signer).unwrap();
  assert_eq!(first, second);

  let verified = verify_job_spec(
    first.envelope(),
    &BTreeMap::from([(signer.key_id().to_owned(), signer.verifying_key())]),
    JobBinding {
      job_id: &job_id().to_string(),
      attempt: 1,
      now: 100,
    },
  )
  .unwrap();
  assert_eq!(verified.source.revision, "0123456789abcdef");
  assert_eq!(verified.source.reference.as_deref(), Some("refs/heads/main"));
  assert_eq!(
    verified.source.parameters["url"],
    json!("https://example.test/repository.git")
  );
  assert_eq!(verified.execution.variables["profile"], "release");
  assert_eq!(verified.execution.variables["retries"], "2");
  assert_eq!(verified.execution.variables["strict"], "true");
  assert_eq!(verified.execution.secrets_profile.as_deref(), Some("ci/secrets.yml"));
  assert_eq!(verified.issued_at, 100);
  assert_eq!(verified.expires_at, 700);
}

#[test]
fn rejects_every_caller_controlled_server_owned_field() {
  for forbidden in [
    "secrets",
    "secrets_profile",
    "host_path",
    "fencing_token",
    "upload_url",
    "signed_job_spec",
  ] {
    let mut template = serde_json::Map::from_iter([("commands".to_owned(), json!(["build"]))]);
    template.insert(forbidden.to_owned(), json!("caller-controlled"));
    let result = fixture(Value::Object(template)).and_then(|(template, signer)| {
      sign_ready_job_spec(&template, AttemptNumber::FIRST, job_id(), issued_at(), &signer)
    });
    assert!(
      matches!(result, Err(JobSpecDerivationError::InvalidTemplate)),
      "accepted forbidden field {forbidden}"
    );
  }
}

#[test]
fn rejects_non_primitive_build_parameters_before_signing() {
  let (_, signer) = fixture(json!({"commands": ["build"]})).unwrap();
  let build = JobSpecBuildSnapshot::new(
    build_id(),
    ImmutableRevision::new("0123456789abcdef").unwrap(),
    None,
    RepositoryLocator::new("https://example.test/repository.git").unwrap(),
    BTreeMap::from([("nested".to_owned(), json!({"secret": "not-a-variable"}))]),
  )
  .unwrap();

  assert!(matches!(
    derive_job_spec_template(&build, node_id(), json!({"commands": ["build"]}), &policy())
      .and_then(|template| sign_ready_job_spec(&template, AttemptNumber::FIRST, job_id(), issued_at(), &signer)),
    Err(JobSpecDerivationError::InvalidParameter(name)) if name == "nested"
  ));
}

#[test]
fn rejects_host_paths_and_credential_bearing_repository_locators() {
  for locator in [
    "/srv/repository",
    "~/repository",
    r"C:\repository",
    "C:repository",
    "file:///srv/repository",
    "https://user:password@example.test/repository.git",
    "https://example.test/repository.git?token=secret",
    "../private-repository",
    "git@example.test:private-repository",
    "https://example.test/../private-repository",
    "https://example.test/%2e%2e/private-repository",
  ] {
    assert!(
      RepositoryLocator::new(locator).is_err(),
      "accepted unsafe repository locator {locator}"
    );
  }
}

#[test]
fn validity_interval_is_positive_bounded_and_strictly_deserialized() {
  assert!(JobSpecValidity::new(0).is_err());
  assert!(JobSpecValidity::new(MAX_JOB_SPEC_VALIDITY_SECONDS).is_ok());
  assert!(JobSpecValidity::new(MAX_JOB_SPEC_VALIDITY_SECONDS + 1).is_err());
  assert!(serde_json::from_value::<JobSpecValidity>(json!(0)).is_err());
  assert!(serde_json::from_value::<JobSpecValidity>(json!(MAX_JOB_SPEC_VALIDITY_SECONDS + 1)).is_err());
}

fn fixture(template: Value) -> Result<(JobSpecTemplate, JobSpecSigner), JobSpecDerivationError> {
  let build = JobSpecBuildSnapshot::new(
    build_id(),
    ImmutableRevision::new("0123456789abcdef").unwrap(),
    Some(SourceReference::new("refs/heads/main").unwrap()),
    RepositoryLocator::new("https://example.test/repository.git").unwrap(),
    BTreeMap::from([
      ("profile".to_owned(), json!("release")),
      ("retries".to_owned(), json!(2)),
      ("strict".to_owned(), json!(true)),
    ]),
  )
  .unwrap();
  let template = derive_job_spec_template(&build, node_id(), template, &policy())?;
  Ok((template, JobSpecSigner::new("active-key", [7; 32]).unwrap()))
}

fn policy() -> JobSpecPolicySnapshot {
  JobSpecPolicySnapshot::new(
    SourcePluginPolicy::new("git", "1.0.0", DIGEST, "url").unwrap(),
    OctaSpec {
      version: "0.4.0".to_owned(),
      runner_sha256: DIGEST.to_owned(),
      runner_protocol: 3,
      event_schema: 3,
      plugin_protocol: 1,
      plugin_digests: BTreeMap::from([("shell".to_owned(), DIGEST.to_owned())]),
    },
    RuntimeSpec {
      target: RuntimeTarget::Native {
        platform: PlatformSpec {
          os: PlatformOs::Linux,
          architecture: PlatformArchitecture::Amd64,
        },
      },
      cpu_millis: 1_000,
      memory_bytes: 512 * 1024 * 1024,
      writable_disk_bytes: 1024 * 1024 * 1024,
      timeout_seconds: 600,
      network: NetworkPolicy::Disabled,
      workload_identity_profile: None,
    },
    Some(SecretProfileName::new("ci/secrets.yml").unwrap()),
    None,
    OutputLimits {
      artifact_count: 2,
      artifact_bytes: 1_024,
      report_count: 2,
      report_bytes: 1_024,
      single_output_bytes: 512,
    },
    JobSpecValidity::new(600).unwrap(),
  )
  .unwrap()
}

fn issued_at() -> Timestamp {
  Timestamp::from_unix_millis(100_999).unwrap()
}

fn build_id() -> BuildId {
  BuildId::from_str("00000000-0000-0000-0000-000000000001").unwrap()
}

fn job_id() -> JobId {
  JobId::from_str("00000000-0000-0000-0000-000000000003").unwrap()
}

fn node_id() -> PipelineNodeId {
  PipelineNodeId::new("build").unwrap()
}
