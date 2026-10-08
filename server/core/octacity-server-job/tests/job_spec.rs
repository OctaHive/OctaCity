use std::{collections::BTreeMap, str::FromStr};

use octacity_protocol::{
  ExecutionMode, ExecutionTargetV2, JobBinding, JobSpecV3, NetworkPolicy, OctaSpec, OutputLimits, PlatformArchitecture,
  PlatformOs, PlatformSpec, RuntimeSpec, RuntimeSpecV2, RuntimeTarget, VerifiedJobSpec, guarantees_for,
  verify_compatible_job_spec, verify_job_spec,
};
use octacity_server_domain::{
  AttemptNumber, BuildId, ImmutableRevision, JobId, PipelineNodeId, RepositoryLocator, SourceReference, Timestamp,
};
use octacity_server_job::{
  JobRuntimePolicy, JobSpecBuildSnapshot, JobSpecDerivationError, JobSpecPolicySnapshot, JobSpecSigner,
  JobSpecTemplate, JobSpecValidity, MAX_JOB_SPEC_VALIDITY_SECONDS, ManagedJobSpecIntent, SourcePluginPolicy,
  derive_job_spec_template, derive_managed_job_spec_template, sign_ready_job_spec,
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
fn signs_v2_intent_without_reinterpreting_the_legacy_signing_path() {
  let build = JobSpecBuildSnapshot::new(
    build_id(),
    ImmutableRevision::new("0123456789abcdef").unwrap(),
    None,
    RepositoryLocator::new("https://example.test/repository.git").unwrap(),
    BTreeMap::new(),
  )
  .unwrap();
  let platform = PlatformSpec {
    os: PlatformOs::Linux,
    architecture: PlatformArchitecture::Amd64,
  };
  let policy = JobSpecPolicySnapshot::new(
    SourcePluginPolicy::new("git", "1.0.0", DIGEST, "url").unwrap(),
    OctaSpec {
      version: "0.4.0".to_owned(),
      runner_sha256: DIGEST.to_owned(),
      runner_protocol: 3,
      event_schema: 3,
      plugin_protocol: 1,
      plugin_digests: BTreeMap::new(),
    },
    JobRuntimePolicy::Current(RuntimeSpecV2 {
      target: ExecutionTargetV2 {
        mode: ExecutionMode::Isolation,
        host_platform: platform,
        target_platform: platform,
        required_guarantees: guarantees_for(ExecutionMode::Isolation),
        immutable_image: None,
      },
      cpu_millis: 1_000,
      memory_bytes: 1_024,
      writable_disk_bytes: 2_048,
      timeout_seconds: 60,
      network: NetworkPolicy::Disabled,
      workload_identity_profile: None,
    }),
    None,
    None,
    OutputLimits {
      artifact_count: 0,
      artifact_bytes: 0,
      report_count: 0,
      report_bytes: 0,
      single_output_bytes: 0,
    },
    JobSpecValidity::new(600).unwrap(),
  )
  .unwrap();
  let template = derive_job_spec_template(&build, node_id(), json!({"commands": ["build"]}), &policy).unwrap();
  let signer = JobSpecSigner::new("active-key", [7; 32]).unwrap();
  let signed = sign_ready_job_spec(&template, AttemptNumber::FIRST, job_id(), issued_at(), &signer).unwrap();
  let verified = verify_compatible_job_spec(
    signed.envelope(),
    &BTreeMap::from([(signer.key_id().to_owned(), signer.verifying_key())]),
    JobBinding {
      job_id: &job_id().to_string(),
      attempt: 1,
      now: 100,
    },
  )
  .unwrap();
  assert!(matches!(verified, VerifiedJobSpec::V2(spec) if spec.runtime.target.mode == ExecutionMode::Isolation));
  assert!(
    verify_job_spec(
      signed.envelope(),
      &BTreeMap::from([(signer.key_id().to_owned(), signer.verifying_key())]),
      JobBinding {
        job_id: &job_id().to_string(),
        attempt: 1,
        now: 100,
      },
    )
    .is_err()
  );
}

#[test]
fn ordinary_template_persistence_keeps_its_existing_execution_shape() {
  let (template, _) = fixture(json!({
    "commands": ["build"],
    "arguments": ["--locked"]
  }))
  .unwrap();

  let persisted = serde_json::to_value(&template).unwrap();
  assert_eq!(persisted["execution"]["commands"], json!(["build"]));
  assert_eq!(persisted["execution"]["arguments"], json!(["--locked"]));
  assert!(persisted["execution"].get("ordinary").is_none());
  assert!(persisted["execution"].get("managed").is_none());
  assert_eq!(serde_json::from_value::<JobSpecTemplate>(persisted).unwrap(), template);
}

#[test]
fn derives_and_signs_stable_managed_v3_intent_without_transfer_capabilities() {
  let canonical = canonical_v3_spec();
  let build = JobSpecBuildSnapshot::new(
    build_id(),
    ImmutableRevision::new(canonical.source.revision.clone()).unwrap(),
    None,
    RepositoryLocator::new("https://example.test/repository.git").unwrap(),
    BTreeMap::new(),
  )
  .unwrap();
  let policy = managed_policy(&canonical);
  let intent = ManagedJobSpecIntent::new(
    canonical.execution.clone(),
    canonical.factory.clone().unwrap(),
    canonical.protected_inputs.clone(),
    canonical.permissions.clone(),
    canonical.required_enforcement.clone(),
  );
  let mut untrusted = serde_json::to_value(&intent).unwrap();
  untrusted["transfer_url"] = json!("https://storage.invalid/bearer-secret");
  assert!(serde_json::from_value::<ManagedJobSpecIntent>(untrusted).is_err());

  let template = derive_managed_job_spec_template(&build, node_id(), intent, &policy).unwrap();
  assert_eq!(template.protected_inputs(), Some(&canonical.protected_inputs));
  let persisted = serde_json::to_value(&template).unwrap();
  assert_json_omits_fields(&persisted, &["transfer_url", "download_url", "bearer", "credential"]);

  let signer = JobSpecSigner::new("active-key", [7; 32]).unwrap();
  let first = sign_ready_job_spec(&template, AttemptNumber::FIRST, job_id(), issued_at(), &signer).unwrap();
  let second = sign_ready_job_spec(&template, AttemptNumber::FIRST, job_id(), issued_at(), &signer).unwrap();
  assert_eq!(first, second);
  let verified = verify_compatible_job_spec(
    first.envelope(),
    &BTreeMap::from([(signer.key_id().to_owned(), signer.verifying_key())]),
    JobBinding {
      job_id: &job_id().to_string(),
      attempt: 1,
      now: 100,
    },
  )
  .unwrap();
  let VerifiedJobSpec::V3(verified) = verified else {
    panic!("managed intent must produce JobSpec v3");
  };
  assert_eq!(verified.execution, canonical.execution);
  assert_eq!(verified.factory, canonical.factory);
  assert_eq!(verified.protected_inputs, canonical.protected_inputs);
  assert_eq!(verified.permissions, canonical.permissions);
  assert_eq!(verified.required_enforcement, canonical.required_enforcement);
}

#[test]
fn managed_v3_rejects_legacy_runtime_and_discarded_ordinary_inputs() {
  let canonical = canonical_v3_spec();
  let intent = || {
    ManagedJobSpecIntent::new(
      canonical.execution.clone(),
      canonical.factory.clone().unwrap(),
      canonical.protected_inputs.clone(),
      canonical.permissions.clone(),
      canonical.required_enforcement.clone(),
    )
  };
  let build = JobSpecBuildSnapshot::new(
    build_id(),
    ImmutableRevision::new(canonical.source.revision.clone()).unwrap(),
    None,
    RepositoryLocator::new("https://example.test/repository.git").unwrap(),
    BTreeMap::new(),
  )
  .unwrap();
  let legacy = policy();
  assert!(matches!(
    derive_managed_job_spec_template(&build, node_id(), intent(), &legacy),
    Err(JobSpecDerivationError::InvalidPolicy)
  ));

  let parameterized = JobSpecBuildSnapshot::new(
    build_id(),
    ImmutableRevision::new(canonical.source.revision.clone()).unwrap(),
    None,
    RepositoryLocator::new("https://example.test/repository.git").unwrap(),
    BTreeMap::from([("repository-controlled".to_owned(), json!("ignored"))]),
  )
  .unwrap();
  assert!(matches!(
    derive_managed_job_spec_template(&parameterized, node_id(), intent(), &managed_policy(&canonical)),
    Err(JobSpecDerivationError::InvalidPolicy)
  ));
}

#[test]
fn placement_policy_requires_the_wire_revision_derived_from_execution_intent() {
  let (legacy, _) = fixture(json!({"commands": ["build"]})).unwrap();
  assert_eq!(legacy.placement_policy().minimum_execution_contract, 1);

  let canonical = canonical_v3_spec();
  let build = JobSpecBuildSnapshot::new(
    build_id(),
    ImmutableRevision::new(canonical.source.revision.clone()).unwrap(),
    None,
    RepositoryLocator::new("https://example.test/repository.git").unwrap(),
    BTreeMap::new(),
  )
  .unwrap();
  let managed = derive_managed_job_spec_template(
    &build,
    node_id(),
    ManagedJobSpecIntent::new(
      canonical.execution.clone(),
      canonical.factory.clone().unwrap(),
      canonical.protected_inputs.clone(),
      canonical.permissions.clone(),
      canonical.required_enforcement.clone(),
    ),
    &managed_policy(&canonical),
  )
  .unwrap();
  assert_eq!(managed.placement_policy().minimum_execution_contract, 3);
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
    JobRuntimePolicy::Legacy(RuntimeSpec {
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
    }),
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

fn canonical_v3_spec() -> JobSpecV3 {
  serde_json::from_str(include_str!(
    "../../../../shared/protocol-fixtures/job-spec/job-spec-v3.json"
  ))
  .unwrap()
}

fn managed_policy(spec: &JobSpecV3) -> JobSpecPolicySnapshot {
  JobSpecPolicySnapshot::new(
    SourcePluginPolicy::new(
      spec.source.provider.clone(),
      spec.source.plugin_version.clone(),
      spec.source.plugin_sha256.clone(),
      "url",
    )
    .unwrap(),
    spec.octa.clone(),
    JobRuntimePolicy::Current(spec.runtime.clone()),
    None,
    spec.cache.clone(),
    spec.outputs.clone(),
    JobSpecValidity::new(spec.expires_at - spec.issued_at).unwrap(),
  )
  .unwrap()
}

fn assert_json_omits_fields(value: &Value, forbidden: &[&str]) {
  match value {
    Value::Object(fields) => {
      for (name, nested) in fields {
        assert!(!forbidden.contains(&name.as_str()), "persisted forbidden field {name}");
        assert_json_omits_fields(nested, forbidden);
      }
    }
    Value::Array(values) => {
      for nested in values {
        assert_json_omits_fields(nested, forbidden);
      }
    }
    _ => {}
  }
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
