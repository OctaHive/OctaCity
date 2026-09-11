//! Coordinator transport DTO validation and strict-JSON tests.

use std::collections::BTreeMap;

use crate::{PlatformArchitecture, PlatformOs};

use super::*;

fn envelope() -> SignedEnvelope {
  SignedEnvelope {
    key_id: "primary".to_owned(),
    algorithm: "ed25519".to_owned(),
    payload: "payload".to_owned(),
    signature: "signature".to_owned(),
  }
}

fn capacity() -> HostCapacity {
  HostCapacity {
    logical_cpu_count: 8,
    total_memory_bytes: 16 * 1024,
    work_disk_total_bytes: 32 * 1024,
    state_disk_total_bytes: 64 * 1024,
    virtualization_available: true,
  }
}

fn inventory() -> AgentInventory {
  AgentInventory {
    agent_id: "agent-1".to_owned(),
    agent_version: "0.1.0".to_owned(),
    coordinator_protocols: vec![COORDINATOR_PROTOCOL_VERSION],
    labels: BTreeMap::from([("region".to_owned(), "test".to_owned())]),
    host_platform: PlatformSpec {
      os: PlatformOs::Linux,
      architecture: PlatformArchitecture::Amd64,
    },
    host_capacity: capacity(),
    runtimes: vec![RuntimeCapability {
      backend: "microsandbox".to_owned(),
      mode: RuntimeMode::Oci,
      platform: PlatformSpec {
        os: PlatformOs::Linux,
        architecture: PlatformArchitecture::Amd64,
      },
      isolation: Some(OciIsolation::Hypervisor),
    }],
    octa: OctaInventory {
      version: "0.3.0".to_owned(),
      runner_sha256: "1".repeat(64),
      build_commit: Some("abc123".to_owned()),
      runner_protocols: vec![1],
      event_schemas: vec![1],
      plugin_protocols: vec![1],
      octafile_versions: vec![1],
      features: vec!["reports".to_owned()],
      plugins: vec![TaskPluginInventory {
        name: "shell".to_owned(),
        version: "0.3.0".to_owned(),
        protocol: 1,
        platforms: vec!["linux-x86_64".to_owned()],
        sha256: "2".repeat(64),
        capabilities: vec!["shell".to_owned()],
      }],
    },
    source_plugins: vec![SourcePluginInventory {
      name: "git".to_owned(),
      version: "0.1.0".to_owned(),
      protocol_min: 1,
      protocol_max: 1,
      platforms: vec!["linux-x86_64".to_owned()],
      sha256: "3".repeat(64),
    }],
  }
}

fn lease() -> LeaseAssignment {
  LeaseAssignment {
    lease_id: "lease-1".to_owned(),
    job_id: "job-1".to_owned(),
    attempt: 1,
    fencing_token: "fence-1".to_owned(),
    issued_at: 100,
    expires_at: 200,
    signed_job_spec: envelope(),
  }
}

#[test]
fn validates_complete_registration_inventory() {
  let request = RegisterAgentRequest {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: "request-1".to_owned(),
    inventory: inventory(),
  };
  request.validate().unwrap();
  let json = serde_json::to_string(&request).unwrap();
  let decoded: RegisterAgentRequest = serde_json::from_str(&json).unwrap();
  assert_eq!(decoded, request);

  let mut invalid = request;
  invalid.inventory.runtimes[0].isolation = None;
  assert!(invalid.validate().unwrap_err().to_string().contains("isolation"));
}

#[test]
fn rejects_expired_short_and_unfenced_leases() {
  lease().validate(120, 10).unwrap();

  let mut expired = lease();
  expired.expires_at = 120;
  assert!(expired.validate(120, 10).is_err());

  let mut short = lease();
  short.expires_at = 131;
  assert!(short.validate(120, 10).is_ok());
  short.expires_at = 130;
  assert!(short.validate(120, 10).is_err());

  let mut unfenced = lease();
  unfenced.fencing_token.clear();
  assert!(unfenced.validate(120, 10).is_err());
}

#[test]
fn correlates_responses_and_validates_renewal() {
  let response = AcquireLeaseResponse::Lease {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: "request-1".to_owned(),
    lease: lease(),
  };
  response.validate("request-1", 120, 10).unwrap();
  assert!(response.validate("another-request", 120, 10).is_err());

  let heartbeat = HeartbeatResponse {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: "heartbeat-1".to_owned(),
    directive: HeartbeatDirective::Continue { expires_at: 200 },
  };
  heartbeat.validate("heartbeat-1", 120, 10).unwrap();
  assert!(heartbeat.validate("heartbeat-1", 195, 10).is_err());
}

#[test]
fn bounds_host_snapshots_by_registered_capacity() {
  let snapshot = HostSnapshot {
    available_cpu_millis: 4_000,
    available_memory_bytes: 8 * 1024,
    work_disk_free_bytes: 16 * 1024,
    state_disk_free_bytes: 32 * 1024,
    active_job: Some(ActiveJob {
      job_id: "job-1".to_owned(),
      attempt: 1,
      lease_id: "lease-1".to_owned(),
    }),
    backends: vec![BackendHealth {
      backend: "microsandbox".to_owned(),
      status: BackendHealthStatus::Ready,
      message: None,
    }],
  };
  snapshot.validate(&capacity()).unwrap();

  let mut invalid = snapshot;
  invalid.available_memory_bytes = capacity().total_memory_bytes + 1;
  assert!(invalid.validate(&capacity()).is_err());
}

#[test]
fn rejects_unknown_transport_fields() {
  let json = r#"{
    "protocol_version":1,
    "request_id":"request-1",
    "registration_id":"registration-1",
    "wait_seconds":30,
    "unexpected":true
  }"#;
  assert!(serde_json::from_str::<AcquireLeaseRequest>(json).is_err());
}

#[test]
fn golden_coordinator_documents_match_the_wire_types() {
  let registration: RegisterAgentRequest = serde_json::from_str(include_str!(
    "../../../../protocol/coordinator/register-request-v1.json"
  ))
  .unwrap();
  registration.validate().unwrap();

  let response: RegisterAgentResponse = serde_json::from_str(include_str!(
    "../../../../protocol/coordinator/register-response-v1.json"
  ))
  .unwrap();
  response.validate("register-20260911-1").unwrap();

  let lease: AcquireLeaseResponse =
    serde_json::from_str(include_str!("../../../../protocol/coordinator/lease-response-v1.json")).unwrap();
  lease.validate("poll-20260911-1", 1_789_056_100, 10).unwrap();

  let heartbeat: HeartbeatRequest = serde_json::from_str(include_str!(
    "../../../../protocol/coordinator/heartbeat-request-v1.json"
  ))
  .unwrap();
  heartbeat.validate(&registration.inventory.host_capacity).unwrap();

  let heartbeat: HeartbeatResponse = serde_json::from_str(include_str!(
    "../../../../protocol/coordinator/heartbeat-response-v1.json"
  ))
  .unwrap();
  heartbeat.validate("heartbeat-20260911-1", 1_789_056_100, 10).unwrap();
}
