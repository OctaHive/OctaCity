use base64::{Engine as _, engine::general_purpose::STANDARD};
use octacity_server::{ServerConfig, ServerRuntime};
use reqwest::StatusCode;

#[test]
fn configuration_is_strict_and_validated_before_startup() {
  let valid = ServerConfig::parse_toml(valid_configuration()).unwrap();
  assert_eq!(valid.management_bind().port(), 0);

  assert!(ServerConfig::parse_toml("management_bind = \"127.0.0.1:0\"").is_err());

  let unknown = r#"
management_bind = "127.0.0.1:0"
shutdown_grace_milliseconds = 1000
supported_pipeline_capabilities = ["native"]
surprise = true

[postgres]
url_file = "postgres-url"

[object_storage]
endpoint = "http://127.0.0.1:9000"
region = "us-east-1"
bucket = "octacity-artifacts"
access_key_file = "object-access-key"
secret_key_file = "object-secret-key"

[signing]
key_id = "test-key"
key_file = "signing-key"

[agent_credentials]
enrollment_key_file = "agent-enrollment-key"
"#;
  assert!(ServerConfig::parse_toml(unknown).is_err());

  let unsupported_dynamic_provisioner = valid_configuration().replace(
    "supported_pipeline_capabilities = [\"native\"]",
    "supported_pipeline_capabilities = [\"native\"]\nagent_provisioning_adapter_registry = \"provisioning-adapters\"",
  );
  assert!(matches!(
    ServerConfig::parse_toml(&unsupported_dynamic_provisioner),
    Err(octacity_server::ServerConfigError::Parse { .. })
  ));

  let zero_grace = r#"
management_bind = "127.0.0.1:0"
shutdown_grace_milliseconds = 0

[postgres]
url_file = "postgres-url"

[object_storage]
endpoint = "http://127.0.0.1:9000"
region = "us-east-1"
bucket = "octacity-artifacts"
access_key_file = "object-access-key"
secret_key_file = "object-secret-key"

[signing]
key_id = "test-key"
key_file = "signing-key"

[agent_credentials]
enrollment_key_file = "agent-enrollment-key"
"#;
  assert!(ServerConfig::parse_toml(zero_grace).is_err());

  let insecure_remote_storage = valid_configuration().replace("http://127.0.0.1:9000", "http://objects.example");
  assert!(ServerConfig::parse_toml(&insecure_remote_storage).is_err());

  let unsafe_bucket = valid_configuration().replace("octacity-artifacts", "INVALID_BUCKET");
  assert!(ServerConfig::parse_toml(&unsafe_bucket).is_err());

  let invalid_signing_key_id = valid_configuration().replace("key_id = \"test-key\"", "key_id = \"\"");
  assert!(ServerConfig::parse_toml(&invalid_signing_key_id).is_err());

  let insecure_cache_endpoint = valid_configuration().replace("https://cache.example", "http://cache.example");
  assert!(ServerConfig::parse_toml(&insecure_cache_endpoint).is_err());

  let disabled_cache_session = valid_configuration().replace(
    "session_lifetime_milliseconds = 300000",
    "session_lifetime_milliseconds = 0",
  );
  assert!(ServerConfig::parse_toml(&disabled_cache_session).is_err());

  let disabled_capability_recheck = valid_configuration().replace(
    "secret_key_file = \"object-secret-key\"",
    "secret_key_file = \"object-secret-key\"\ncapability_recheck_interval_milliseconds = 0",
  );
  assert!(ServerConfig::parse_toml(&disabled_capability_recheck).is_err());

  for setting in [
    "upload_capability_lifetime_milliseconds = 0",
    "download_capability_lifetime_milliseconds = 3600001",
  ] {
    let invalid_lifetime = valid_configuration().replace(
      "secret_key_file = \"object-secret-key\"",
      &format!("secret_key_file = \"object-secret-key\"\n{setting}"),
    );
    assert!(ServerConfig::parse_toml(&invalid_lifetime).is_err());
  }

  let disabled_listener_reconnect = valid_configuration().replace(
    "supported_pipeline_capabilities = [\"native\"]",
    "supported_pipeline_capabilities = [\"native\"]\nready_job_listener_reconnect_milliseconds = 0",
  );
  assert!(ServerConfig::parse_toml(&disabled_listener_reconnect).is_err());

  for setting in [
    "agent_telemetry_export_timeout_milliseconds = 0",
    "agent_telemetry_max_in_flight_exports = 65",
  ] {
    let unbounded_telemetry = valid_configuration().replace(
      "supported_pipeline_capabilities = [\"native\"]",
      &format!("supported_pipeline_capabilities = [\"native\"]\n{setting}"),
    );
    assert!(ServerConfig::parse_toml(&unbounded_telemetry).is_err());
  }

  for setting in [
    "window_milliseconds = 0",
    "tracked_identities = 0",
    "management_requests = 10\nexpensive_management_requests = 11",
    "agent_requests = 100001",
    "agent_heartbeats = 0",
    "webhook_requests = 0",
  ] {
    let unbounded_admission = valid_configuration().replace(
      "supported_pipeline_capabilities = [\"native\"]",
      &format!("supported_pipeline_capabilities = [\"native\"]\n\n[admission]\n{setting}"),
    );
    assert!(ServerConfig::parse_toml(&unbounded_admission).is_err());
  }

  let oversized = "x".repeat(1024 * 1024 + 1);
  assert!(matches!(
    ServerConfig::parse_toml(&oversized),
    Err(octacity_server::ServerConfigError::TooLarge { .. })
  ));
}

#[tokio::test]
async fn external_unauthenticated_management_requires_acknowledgement_before_bind() {
  let reservation = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = reservation.local_addr().unwrap().port();
  drop(reservation);
  let external = valid_configuration().replace("127.0.0.1:0", &format!("0.0.0.0:{port}"));

  let error = ServerConfig::parse_toml(&external).unwrap_err();
  assert!(error.to_string().contains("acknowledge_unauthenticated_management"));
  let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, port))
    .await
    .expect("configuration rejection must happen before any listener bind");
  drop(listener);

  let acknowledged = external.replace(
    &format!("management_bind = \"0.0.0.0:{port}\""),
    &format!("management_bind = \"0.0.0.0:{port}\"\nacknowledge_unauthenticated_management = true"),
  );
  let config = ServerConfig::parse_toml(&acknowledged).unwrap();
  assert!(config.management_externally_reachable());
  assert!(config.unauthenticated_management_acknowledged());
}

#[test]
fn management_and_agent_ingress_are_independently_configured() {
  let configured = valid_configuration().replace(
    "management_bind = \"127.0.0.1:0\"",
    "management_bind = \"127.0.0.1:0\"\nagent_bind = \"127.0.0.1:0\"",
  );
  let config = ServerConfig::parse_toml(&configured).unwrap();
  assert_eq!(config.agent_bind().unwrap().ip(), std::net::Ipv4Addr::LOCALHOST);

  let overlapping = valid_configuration().replace(
    "management_bind = \"127.0.0.1:0\"",
    "management_bind = \"127.0.0.1:8080\"\nagent_bind = \"0.0.0.0:8080\"\nacknowledge_unauthenticated_management = true",
  );
  let error = ServerConfig::parse_toml(&overlapping).unwrap_err();
  assert!(error.to_string().contains("independently bindable"));
}

#[test]
fn webhook_ingress_requires_its_authenticated_configuration() {
  let configured = valid_configuration().replace(
    "management_bind = \"127.0.0.1:0\"",
    "management_bind = \"127.0.0.1:0\"\nwebhook_bind = \"127.0.0.1:0\"",
  );
  let error = ServerConfig::parse_toml(&configured).unwrap_err();
  assert!(error.to_string().contains("must be configured together"));

  let configured = valid_configuration().replace(
    "management_bind = \"127.0.0.1:0\"",
    "management_bind = \"127.0.0.1:0\"\nwebhook_bind = \"127.0.0.1:0\"\nwebhook_public_base_url = \"https://hooks.example.test\"\nwebhook_adapter_registry = \"webhook-adapters\"",
  );
  let config = ServerConfig::parse_toml(&configured).unwrap();
  assert_eq!(config.webhook_bind().unwrap().ip(), std::net::Ipv4Addr::LOCALHOST);
  assert_eq!(
    config.webhook_callback_origin().as_ref().map(|origin| origin.as_str()),
    Some("https://hooks.example.test")
  );

  for invalid_origin in [
    "http://hooks.example.test",
    "https://user@hooks.example.test",
    "https://hooks.example.test/path",
    "https://hooks.example.test/",
    "https://hooks.example.test:invalid",
  ] {
    let invalid = configured.replace("https://hooks.example.test", invalid_origin);
    assert!(ServerConfig::parse_toml(&invalid).is_err());
  }

  let loopback_development = configured.replace("https://hooks.example.test", "http://127.0.0.1:8082");
  ServerConfig::parse_toml(&loopback_development).expect("loopback HTTP must remain available for development");
}

#[test]
fn adapter_configuration_is_complete_unique_and_claim_safe() {
  let registry_only = valid_configuration().replace(
    "supported_pipeline_capabilities = [\"native\"]",
    "supported_pipeline_capabilities = [\"native\"]\nvcs_adapter_registry = \"vcs-adapters\"",
  );
  assert!(ServerConfig::parse_toml(&registry_only).is_err());

  let integration = r#"
vcs_adapter_registry = "vcs-adapters"

[[vcs_integrations]]
integration_id = "00000000-0000-0000-0000-000000000201"
adapter_id = "git"
adapter_sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
credential_handle = "secret:repository"
"#;
  let configured = valid_configuration().replace(
    "supported_pipeline_capabilities = [\"native\"]",
    &format!("supported_pipeline_capabilities = [\"native\"]\n{integration}"),
  );
  let parsed = ServerConfig::parse_toml(&configured).unwrap();
  let debug = format!("{parsed:?}");
  assert!(!debug.contains("secret:repository"));
  assert!(debug.contains("<redacted>"));

  let duplicate = configured.replace(
    "credential_handle = \"secret:repository\"",
    "credential_handle = \"secret:repository\"\n\n[[vcs_integrations]]\nintegration_id = \"00000000-0000-0000-0000-000000000201\"\nadapter_id = \"git\"\nadapter_sha256 = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\ncredential_handle = \"secret:other\"",
  );
  assert!(ServerConfig::parse_toml(&duplicate).is_err());

  let unsafe_claim = valid_configuration().replace(
    "supported_pipeline_capabilities = [\"native\"]",
    "supported_pipeline_capabilities = [\"native\"]\nwebhook_operation_timeout_milliseconds = 5000\nwebhook_cancellation_grace_milliseconds = 1000\nwebhook_delivery_batch_size = 1\nwebhook_worker_claim_lifetime_milliseconds = 7000",
  );
  let error = ServerConfig::parse_toml(&unsafe_claim).unwrap_err();
  assert!(error.to_string().contains("sequential adapter budget"));
}

#[test]
fn legacy_delivery_named_webhook_worker_settings_remain_accepted() {
  let legacy = valid_configuration().replace(
    "supported_pipeline_capabilities = [\"native\"]",
    "supported_pipeline_capabilities = [\"native\"]\nwebhook_delivery_poll_interval_milliseconds = 1000\nwebhook_delivery_claim_lifetime_milliseconds = 30000\nwebhook_delivery_max_attempts = 5\nwebhook_delivery_initial_retry_milliseconds = 1000\nwebhook_delivery_maximum_retry_milliseconds = 60000",
  );

  ServerConfig::parse_toml(&legacy).expect("legacy webhook worker names must remain compatible");
}

fn valid_configuration() -> &'static str {
  r#"
management_bind = "127.0.0.1:0"
shutdown_grace_milliseconds = 1000
supported_pipeline_capabilities = ["native"]

[postgres]
url_file = "postgres-url"

[object_storage]
endpoint = "http://127.0.0.1:9000"
region = "us-east-1"
bucket = "octacity-artifacts"
access_key_file = "object-access-key"
secret_key_file = "object-secret-key"

[signing]
key_id = "test-key"
key_file = "signing-key"

[agent_credentials]
enrollment_key_file = "agent-enrollment-key"

[cache]
endpoint = "https://cache.example"
credential_key_file = "cache-credential-key"
session_lifetime_milliseconds = 300000

[job_spec]
policy_file = "job-spec-policy.json"
"#
}

#[test]
fn configuration_file_limit_is_enforced_on_bytes_read_from_one_handle() {
  let directory = tempfile::tempdir().unwrap();
  let path = directory.path().join("oversized.toml");
  std::fs::write(&path, vec![b'x'; 1024 * 1024 + 1]).unwrap();

  assert!(matches!(
    ServerConfig::load(&path),
    Err(octacity_server::ServerConfigError::TooLarge { .. })
  ));
}

#[tokio::test]
async fn configured_dependency_failure_keeps_only_readiness_unavailable() {
  let unavailable = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let directory = tempfile::tempdir().unwrap();
  let config = production_test_config(directory.path(), unavailable.local_addr().unwrap().port());
  let runtime = ServerRuntime::start(config).await.unwrap();
  let client = reqwest::Client::new();
  let origin = format!("http://{}", runtime.management_addr());

  assert_eq!(
    client
      .get(format!("{origin}/health/ready"))
      .send()
      .await
      .unwrap()
      .status(),
    StatusCode::SERVICE_UNAVAILABLE
  );
  assert_eq!(
    client
      .get(format!("{origin}/health/live"))
      .send()
      .await
      .unwrap()
      .status(),
    StatusCode::OK
  );
  let openapi: serde_json::Value = client
    .get(format!("{origin}/api/v1/openapi.json"))
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap()
    .json()
    .await
    .unwrap();
  assert!(openapi["paths"]["/api/v1/builds/{build_id}"].is_object());
  assert_eq!(
    client
      .post(format!("{origin}/api/v1/projects"))
      .header("idempotency-key", "runtime-composition-check")
      .json(&serde_json::json!({"parent_id": null, "name": "Runtime check"}))
      .send()
      .await
      .unwrap()
      .status(),
    StatusCode::SERVICE_UNAVAILABLE,
    "the production listener must dispatch through the PostgreSQL application instead of returning 404"
  );
  let registration: octacity_protocol::RegisterAgentRequest = serde_json::from_str(include_str!(
    "../../../shared/protocol-fixtures/coordinator/register-request-v1.json"
  ))
  .unwrap();
  let agent_origin = format!("http://{}", runtime.agent_addr().unwrap());
  let response = client
    .post(format!("{agent_origin}/api/v1/agents/register"))
    .header(
      reqwest::header::AUTHORIZATION,
      "Bearer enrollment.00000000-0000-0000-0000-000000000001.BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc",
    )
    .header("idempotency-key", &registration.request_id)
    .json(&registration)
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
  let error: octacity_protocol::CoordinatorErrorResponse = response.json().await.unwrap();
  assert_eq!(error.code, "unavailable");

  runtime.shutdown().await.unwrap();
}

fn production_test_config(directory: &std::path::Path, unavailable_port: u16) -> ServerConfig {
  ServerConfig::parse_toml(&production_test_config_source(directory, unavailable_port)).unwrap()
}

fn production_test_config_source(directory: &std::path::Path, unavailable_port: u16) -> String {
  let postgres_url = directory.join("postgres-url");
  let access_key = directory.join("object-access-key");
  let secret_key = directory.join("object-secret-key");
  let signing_key = directory.join("signing-key");
  let enrollment_key = directory.join("agent-enrollment-key");
  let cache_key = directory.join("cache-credential-key");
  let job_spec_policy = directory.join("job-spec-policy.json");
  std::fs::write(
    &postgres_url,
    format!("postgres://octacity:secret@127.0.0.1:{unavailable_port}/octacity"),
  )
  .unwrap();
  std::fs::write(&access_key, "access").unwrap();
  std::fs::write(&secret_key, "secret").unwrap();
  std::fs::write(&signing_key, STANDARD.encode([7_u8; 32])).unwrap();
  std::fs::write(&enrollment_key, STANDARD.encode([8_u8; 32])).unwrap();
  std::fs::write(&cache_key, STANDARD.encode([9_u8; 32])).unwrap();
  std::fs::write(
    &job_spec_policy,
    r#"{"source":{"provider":"git","plugin_version":"1.0.0","plugin_sha256":"0000000000000000000000000000000000000000000000000000000000000000","repository_parameter":"url"},"octa":{"version":"1.0.0","runner_sha256":"0000000000000000000000000000000000000000000000000000000000000000","runner_protocol":1,"event_schema":1,"plugin_protocol":1,"plugin_digests":{}},"validity":3600}"#,
  )
  .unwrap();
  #[cfg(unix)]
  for path in [
    &postgres_url,
    &access_key,
    &secret_key,
    &signing_key,
    &enrollment_key,
    &cache_key,
  ] {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
  }

  let path = |path: &std::path::Path| path.display().to_string().replace('\\', "\\\\");
  format!(
    r#"
management_bind = "127.0.0.1:0"
agent_bind = "127.0.0.1:0"
shutdown_grace_milliseconds = 1000
supported_pipeline_capabilities = ["native"]
readiness_check_interval_milliseconds = 10
readiness_check_timeout_milliseconds = 100

[postgres]
url_file = "{}"

[object_storage]
endpoint = "http://127.0.0.1:{unavailable_port}"
region = "us-east-1"
bucket = "octacity-artifacts"
access_key_file = "{}"
secret_key_file = "{}"

[signing]
key_id = "test-key"
key_file = "{}"

[agent_credentials]
enrollment_key_file = "{}"

[cache]
endpoint = "https://cache.example"
credential_key_file = "{}"
session_lifetime_milliseconds = 300000

[job_spec]
policy_file = "{}"
"#,
    path(&postgres_url),
    path(&access_key),
    path(&secret_key),
    path(&signing_key),
    path(&enrollment_key),
    path(&cache_key),
    path(&job_spec_policy),
  )
}

#[test]
fn issue_mcp_connections_require_explicit_project_read_scope_and_private_credential_references() {
  let reader = r#"
[[issue_mcp_readers]]
id = "project-triage"
project_id = "0199a6f4-d56c-7440-9aa2-6a320f862795"
credential_file = "/run/secrets/issue-mcp-triage"
expires_at_unix_ms = 2000000000000
issue_ids = ["0199a6f4-d56c-7440-9aa2-6a320f862799"]
"#;
  assert!(ServerConfig::parse_toml(&format!("{}{reader}", valid_configuration())).is_ok());
  for invalid in [
    reader.replace("project-triage", "invalid reader"),
    reader.replace("/run/secrets/issue-mcp-triage", "relative-token"),
    reader.replace("expires_at_unix_ms = 2000000000000", "expires_at_unix_ms = 0"),
    reader.replace("project_id = \"0199a6f4-d56c-7440-9aa2-6a320f862795\"", ""),
    format!("{reader}{reader}"),
    format!("{reader}write = true\n"),
    format!("{reader}token = \"must-never-be-inline\"\n"),
  ] {
    assert!(ServerConfig::parse_toml(&format!("{}{invalid}", valid_configuration())).is_err());
  }
}

#[tokio::test]
async fn configured_issue_mcp_verifies_its_own_private_tokens_expiry_rotation_and_revocation() {
  let unavailable = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let directory = tempfile::tempdir().unwrap();
  let source = production_test_config_source(directory.path(), unavailable.local_addr().unwrap().port());
  let active_path = directory.path().join("issue-mcp-active");
  let expired_path = directory.path().join("issue-mcp-expired");
  let active = format!("ocmcp.project-triage.{}", "a".repeat(64));
  let rotated = format!("ocmcp.project-triage.{}", "b".repeat(64));
  let expired = format!("ocmcp.expired-reader.{}", "c".repeat(64));
  for (path, token) in [(&active_path, &active), (&expired_path, &expired)] {
    std::fs::write(path, format!("{token}\n")).unwrap();
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt as _;
      std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
  }
  let mut source = source;
  for (id, path, expiry) in [
    ("project-triage", &active_path, 2_000_000_000_000_i64),
    ("expired-reader", &expired_path, 1),
  ] {
    source.push_str(&format!(
      r#"
[[issue_mcp_readers]]
id = "{id}"
project_id = "0199a6f4-d56c-7440-9aa2-6a320f862795"
credential_file = '{}'
expires_at_unix_ms = {expiry}
issue_ids = []
"#,
      path.display()
    ));
  }
  let config = ServerConfig::parse_toml(&source).unwrap();
  let runtime = ServerRuntime::start(config).await.unwrap();
  let client = reqwest::Client::new();
  let url = format!("http://{}/mcp/issues", runtime.management_addr());
  let initialize = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize",
    "params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"runtime-test","version":"1"}}});
  for token in [
    "fixture-source-token",
    "fixture-ai-token",
    expired.as_str(),
    rotated.as_str(),
  ] {
    let response = client
      .post(&url)
      .bearer_auth(token)
      .json(&initialize)
      .send()
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(response.bytes().await.unwrap().is_empty());
  }
  let response: serde_json::Value = client
    .post(&url)
    .bearer_auth(&active)
    .json(&initialize)
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(response["result"]["capabilities"], serde_json::json!({"tools":{}}));
  assert!(!response.to_string().contains(&active));
  // An explicitly empty resource scope denies a detail read before the unavailable DB is consulted.
  let detail: serde_json::Value = client
    .post(&url)
    .bearer_auth(&active)
    .json(&serde_json::json!({
    "jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"read_issue",
      "arguments":{"issue_id":"0199a6f4-d56c-7440-9aa2-6a320f862799"}}}))
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(detail["result"]["structuredContent"]["status"], "not_available");
  std::fs::write(&active_path, &rotated).unwrap();
  assert_eq!(
    client
      .post(&url)
      .bearer_auth(&active)
      .json(&initialize)
      .send()
      .await
      .unwrap()
      .status(),
    StatusCode::UNAUTHORIZED
  );
  assert_eq!(
    client
      .post(&url)
      .bearer_auth(&rotated)
      .json(&initialize)
      .send()
      .await
      .unwrap()
      .status(),
    StatusCode::OK
  );
  std::fs::remove_file(&active_path).unwrap();
  assert_eq!(
    client
      .post(&url)
      .bearer_auth(&rotated)
      .json(&initialize)
      .send()
      .await
      .unwrap()
      .status(),
    StatusCode::UNAUTHORIZED
  );
  runtime.shutdown().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn offline_issue_mcp_validation_reads_only_rebased_private_files() {
  use std::os::unix::fs::PermissionsExt as _;
  let directory = tempfile::tempdir().unwrap();
  let source = production_test_config_source(directory.path(), 1);
  let token_path = directory.path().join("issue-mcp-token");
  std::fs::write(&token_path, format!("ocmcp.offline-reader.{}", "d".repeat(64))).unwrap();
  std::fs::set_permissions(&token_path, std::fs::Permissions::from_mode(0o600)).unwrap();
  let mut config = ServerConfig::parse_toml(&format!(
    r#"{source}
[[issue_mcp_readers]]
id = "offline-reader"
project_id = "0199a6f4-d56c-7440-9aa2-6a320f862795"
credential_file = '{}'
expires_at_unix_ms = 2000000000000
"#,
    token_path.display()
  ))
  .unwrap();
  let root = directory.path().join("offline-root");
  for file in std::fs::read_dir(directory.path()).unwrap() {
    let file = file.unwrap().path();
    if file.is_file() {
      let rebased = root.join(file.strip_prefix("/").unwrap());
      std::fs::create_dir_all(rebased.parent().unwrap()).unwrap();
      std::fs::copy(&file, &rebased).unwrap();
      std::fs::set_permissions(&rebased, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
  }
  config.rebase_runtime_files(&root).unwrap();
  std::fs::remove_file(&token_path).unwrap();
  octacity_server::validate_runtime_files(&config)
    .await
    .expect("offline validation must not read the former live token path");
}
