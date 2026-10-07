//! Wire-shape, signature, and signed-boundary validation tests.

use super::*;
use ed25519_dalek::Signer as _;
use ed25519_dalek::SigningKey;

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
      target: RuntimeTarget::Oci {
        platform: PlatformSpec {
          os: PlatformOs::Linux,
          architecture: PlatformArchitecture::Amd64,
        },
        isolation: OciIsolation::Hypervisor,
        image: format!("registry.example.com/octacity/build@sha256:{DIGEST}"),
      },
      cpu_millis: 1000,
      memory_bytes: 512 * 1024 * 1024,
      writable_disk_bytes: 1024 * 1024 * 1024,
      timeout_seconds: 600,
      network: NetworkPolicy::Disabled,
      workload_identity_profile: None,
    },
    cache: None,
    outputs: OutputLimits {
      artifact_count: 10,
      artifact_bytes: 1024,
      report_count: 10,
      report_bytes: 1024,
      single_output_bytes: 1024,
    },
  }
}

fn envelope(spec: &JobSpecV1, signing_key: &SigningKey) -> SignedEnvelope {
  signed_envelope(spec, signing_key)
}

fn signed_envelope(spec: &impl Serialize, signing_key: &SigningKey) -> SignedEnvelope {
  let payload = serde_json::to_vec(spec).unwrap();
  SignedEnvelope {
    key_id: "test-key".to_owned(),
    algorithm: SIGNATURE_ALGORITHM.to_owned(),
    payload: BASE64.encode(&payload),
    signature: BASE64.encode(signing_key.sign(&payload).to_bytes()),
  }
}

fn v2_spec() -> JobSpecV2 {
  let legacy = spec();
  JobSpecV2 {
    protocol_version: EXECUTION_CONTRACT_V2,
    job_id: legacy.job_id,
    attempt: legacy.attempt,
    issued_at: legacy.issued_at,
    expires_at: legacy.expires_at,
    source: legacy.source,
    octa: legacy.octa,
    execution: legacy.execution,
    runtime: RuntimeSpecV2 {
      target: ExecutionTargetV2 {
        mode: ExecutionMode::Virtualization,
        host_platform: PlatformSpec {
          os: PlatformOs::Macos,
          architecture: PlatformArchitecture::Arm64,
        },
        target_platform: PlatformSpec {
          os: PlatformOs::Linux,
          architecture: PlatformArchitecture::Arm64,
        },
        required_guarantees: guarantees_for(ExecutionMode::Virtualization),
        immutable_image: Some(format!("registry.example.com/octacity/build@sha256:{DIGEST}")),
      },
      cpu_millis: legacy.runtime.cpu_millis,
      memory_bytes: legacy.runtime.memory_bytes,
      writable_disk_bytes: legacy.runtime.writable_disk_bytes,
      timeout_seconds: legacy.runtime.timeout_seconds,
      network: legacy.runtime.network,
      workload_identity_profile: legacy.runtime.workload_identity_profile,
    },
    cache: legacy.cache,
    outputs: legacy.outputs,
  }
}

fn v3_spec() -> JobSpecV3 {
  let mut current = v2_spec();
  current
    .octa
    .plugin_digests
    .insert("codex".to_owned(), DIGEST.to_owned());
  let executable = FactoryImmutableReferenceV3 {
    identity: "codex-cli".to_owned(),
    version: "0.116.0".to_owned(),
    sha256: DIGEST.to_owned(),
  };
  JobSpecV3 {
    protocol_version: EXECUTION_CONTRACT_V3,
    job_id: current.job_id,
    attempt: current.attempt,
    issued_at: current.issued_at,
    expires_at: current.expires_at,
    source: current.source,
    octa: current.octa,
    execution: ManagedOctaExecutionV3 {
      octafile_input: "managed-octafile".to_owned(),
      tasks: vec!["implement".to_owned()],
    },
    runtime: RuntimeSpecV2 {
      network: NetworkPolicy::Restricted {
        allowed_hosts: vec!["api.openai.com".to_owned()],
      },
      ..current.runtime
    },
    cache: current.cache,
    outputs: current.outputs,
    factory: Some(FactoryCausalityV3 {
      factory_run_id: "00000000-0000-0000-0000-000000000001".to_owned(),
      factory_configuration_id: "00000000-0000-0000-0000-000000000002".to_owned(),
      factory_configuration_version: 1,
      stage_attempt_id: "00000000-0000-0000-0000-000000000003".to_owned(),
      stage_kind: FactoryStageKindV3::Implementation,
      task_envelope_digest: DIGEST.to_owned(),
      subject_digest: DIGEST.to_owned(),
      parent: None,
    }),
    protected_inputs: ProtectedInputManifestV3 {
      inputs: vec![
        ProtectedInputV3 {
          artifact_id: "context-manifest".to_owned(),
          size_bytes: 128,
          sha256: DIGEST.to_owned(),
          media_type: "application/json".to_owned(),
          destination: "/octacity/protected/context-manifest.json".to_owned(),
        },
        ProtectedInputV3 {
          artifact_id: "managed-octafile".to_owned(),
          size_bytes: 256,
          sha256: DIGEST.to_owned(),
          media_type: "application/yaml".to_owned(),
          destination: "/octacity/protected/Octafile.yml".to_owned(),
        },
      ],
    },
    permissions: FactoryPermissionSetV3 {
      plugins: vec![FactoryImmutableReferenceV3 {
        identity: "codex".to_owned(),
        version: "0.5.0".to_owned(),
        sha256: DIGEST.to_owned(),
      }],
      executables: vec![executable.clone()],
      tools: Vec::new(),
      commands: vec![FactoryCommandPermissionV3 {
        executable,
        arguments: vec![FactoryCommandArgumentV3::Exact {
          value: "--json".to_owned(),
        }],
      }],
      max_descendants: 4,
      mounts: vec![
        FactoryMountPermissionV3 {
          root: "/octacity/protected".to_owned(),
          mode: FactoryMountModeV3::ReadOnly,
        },
        FactoryMountPermissionV3 {
          root: "/workspace/source".to_owned(),
          mode: FactoryMountModeV3::ReadWrite,
        },
      ],
      network_hosts: vec!["api.openai.com".to_owned()],
      secret_profiles: vec!["model-coding".to_owned()],
      workload_identity_profiles: Vec::new(),
      resources: FactoryResourceLimitsV3 {
        cpu_millis: 1_000,
        memory_bytes: 512 * 1024 * 1024,
        disk_bytes: 1024 * 1024 * 1024,
        process_count: 8,
        elapsed_millis: 600_000,
      },
      outputs: FactoryOutputPermissionsV3 {
        kinds: vec!["codex-result".to_owned()],
        max_artifact_count: 10,
        max_artifact_bytes: 1_024,
        max_report_count: 10,
        max_report_bytes: 1_024,
      },
    },
    required_enforcement: FactoryEnforcementCapabilityV3::ALL.to_vec(),
  }
}

fn binding(spec: &JobSpecV1) -> JobBinding<'_> {
  JobBinding {
    job_id: &spec.job_id,
    attempt: spec.attempt,
    now: spec.issued_at,
  }
}

#[test]
fn documented_job_spec_example_matches_the_wire_type() {
  let specification = include_str!("../../../docs/reference/protocols/signed-job-spec-v1.md").replace("\r\n", "\n");
  let section = specification
    .split_once("## Complete JobSpec shape")
    .expect("specification must contain the complete example")
    .1;
  let json = section
    .split_once("```json\n")
    .expect("complete example must be a JSON block")
    .1
    .split_once("\n```")
    .expect("complete example JSON block must terminate")
    .0;
  let spec: JobSpecV1 = serde_json::from_str(json).expect("documented JobSpec must deserialize");
  spec
    .validate(&JobBinding {
      job_id: &spec.job_id,
      attempt: spec.attempt,
      now: spec.issued_at,
    })
    .expect("documented JobSpec must pass semantic validation");
}

#[test]
fn validates_per_output_limits_against_each_enabled_output_kind() {
  let mut spec = spec();
  spec.outputs.single_output_bytes = 1025;
  assert!(spec.validate(&binding(&spec)).is_err());

  spec.outputs.artifact_count = 0;
  spec.outputs.artifact_bytes = 0;
  spec.outputs.single_output_bytes = 1024;
  assert!(spec.validate(&binding(&spec)).is_ok());

  spec.outputs.report_count = 0;
  spec.outputs.report_bytes = 0;
  assert!(spec.validate(&binding(&spec)).is_err());
  spec.outputs.single_output_bytes = 0;
  assert!(spec.validate(&binding(&spec)).is_ok());
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
fn negotiates_v2_without_reinterpreting_legacy_envelopes() {
  let signing_key = SigningKey::from_bytes(&[7; 32]);
  let keys = BTreeMap::from([("test-key".to_owned(), signing_key.verifying_key())]);

  let legacy = spec();
  let verified = verify_compatible_job_spec(&envelope(&legacy, &signing_key), &keys, binding(&legacy)).unwrap();
  let VerifiedJobSpec::V1(round_trip) = verified else {
    panic!("a v1 envelope must remain a v1 Native/OCI document");
  };
  assert_eq!(round_trip, legacy);

  let current = v2_spec();
  let verified = verify_compatible_job_spec(
    &signed_envelope(&current, &signing_key),
    &keys,
    JobBinding {
      job_id: &current.job_id,
      attempt: current.attempt,
      now: current.issued_at,
    },
  )
  .unwrap();
  let VerifiedJobSpec::V2(round_trip) = verified else {
    panic!("a v2 envelope must use provider-neutral execution intent");
  };
  assert_eq!(round_trip, current);
  assert!(
    serde_json::to_value(&round_trip).unwrap()["runtime"]["target"]
      .get("provider")
      .is_none()
  );
}

#[test]
fn canonical_job_spec_fixtures_preserve_legacy_bytes_and_define_v3() {
  let fixtures = [
    (
      include_str!("../../protocol-fixtures/job-spec/job-spec-v1.json").trim(),
      serde_json::to_vec(&spec()).unwrap(),
    ),
    (
      include_str!("../../protocol-fixtures/job-spec/job-spec-v2.json").trim(),
      serde_json::to_vec(&v2_spec()).unwrap(),
    ),
    (
      include_str!("../../protocol-fixtures/job-spec/job-spec-v3.json").trim(),
      serde_json::to_vec(&v3_spec()).unwrap(),
    ),
  ];
  for (fixture, encoded) in &fixtures {
    assert_eq!(encoded, fixture.as_bytes());
  }
  let decoded: JobSpecV3 = serde_json::from_str(fixtures[2].0).unwrap();
  decoded
    .validate(&JobBinding {
      job_id: &decoded.job_id,
      attempt: decoded.attempt,
      now: decoded.issued_at,
    })
    .unwrap();
}

#[test]
fn verifies_v3_only_after_signature_and_strict_semantic_validation() {
  let signing_key = SigningKey::from_bytes(&[7; 32]);
  let keys = BTreeMap::from([("test-key".to_owned(), signing_key.verifying_key())]);
  let current = v3_spec();
  let verified = verify_compatible_job_spec(
    &signed_envelope(&current, &signing_key),
    &keys,
    JobBinding {
      job_id: &current.job_id,
      attempt: current.attempt,
      now: current.issued_at,
    },
  )
  .unwrap();
  assert_eq!(verified, VerifiedJobSpec::V3(Box::new(current)));

  let mut malformed = serde_json::to_value(v3_spec()).unwrap();
  malformed["transfer_url"] = serde_json::json!("https://storage.invalid/private");
  let envelope = signed_envelope(&malformed, &signing_key);
  assert!(matches!(
    verify_compatible_job_spec(
      &envelope,
      &keys,
      JobBinding {
        job_id: "job-1",
        attempt: 1,
        now: 100,
      },
    ),
    Err(JobSpecError::Json(_))
  ));
}

#[test]
fn v3_rejects_unsafe_inputs_incomplete_enforcement_and_host_fallback() {
  let mut value = v3_spec();
  value.protected_inputs.inputs[0].destination = "/octacity/protected/../escape".to_owned();
  assert!(value.validate(&binding(&spec())).is_err());

  let mut value = v3_spec();
  value.required_enforcement.pop();
  assert!(value.validate(&binding(&spec())).is_err());

  let mut value = v3_spec();
  value.runtime.target.mode = ExecutionMode::Host;
  value.runtime.target.target_platform = value.runtime.target.host_platform;
  value.runtime.target.required_guarantees.clear();
  value.runtime.target.immutable_image = None;
  assert!(value.validate(&binding(&spec())).is_err());

  let mut value = v3_spec();
  value.permissions.mounts.swap(0, 1);
  assert!(value.validate(&binding(&spec())).is_err());
}

#[test]
fn v3_wildcard_command_arguments_have_explicit_enforceable_bounds() {
  let mut value = v3_spec();
  value.permissions.commands[0].arguments = vec![FactoryCommandArgumentV3::Any { max_bytes: 4 }];
  value.permissions.validate().unwrap();
  assert!(value.permissions.commands[0].permits_arguments(&["four".to_owned()]));
  assert!(!value.permissions.commands[0].permits_arguments(&["longer".to_owned()]));

  value.permissions.commands[0].arguments = vec![FactoryCommandArgumentV3::Any { max_bytes: 0 }];
  assert!(value.permissions.validate().is_err());
  value.permissions.commands[0].arguments = vec![FactoryCommandArgumentV3::Any {
    max_bytes: u32::try_from(MAX_FACTORY_COMMAND_ARGUMENT_BYTES + 1).unwrap(),
  }];
  assert!(value.permissions.validate().is_err());
}

#[test]
fn older_agents_remain_eligible_for_v1_and_v2_but_not_v3() {
  let old_agent = ExecutionContractRange { min: 1, max: 2 };
  let v3_agent = ExecutionContractRange { min: 1, max: 3 };
  assert_eq!(SUPPORTED_EXECUTION_CONTRACTS.negotiate(old_agent), Some(2));
  assert_eq!(SUPPORTED_EXECUTION_CONTRACTS.negotiate(v3_agent), Some(3));
  assert!(old_agent.contains(EXECUTION_CONTRACT_V1));
  assert!(old_agent.contains(EXECUTION_CONTRACT_V2));
  assert!(!old_agent.contains(EXECUTION_CONTRACT_V3));
  assert!(v3_agent.contains(EXECUTION_CONTRACT_V3));
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
fn native_and_oci_targets_have_distinct_wire_shapes() {
  let mut value = spec();
  value.runtime.target = RuntimeTarget::Native {
    platform: PlatformSpec {
      os: PlatformOs::Linux,
      architecture: PlatformArchitecture::Amd64,
    },
  };
  assert_eq!(value.runtime.mode(), RuntimeMode::Native);
  assert!(
    value
      .validate(&JobBinding {
        job_id: "job-1",
        attempt: 1,
        now: 150,
      })
      .is_ok()
  );

  let json = serde_json::to_value(&value.runtime).unwrap();
  assert_eq!(json["target"]["mode"], "native");
  assert!(json["target"].get("image").is_none());
}

#[test]
fn rejects_mutable_or_ambiguous_runtime_images() {
  for image in [
    "registry.example.com/build:latest",
    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "https://registry.example.com/build@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "registry.example.com/build@tag@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
  ] {
    let mut value = spec();
    let RuntimeTarget::Oci { image: target, .. } = &mut value.runtime.target else {
      unreachable!();
    };
    *target = image.to_owned();
    assert!(value.validate(&binding(&value)).is_err());
  }
}

#[test]
fn rejects_a_macos_oci_guest() {
  let mut value = spec();
  let RuntimeTarget::Oci { platform, .. } = &mut value.runtime.target else {
    unreachable!();
  };
  platform.os = PlatformOs::Macos;

  assert!(value.validate(&binding(&value)).unwrap_err().contains("macOS guest"));
}

#[test]
fn compares_signed_output_limits_component_by_component() {
  let maximum = OutputLimits {
    artifact_count: 4,
    artifact_bytes: 1024,
    report_count: 2,
    report_bytes: 512,
    single_output_bytes: 256,
  };
  let mut requested = maximum.clone();
  assert!(requested.validate().is_ok());
  assert!(requested.is_within(&maximum));

  requested.artifact_count += 1;
  assert!(!requested.is_within(&maximum));
  requested = maximum.clone();
  requested.artifact_bytes += 1;
  assert!(!requested.is_within(&maximum));
  requested = maximum.clone();
  requested.report_count += 1;
  assert!(!requested.is_within(&maximum));
  requested = maximum.clone();
  requested.report_bytes += 1;
  assert!(!requested.is_within(&maximum));
  requested = maximum.clone();
  requested.single_output_bytes += 1;
  assert!(!requested.is_within(&maximum));
}

#[test]
fn rejects_an_empty_workload_identity_profile() {
  let mut value = spec();
  value.runtime.workload_identity_profile = Some(String::new());

  assert!(
    value
      .validate(&binding(&value))
      .unwrap_err()
      .contains("must not be empty")
  );
}

#[test]
fn rejects_oversized_base64_before_decoding() {
  let signing_key = SigningKey::from_bytes(&[3; 32]);
  let keys = BTreeMap::from([("test-key".to_owned(), signing_key.verifying_key())]);
  let specification = spec();
  let mut envelope = SignedEnvelope {
    key_id: "test-key".to_owned(),
    algorithm: SIGNATURE_ALGORITHM.to_owned(),
    payload: "A".repeat(MAX_ENCODED_JOB_SPEC_BYTES + 1),
    signature: BASE64.encode([0_u8; ED25519_SIGNATURE_BYTES]),
  };
  assert!(matches!(
    verify_job_spec(&envelope, &keys, binding(&specification)),
    Err(JobSpecError::PayloadTooLarge)
  ));

  envelope.payload = BASE64.encode(b"{}");
  envelope.signature = "A".repeat(MAX_ENCODED_SIGNATURE_BYTES + 1);
  assert!(matches!(
    verify_job_spec(&envelope, &keys, binding(&specification)),
    Err(JobSpecError::SignatureLength)
  ));
}

#[test]
fn cache_policy_preserves_independent_permissions_and_portable_namespaces() {
  use octa_cache_protocol::CacheMode;

  for (read, write, mode) in [
    (true, false, CacheMode::ReadOnly),
    (false, true, CacheMode::WriteOnly),
    (true, true, CacheMode::ReadWrite),
  ] {
    let policy = CachePolicy {
      namespace: "project/main".to_owned(),
      read,
      write,
    };
    assert_eq!(policy.mode().unwrap(), mode);
  }

  for namespace in ["", "project\nmain"] {
    let policy = CachePolicy {
      namespace: namespace.to_owned(),
      read: true,
      write: true,
    };
    assert!(policy.validate().is_err(), "accepted namespace {namespace:?}");
  }
  assert!(
    CachePolicy {
      namespace: "project/main".to_owned(),
      read: false,
      write: false,
    }
    .validate()
    .is_err()
  );
  assert!(
    CachePolicy {
      namespace: "project/main".to_owned(),
      read: false,
      write: false,
    }
    .mode()
    .is_err()
  );
}

#[test]
fn platform_keys_round_trip_and_map_to_octa_cache_dimensions() {
  use octa_cache_protocol::{PlatformArchitecture as CacheArchitecture, PlatformOs as CacheOs};

  for (key, platform, cache_os, cache_architecture) in [
    (
      "linux-amd64",
      PlatformSpec {
        os: PlatformOs::Linux,
        architecture: PlatformArchitecture::Amd64,
      },
      CacheOs::Linux,
      CacheArchitecture::Amd64,
    ),
    (
      "linux-arm64",
      PlatformSpec {
        os: PlatformOs::Linux,
        architecture: PlatformArchitecture::Arm64,
      },
      CacheOs::Linux,
      CacheArchitecture::Arm64,
    ),
    (
      "windows-amd64",
      PlatformSpec {
        os: PlatformOs::Windows,
        architecture: PlatformArchitecture::Amd64,
      },
      CacheOs::Windows,
      CacheArchitecture::Amd64,
    ),
    (
      "windows-arm64",
      PlatformSpec {
        os: PlatformOs::Windows,
        architecture: PlatformArchitecture::Arm64,
      },
      CacheOs::Windows,
      CacheArchitecture::Arm64,
    ),
    (
      "macos-amd64",
      PlatformSpec {
        os: PlatformOs::Macos,
        architecture: PlatformArchitecture::Amd64,
      },
      CacheOs::Macos,
      CacheArchitecture::Amd64,
    ),
    (
      "macos-arm64",
      PlatformSpec {
        os: PlatformOs::Macos,
        architecture: PlatformArchitecture::Arm64,
      },
      CacheOs::Macos,
      CacheArchitecture::Arm64,
    ),
  ] {
    let parsed: PlatformSpec = key.parse().unwrap();
    assert_eq!(parsed, platform);
    assert_eq!(parsed.to_string(), key);
    assert_eq!(CacheOs::from(parsed.os), cache_os);
    assert_eq!(CacheArchitecture::from(parsed.architecture), cache_architecture);
  }
  assert!("linux-x86_64".parse::<PlatformSpec>().is_err());
}
