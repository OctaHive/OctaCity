//! Wire-shape, signature, and signed-boundary validation tests.

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

fn binding(spec: &JobSpecV1) -> JobBinding<'_> {
  JobBinding {
    job_id: &spec.job_id,
    attempt: spec.attempt,
    now: spec.issued_at,
  }
}

#[test]
fn documented_job_spec_example_matches_the_wire_type() {
  let specification = include_str!("../../../docs/protocols/signed-job-spec-v1.md").replace("\r\n", "\n");
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
