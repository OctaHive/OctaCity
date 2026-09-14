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
      resource_usage: None,
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
fn validates_and_correlates_fenced_output_uploads() {
  let lease = LeaseFence {
    lease_id: "lease-1".to_owned(),
    job_id: "job-1".to_owned(),
    attempt: 1,
    fencing_token: "fence-1".to_owned(),
  };
  let request = BeginOutputUploadRequest {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: "request-1".to_owned(),
    registration_id: "registration-1".to_owned(),
    lease: lease.clone(),
    upload_key: "upload-key".to_owned(),
    output: OutputUploadMetadata {
      run_id: 17,
      task_id: 23,
      kind: OutputKind::Report,
      name: "tests".to_owned(),
      content_type: None,
      report_format: Some("junit".to_owned()),
      transport_content_type: "application/octet-stream".to_owned(),
      size_bytes: 12,
      sha256: "0".repeat(64),
    },
  };
  request.validate().unwrap();
  let response = BeginOutputUploadResponse {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: request.request_id.clone(),
    upload_id: "output-1".to_owned(),
    put_url: "https://objects.example/upload?signature=opaque".to_owned(),
    required_headers: BTreeMap::from([("x-amz-checksum-sha256".to_owned(), "opaque".to_owned())]),
    expires_at: 200,
  };
  response.validate(&request.request_id, 100).unwrap();

  let mut unsafe_response = response;
  unsafe_response.required_headers = BTreeMap::from([("authorization".to_owned(), "secret".to_owned())]);
  assert!(unsafe_response.validate(&request.request_id, 100).is_err());

  let complete = CompleteOutputUploadRequest {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: "request-2".to_owned(),
    registration_id: "registration-1".to_owned(),
    lease,
    upload_id: "output-1".to_owned(),
  };
  complete.validate().unwrap();
  CompleteOutputUploadResponse {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: complete.request_id.clone(),
    upload_id: complete.upload_id.clone(),
  }
  .validate(&complete.request_id, &complete.upload_id)
  .unwrap();
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

  let events: AppendEventsRequest = serde_json::from_str(include_str!(
    "../../../../protocol/coordinator/events-append-request-v1.json"
  ))
  .unwrap();
  events.validate().unwrap();

  let events_response: AppendEventsResponse = serde_json::from_str(include_str!(
    "../../../../protocol/coordinator/events-append-response-v1.json"
  ))
  .unwrap();
  events_response.validate("events-20260911-1", 1, 2).unwrap();

  let upload: BeginOutputUploadRequest = serde_json::from_str(include_str!(
    "../../../../protocol/coordinator/output-begin-request-v1.json"
  ))
  .unwrap();
  upload.validate().unwrap();

  let upload_response: BeginOutputUploadResponse = serde_json::from_str(include_str!(
    "../../../../protocol/coordinator/output-begin-response-v1.json"
  ))
  .unwrap();
  upload_response
    .validate("upload-begin-20260911-1", 1_789_056_100)
    .unwrap();

  let upload_complete: CompleteOutputUploadRequest = serde_json::from_str(include_str!(
    "../../../../protocol/coordinator/output-complete-request-v1.json"
  ))
  .unwrap();
  upload_complete.validate().unwrap();

  let upload_complete_response: CompleteOutputUploadResponse = serde_json::from_str(include_str!(
    "../../../../protocol/coordinator/output-complete-response-v1.json"
  ))
  .unwrap();
  upload_complete_response
    .validate("upload-complete-20260911-1", "upload-42-artifact-1")
    .unwrap();

  let completion: CompleteLeaseRequest = serde_json::from_str(include_str!(
    "../../../../protocol/coordinator/complete-request-v1.json"
  ))
  .unwrap();
  completion.validate().unwrap();

  let completion_response: CompleteLeaseResponse = serde_json::from_str(include_str!(
    "../../../../protocol/coordinator/complete-response-v1.json"
  ))
  .unwrap();
  completion_response
    .validate("complete-20260911-1", "completion-42-1")
    .unwrap();
}

#[test]
fn event_batches_are_fenced_contiguous_and_resource_samples_are_sane() {
  let lease = LeaseFence::from(&lease());
  let mut request = AppendEventsRequest {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: "events-1".to_owned(),
    registration_id: "registration-1".to_owned(),
    lease: lease.clone(),
    events: vec![AttemptEventEnvelope {
      job_id: lease.job_id.clone(),
      attempt: lease.attempt,
      lease_id: lease.lease_id.clone(),
      fencing_token: lease.fencing_token.clone(),
      stream_sequence: 1,
      kind: AttemptEventKind::Agent {
        event: AgentLifecycleEvent::ResourceUsage {
          usage: ResourceUsageSnapshot {
            observed_at_unix_ms: 1,
            elapsed_ms: 2,
            cpu_time_ms: 1,
            memory_current_bytes: 3,
            memory_peak_bytes: 4,
            disk_current_bytes: 5,
            disk_peak_bytes: 6,
            io_read_bytes: 7,
            io_written_bytes: 8,
            network_received_bytes: None,
            network_transmitted_bytes: None,
          },
        },
      },
    }],
  };
  request.validate().unwrap();
  let mut duplicate = request.events[0].clone();
  duplicate.stream_sequence = 3;
  request.events.push(duplicate);
  assert!(request.validate().unwrap_err().to_string().contains("contiguous"));
  request.events[1].stream_sequence = 2;
  request.events[1].fencing_token = "stale-fence".to_owned();
  assert!(request.validate().unwrap_err().to_string().contains("fence"));
}

#[test]
fn append_metadata_reserve_covers_maximally_escaped_bounded_identifiers() {
  // Quotes are valid identifier bytes and have the largest common JSON escape
  // expansion. Use every allowed record so DTO growth cannot silently make the
  // configuration preflight underestimate a real append request.
  let identifier = "\"".repeat(MAX_COORDINATOR_IDENTIFIER_BYTES);
  let fence = LeaseFence {
    lease_id: identifier.clone(),
    job_id: identifier.clone(),
    attempt: u32::MAX,
    fencing_token: identifier.clone(),
  };
  let events = (1..=MAX_EVENT_BATCH_RECORDS as u64)
    .map(|stream_sequence| AttemptEventEnvelope {
      job_id: fence.job_id.clone(),
      attempt: fence.attempt,
      lease_id: fence.lease_id.clone(),
      fencing_token: fence.fencing_token.clone(),
      stream_sequence,
      kind: AttemptEventKind::Agent {
        event: AgentLifecycleEvent::StateChanged {
          state: JobLifecycleState::Running,
        },
      },
    })
    .collect::<Vec<_>>();
  let request = AppendEventsRequest {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: identifier.clone(),
    registration_id: identifier,
    lease: fence,
    events,
  };
  request.validate().unwrap();
  let encoded_event_bytes = request
    .events
    .iter()
    .map(|event| serde_json::to_vec(event).unwrap().len() + 1)
    .sum::<usize>();
  let encoded_request_bytes = serde_json::to_vec(&request).unwrap().len();

  assert!(encoded_request_bytes.saturating_sub(encoded_event_bytes) <= MAX_APPEND_REQUEST_OVERHEAD_BYTES);
}

#[test]
fn rejects_invalid_inventory_and_registration_boundaries() {
  let valid = inventory();
  let mut invalid = valid.clone();
  invalid.coordinator_protocols = vec![2];
  assert!(invalid.validate().unwrap_err().to_string().contains("protocol v1"));
  invalid = valid.clone();
  invalid.host_capacity.logical_cpu_count = 0;
  assert!(invalid.validate().unwrap_err().to_string().contains("capacity"));
  invalid = valid.clone();
  invalid.runtimes.clear();
  assert!(invalid.validate().unwrap_err().to_string().contains("runtime"));
  invalid = valid.clone();
  invalid.runtimes.push(invalid.runtimes[0].clone());
  assert!(invalid.validate().unwrap_err().to_string().contains("duplicates"));
  invalid = valid.clone();
  invalid.runtimes[0].mode = RuntimeMode::Native;
  assert!(invalid.validate().unwrap_err().to_string().contains("Native"));
  invalid = valid.clone();
  invalid.octa.octafile_versions.clear();
  assert!(
    invalid
      .validate()
      .unwrap_err()
      .to_string()
      .contains("octafile_versions")
  );
  invalid = valid.clone();
  invalid.octa.plugins.push(invalid.octa.plugins[0].clone());
  assert!(
    invalid
      .validate()
      .unwrap_err()
      .to_string()
      .contains("task plugin names")
  );
  invalid = valid.clone();
  invalid.source_plugins.push(invalid.source_plugins[0].clone());
  assert!(
    invalid
      .validate()
      .unwrap_err()
      .to_string()
      .contains("source plugin names")
  );

  let mut task = valid.octa.plugins[0].clone();
  task.protocol = 0;
  assert!(task.validate().unwrap_err().to_string().contains("protocol"));
  task = valid.octa.plugins[0].clone();
  task.platforms.clear();
  assert!(task.validate().unwrap_err().to_string().contains("must not be empty"));
  let mut source = valid.source_plugins[0].clone();
  source.protocol_min = 2;
  source.protocol_max = 1;
  assert!(source.validate().unwrap_err().to_string().contains("range"));

  let mut registration = RegisterAgentResponse {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: "request-1".to_owned(),
    registration_id: "registration-1".to_owned(),
    max_retry_delay_ms: 1,
  };
  registration.validate("request-1").unwrap();
  registration.max_retry_delay_ms = 0;
  assert!(
    registration
      .validate("request-1")
      .unwrap_err()
      .to_string()
      .contains("greater")
  );
  registration.max_retry_delay_ms = 1;
  registration.protocol_version += 1;
  assert!(
    registration
      .validate("request-1")
      .unwrap_err()
      .to_string()
      .contains("unsupported")
  );

  let mut acquire = AcquireLeaseRequest {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: "request-1".to_owned(),
    registration_id: "registration-1".to_owned(),
    wait_seconds: 0,
  };
  assert!(acquire.validate().unwrap_err().to_string().contains("wait_seconds"));
  acquire.wait_seconds = 1;
  acquire.validate().unwrap();
}

#[test]
fn validates_no_work_drain_fences_and_resource_boundaries() {
  let no_work = AcquireLeaseResponse::NoWork {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: "request-1".to_owned(),
    retry_after_ms: 1,
  };
  no_work.validate("request-1", 100, 10).unwrap();
  let invalid_no_work = AcquireLeaseResponse::NoWork {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: "request-1".to_owned(),
    retry_after_ms: 0,
  };
  assert!(invalid_no_work.validate("request-1", 100, 10).is_err());
  AcquireLeaseResponse::Drain {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: "request-1".to_owned(),
  }
  .validate("request-1", 100, 10)
  .unwrap();

  let mut invalid_lease = lease();
  invalid_lease.issued_at = 121;
  assert!(
    invalid_lease
      .validate(120, 10)
      .unwrap_err()
      .to_string()
      .contains("interval")
  );
  invalid_lease = lease();
  invalid_lease.attempt = 0;
  assert!(
    invalid_lease
      .validate(120, 10)
      .unwrap_err()
      .to_string()
      .contains("attempt")
  );
  assert!(lease().validate(u64::MAX, 1).is_err());

  let valid_usage = ResourceUsageSnapshot {
    observed_at_unix_ms: 1,
    elapsed_ms: 2,
    cpu_time_ms: 1,
    memory_current_bytes: 3,
    memory_peak_bytes: 4,
    disk_current_bytes: 5,
    disk_peak_bytes: 6,
    io_read_bytes: 7,
    io_written_bytes: 8,
    network_received_bytes: None,
    network_transmitted_bytes: None,
  };
  let mut invalid_usage = valid_usage.clone();
  invalid_usage.observed_at_unix_ms = 0;
  assert!(invalid_usage.validate().unwrap_err().to_string().contains("timestamp"));
  invalid_usage = valid_usage.clone();
  invalid_usage.memory_current_bytes = 5;
  assert!(invalid_usage.validate().unwrap_err().to_string().contains("peak"));

  let mut active = ActiveJob {
    job_id: "job-1".to_owned(),
    attempt: 1,
    lease_id: "lease-1".to_owned(),
    resource_usage: Some(valid_usage),
  };
  active.validate().unwrap();
  active.attempt = 0;
  assert!(active.validate().unwrap_err().to_string().contains("attempt"));
}

#[test]
fn rejects_invalid_event_completion_health_and_error_responses() {
  let mut append: AppendEventsRequest = serde_json::from_str(include_str!(
    "../../../../protocol/coordinator/events-append-request-v1.json"
  ))
  .unwrap();
  let fence = append.lease.clone();
  append.events[0].stream_sequence = 0;
  assert!(
    append.events[0]
      .validate(&fence)
      .unwrap_err()
      .to_string()
      .contains("stream_sequence")
  );
  append.events[0].stream_sequence = 1;
  append.events[0].kind = AttemptEventKind::Runner {
    event: RunnerEventPayload {
      schema_version: 0,
      sequence: 1,
      timestamp: "now".to_owned(),
      category: "stdout".to_owned(),
      data: serde_json::Map::new(),
    },
  };
  assert!(
    append.events[0]
      .validate(&fence)
      .unwrap_err()
      .to_string()
      .contains("header")
  );
  append.events[0].kind = AttemptEventKind::Agent {
    event: AgentLifecycleEvent::AccountingUnavailable {
      consecutive_failures: 0,
    },
  };
  assert!(
    append.events[0]
      .validate(&fence)
      .unwrap_err()
      .to_string()
      .contains("failure count")
  );
  append.events.clear();
  assert!(append.validate().unwrap_err().to_string().contains("batch size"));

  let response = AppendEventsResponse {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: "request-1".to_owned(),
    acknowledged_sequence: 2,
  };
  assert!(response.validate("request-1", 0, 2).is_err());
  assert!(response.validate("request-1", 1, 1).is_err());

  let mut completion: CompleteLeaseRequest = serde_json::from_str(include_str!(
    "../../../../protocol/coordinator/complete-request-v1.json"
  ))
  .unwrap();
  completion.last_event_sequence = 0;
  assert!(
    completion
      .validate()
      .unwrap_err()
      .to_string()
      .contains("acknowledged event")
  );
  let completion_response = CompleteLeaseResponse {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: "request-1".to_owned(),
    completion_id: "completion-1".to_owned(),
  };
  assert!(completion_response.validate("request-1", "completion-2").is_err());

  let mut health = BackendHealth {
    backend: "native".to_owned(),
    status: BackendHealthStatus::Degraded,
    message: Some(String::new()),
  };
  assert!(health.validate().unwrap_err().to_string().contains("health message"));
  health.message = Some("recovering".to_owned());
  health.validate().unwrap();

  let mut error = CoordinatorErrorResponse {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: "request-1".to_owned(),
    code: "unavailable".to_owned(),
    message: "retry".to_owned(),
    retryable: true,
    retry_after_ms: Some(0),
  };
  assert!(
    error
      .validate("request-1")
      .unwrap_err()
      .to_string()
      .contains("retry_after")
  );
  error.retry_after_ms = Some(1);
  error.message = "bad\nmessage".to_owned();
  assert!(error.validate("request-1").unwrap_err().to_string().contains("message"));
}

#[test]
fn upload_response_debug_redacts_the_presigned_capability() {
  let response = BeginOutputUploadResponse {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: "request-1".to_owned(),
    upload_id: "upload-1".to_owned(),
    put_url: "https://storage.example/object?signature=url-secret".to_owned(),
    required_headers: BTreeMap::from([("x-private".to_owned(), "header-secret".to_owned())]),
    expires_at: 1,
  };

  let rendered = format!("{response:?}");
  assert!(!rendered.contains("url-secret"));
  assert!(!rendered.contains("header-secret"));
  assert!(rendered.contains("x-private"));
}
