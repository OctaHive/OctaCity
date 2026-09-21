use std::path::Path;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use octacity_protocol::{
  AcquireLeaseRequest, AcquireLeaseResponse, AgentCredentialToken, AppendEventsRequest, AttemptEventEnvelope,
  AttemptEventKind, CompleteLeaseRequest, JobCompletionStatus, JobLifecycleState, LeaseFence, RegisterAgentRequest,
  RegisterAgentResponse,
};
use octacity_server::{ServerConfig, ServerRuntime};
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

mod support;

use support::{get_json, post_management, publish_policy_and_trigger_definition, resource_id, string};

/// Exercises the released server composition against a disposable PostgreSQL
/// database. The Agent process is represented at its public wire boundary so
/// failures identify the server/application/store seam rather than a local
/// execution backend.
#[tokio::test]
#[ignore = "requires OCTACITY_POSTGRES_URL and a disposable PostgreSQL database"]
async fn management_and_agent_http_complete_a_sequential_pipeline() {
  let postgres_url = std::env::var("OCTACITY_POSTGRES_URL").expect("OCTACITY_POSTGRES_URL must be set");
  let object_endpoint = std::env::var("OCTACITY_MINIO_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:9000".to_owned());
  let directory = tempfile::tempdir().unwrap();
  let config = runtime_config(directory.path(), &postgres_url, &object_endpoint);
  let pool = PgPoolOptions::new()
    .max_connections(4)
    .connect(&postgres_url)
    .await
    .unwrap();
  octacity_server_store_postgres::migrate(&pool).await.unwrap();

  let runtime = ServerRuntime::start(config).await.unwrap();
  let management_origin = format!("http://{}", runtime.management_addr());
  let agent_origin = format!(
    "http://{}",
    runtime.agent_addr().expect("Agent ingress must be enabled")
  );
  let client = Client::new();
  let run = Uuid::new_v4().simple().to_string();

  let pool_resource = post_management(
    &client,
    &management_origin,
    "/api/v1/agent-pools",
    &format!("{run}-pool"),
    json!({
      "name": format!("linux-native-{run}"),
      "definition": {
        "enabled": true,
        "drain_state": "accepting",
        "admission_policy": {
          "mode": "allowlist",
          "platforms": [{"operating_system": "linux", "architecture": "amd64"}]
        },
        "concurrency_limit": 1,
        "fairness_policy": "priority_fifo",
        "static_capacity_limit": 1
      }
    }),
  )
  .await;
  let pool_id = resource_id(&pool_resource);

  let project_resource = post_management(
    &client,
    &management_origin,
    "/api/v1/projects",
    &format!("{run}-project"),
    json!({"parent_id": null, "name": format!("vertical-{run}")}),
  )
  .await;
  let project_id = resource_id(&project_resource);

  let pipeline_resource = post_management(
    &client,
    &management_origin,
    "/api/v1/pipelines",
    &format!("{run}-pipeline"),
    json!({
      "project_id": project_id,
      "name": "release",
      "dag": {
        "nodes": [
          pipeline_node("build", "Build"),
          pipeline_node("test", "Test")
        ],
        "edges": [{"predecessor": "build", "dependent": "test"}]
      }
    }),
  )
  .await;
  let pipeline_id = resource_id(&pipeline_resource);

  let repository_resource = post_management(
    &client,
    &management_origin,
    "/api/v1/repositories",
    &format!("{run}-repository"),
    json!({
      "project_id": project_id,
      "name": "source",
      "definition": {
        "vcs_integration_id": Uuid::new_v4().to_string(),
        "repository_locator": "https://example.invalid/octacity/fixture.git",
        "selection": {
          "allowed_references": [],
          "default_reference": null,
          "allow_exact_revision": true
        }
      }
    }),
  )
  .await;
  let repository_id = resource_id(&repository_resource);

  let configuration_resource = post_management(
    &client,
    &management_origin,
    "/api/v1/build-configurations",
    &format!("{run}-configuration"),
    build_configuration(&project_id, &repository_id, &pipeline_id, &pool_id),
  )
  .await;
  let configuration_id = resource_id(&configuration_resource);

  let trigger_id = publish_policy_and_trigger_definition(
    &client,
    &management_origin,
    &project_id,
    &repository_id,
    &configuration_id,
    &pool_id,
    "native",
  )
  .await;

  let trigger = post_management(
    &client,
    &management_origin,
    "/api/v1/triggers/manual",
    &format!("{run}-trigger"),
    json!({
      "trigger_id": trigger_id,
      "trigger_version": 1,
      "configuration_id": configuration_id,
      "configuration_version": 1,
      "deduplication_identity": format!("{run}-trigger"),
      "source": {"kind": "exact_revision", "value": "0123456789abcdef"},
      "parameters": {},
      "priority": 50
    }),
  )
  .await;
  assert_eq!(trigger["outcome"], "accepted");
  let build_id = string(&trigger, "build_id");
  let attempt_id = string(&trigger, "attempt_id");
  assert_eq!(trigger["ready_job_ids"].as_array().unwrap().len(), 1);

  let enrollment = post_management(
    &client,
    &management_origin,
    "/api/v1/agent-enrollments",
    &format!("{run}-enrollment"),
    json!({
      "pool_id": pool_id,
      "pool_version": 1,
      "expected_platform": {"operating_system": "linux", "architecture": "amd64"}
    }),
  )
  .await;
  let enrollment_credential = string(&enrollment, "credential");

  let mut registration: RegisterAgentRequest = serde_json::from_str(include_str!(
    "../../../../shared/protocol-fixtures/coordinator/register-request-v1.json"
  ))
  .unwrap();
  registration.request_id = format!("{run}-registration");
  registration.inventory.agent_id = format!("linux-builder-{run}");
  let registered: RegisterAgentResponse = protocol_post(
    &client,
    &format!("{agent_origin}/api/v1/agents/register"),
    &registration.request_id,
    &enrollment_credential,
    &registration,
  )
  .await;
  let registration_credential = AgentCredentialToken::parse(&enrollment_credential)
    .unwrap()
    .promote(registered.registration_id.clone())
    .unwrap()
    .encode()
    .to_string();

  let first = acquire(
    &client,
    &agent_origin,
    &registration_credential,
    &registered.registration_id,
    &registration.inventory.agent_id,
    &format!("{run}-lease-1"),
  )
  .await;
  finish_job(
    &client,
    &agent_origin,
    &registration_credential,
    &registered.registration_id,
    first,
    &format!("{run}-first"),
  )
  .await;

  let second = acquire(
    &client,
    &agent_origin,
    &registration_credential,
    &registered.registration_id,
    &registration.inventory.agent_id,
    &format!("{run}-lease-2"),
  )
  .await;
  finish_job(
    &client,
    &agent_origin,
    &registration_credential,
    &registered.registration_id,
    second,
    &format!("{run}-second"),
  )
  .await;

  let build = get_json(&client, format!("{management_origin}/api/v1/builds/{build_id}")).await;
  assert_eq!(build["state"], "succeeded");
  assert_eq!(build["current_attempt"]["id"], attempt_id);
  assert_eq!(build["current_attempt"]["state"], "succeeded");
  let attempt = get_json(&client, format!("{management_origin}/api/v1/attempts/{attempt_id}")).await;
  let jobs = attempt["jobs"].as_array().unwrap();
  assert_eq!(jobs.len(), 2);
  assert!(jobs.iter().all(|job| job["state"] == "succeeded"));
  assert!(jobs.iter().all(|job| job["event_cursor"] == 1));
  for job in jobs {
    let job_id = job["id"].as_str().unwrap();
    let detail = get_json(&client, format!("{management_origin}/api/v1/jobs/{job_id}")).await;
    assert_eq!(detail["terminal"]["state"], "succeeded");
    let events = get_json(
      &client,
      format!("{management_origin}/api/v1/jobs/{job_id}/events?after=0&limit=10&wait_ms=0"),
    )
    .await;
    assert_eq!(events["items"].as_array().unwrap().len(), 1);
  }

  runtime.shutdown().await.unwrap();
}

fn pipeline_node(id: &str, name: &str) -> Value {
  json!({
    "id": id,
    "name": name,
    "dependency_policy": "all_succeeded",
    "required_capabilities": ["native"],
    "execution": {"commands": [id], "parallel": false, "failfast": true}
  })
}

fn build_configuration(project_id: &str, repository_id: &str, pipeline_id: &str, pool_id: &str) -> Value {
  json!({
    "project_id": project_id,
    "name": "release",
    "definition": {
      "enabled": true,
      "job_concurrency_limit": 1,
      "repository_id": repository_id,
      "repository_version": 1,
      "pipeline_id": pipeline_id,
      "pipeline_version": 1,
      "parameters": {"parameters": {}, "deny_unknown": true},
      "triggers": ["manual"],
      "agent_requirements": {
        "capabilities": ["native"],
        "labels": {},
        "minimum_cpu_millis": 1000,
        "minimum_memory_bytes": 1073741824_u64,
        "minimum_disk_bytes": 10737418240_u64
      },
      "allowed_pools": [pool_id],
      "runtime": {
        "class": "native",
        "operating_system": "linux",
        "architecture": "amd64",
        "immutable_image": null,
        "cpu_millis": 1000,
        "memory_bytes": 1073741824_u64,
        "writable_disk_bytes": 10737418240_u64,
        "timeout_seconds": 3600,
        "network": {"mode": "disabled"},
        "workload_identity_profile": null
      },
      "cache": {"namespace": null, "read": false, "write": false},
      "artifacts": {
        "artifact_count": 0,
        "artifact_bytes": 0,
        "report_count": 0,
        "report_bytes": 0,
        "single_output_bytes": 0
      },
      "retry": {"max_attempts": 1, "retry_on": []}
    }
  })
}

async fn acquire(
  client: &Client,
  origin: &str,
  credential: &str,
  registration_id: &str,
  agent_id: &str,
  request_id: &str,
) -> octacity_protocol::LeaseAssignment {
  let response: AcquireLeaseResponse = protocol_post(
    client,
    &format!("{origin}/api/v1/agents/{agent_id}/leases:acquire"),
    request_id,
    credential,
    &AcquireLeaseRequest {
      protocol_version: 1,
      request_id: request_id.to_owned(),
      registration_id: registration_id.to_owned(),
      wait_seconds: 1,
      accept_jobs: true,
      snapshot: octacity_protocol::HostSnapshot {
        available_cpu_millis: 8_000,
        available_memory_bytes: 17_179_869_184,
        work_disk_free_bytes: 107_374_182_400,
        state_disk_free_bytes: 53_687_091_200,
        active_job: None,
        backends: vec![octacity_protocol::BackendHealth {
          backend: "native".to_owned(),
          status: octacity_protocol::BackendHealthStatus::Ready,
          message: None,
        }],
      },
    },
  )
  .await;
  match response {
    AcquireLeaseResponse::Lease { lease, .. } => lease,
    other => panic!("expected a Lease, got {other:?}"),
  }
}

async fn finish_job(
  client: &Client,
  origin: &str,
  credential: &str,
  registration_id: &str,
  lease: octacity_protocol::LeaseAssignment,
  identity: &str,
) {
  let fence = LeaseFence::from(&lease);
  let append_request = AppendEventsRequest {
    protocol_version: 1,
    request_id: format!("{identity}-events"),
    registration_id: registration_id.to_owned(),
    lease: fence.clone(),
    events: vec![AttemptEventEnvelope {
      job_id: lease.job_id.clone(),
      attempt: lease.attempt,
      lease_id: lease.lease_id.clone(),
      fencing_token: lease.fencing_token.clone(),
      stream_sequence: 1,
      occurred_at_unix_ms: unix_now_millis(),
      kind: AttemptEventKind::Agent {
        event: octacity_protocol::AgentLifecycleEvent::StateChanged {
          state: JobLifecycleState::Completing,
        },
      },
    }],
  };
  let acknowledged: octacity_protocol::AppendEventsResponse = protocol_post(
    client,
    &format!("{origin}/api/v1/leases/{}/events:append", lease.lease_id),
    &append_request.request_id,
    credential,
    &append_request,
  )
  .await;
  assert_eq!(acknowledged.acknowledged_sequence, 1);

  let complete_request = CompleteLeaseRequest {
    protocol_version: 1,
    request_id: format!("{identity}-complete"),
    registration_id: registration_id.to_owned(),
    lease: fence,
    completion_id: format!("{identity}-completion"),
    last_event_sequence: 1,
    status: JobCompletionStatus::Succeeded,
    final_usage: None,
    results: Vec::new(),
  };
  let _: octacity_protocol::CompleteLeaseResponse = protocol_post(
    client,
    &format!("{origin}/api/v1/leases/{}/complete", lease.lease_id),
    &complete_request.request_id,
    credential,
    &complete_request,
  )
  .await;
}

async fn protocol_post<T, R>(client: &Client, url: &str, request_id: &str, credential: &str, body: &T) -> R
where
  T: serde::Serialize + ?Sized,
  R: serde::de::DeserializeOwned,
{
  let response = client
    .post(url)
    .header("idempotency-key", request_id)
    .bearer_auth(credential)
    .json(body)
    .send()
    .await
    .unwrap();
  let status = response.status();
  let bytes = response.bytes().await.unwrap();
  assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&bytes));
  serde_json::from_slice(&bytes).unwrap()
}

fn unix_now_millis() -> i64 {
  i64::try_from(
    std::time::SystemTime::now()
      .duration_since(std::time::SystemTime::UNIX_EPOCH)
      .unwrap()
      .as_millis(),
  )
  .unwrap()
}

fn runtime_config(directory: &Path, postgres_url: &str, object_endpoint: &str) -> ServerConfig {
  let postgres_url_file = directory.join("postgres-url");
  let access_key = directory.join("object-access-key");
  let secret_key = directory.join("object-secret-key");
  let signing_key = directory.join("signing-key");
  let enrollment_key = directory.join("agent-enrollment-key");
  let job_spec_policy = directory.join("job-spec-policy.json");
  std::fs::write(&postgres_url_file, postgres_url).unwrap();
  std::fs::write(&access_key, "octacity").unwrap();
  std::fs::write(&secret_key, "octacity-secret").unwrap();
  std::fs::write(&signing_key, STANDARD.encode([7_u8; 32])).unwrap();
  std::fs::write(&enrollment_key, STANDARD.encode([8_u8; 32])).unwrap();
  std::fs::write(
    &job_spec_policy,
    r#"{"source":{"provider":"git","plugin_version":"0.1.0","plugin_sha256":"3333333333333333333333333333333333333333333333333333333333333333","repository_parameter":"url"},"octa":{"version":"0.3.0","runner_sha256":"1111111111111111111111111111111111111111111111111111111111111111","runner_protocol":1,"event_schema":1,"plugin_protocol":1,"plugin_digests":{"shell":"2222222222222222222222222222222222222222222222222222222222222222"}},"validity":3600}"#,
  )
  .unwrap();
  #[cfg(unix)]
  for path in [
    &postgres_url_file,
    &access_key,
    &secret_key,
    &signing_key,
    &enrollment_key,
  ] {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
  }
  let path = |path: &Path| path.display().to_string().replace('\\', "\\\\");
  ServerConfig::parse_toml(&format!(
    r#"
management_bind = "127.0.0.1:0"
agent_bind = "127.0.0.1:0"
shutdown_grace_milliseconds = 3000
readiness_check_interval_milliseconds = 50
readiness_check_timeout_milliseconds = 1000
agent_registration_lifetime_milliseconds = 600000
agent_enrollment_lifetime_milliseconds = 600000
agent_lease_lifetime_milliseconds = 60000
supported_pipeline_capabilities = ["native"]

[postgres]
url_file = "{}"

[object_storage]
endpoint = "{object_endpoint}"
region = "us-east-1"
bucket = "octacity-artifacts"
access_key_file = "{}"
secret_key_file = "{}"

[signing]
key_id = "test-key"
key_file = "{}"

[agent_credentials]
enrollment_key_file = "{}"

[job_spec]
policy_file = "{}"
"#,
    path(&postgres_url_file),
    path(&access_key),
    path(&secret_key),
    path(&signing_key),
    path(&enrollment_key),
    path(&job_spec_policy),
  ))
  .unwrap()
}
