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
"#;
  assert!(ServerConfig::parse_toml(unknown).is_err());

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
"#;
  assert!(ServerConfig::parse_toml(zero_grace).is_err());

  let insecure_remote_storage = valid_configuration().replace("http://127.0.0.1:9000", "http://objects.example");
  assert!(ServerConfig::parse_toml(&insecure_remote_storage).is_err());

  let unsafe_bucket = valid_configuration().replace("octacity-artifacts", "INVALID_BUCKET");
  assert!(ServerConfig::parse_toml(&unsafe_bucket).is_err());

  let invalid_signing_key_id = valid_configuration().replace("key_id = \"test-key\"", "key_id = \"\"");
  assert!(ServerConfig::parse_toml(&invalid_signing_key_id).is_err());

  let disabled_capability_recheck = valid_configuration().replace(
    "secret_key_file = \"object-secret-key\"",
    "secret_key_file = \"object-secret-key\"\ncapability_recheck_interval_milliseconds = 0",
  );
  assert!(ServerConfig::parse_toml(&disabled_capability_recheck).is_err());

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
fn management_agent_and_webhook_ingress_are_independently_configured() {
  let configured = valid_configuration().replace(
    "management_bind = \"127.0.0.1:0\"",
    "management_bind = \"127.0.0.1:0\"\nagent_bind = \"127.0.0.1:0\"\nwebhook_bind = \"[::1]:0\"",
  );
  let config = ServerConfig::parse_toml(&configured).unwrap();
  assert_eq!(config.agent_bind().unwrap().ip(), std::net::Ipv4Addr::LOCALHOST);
  assert_eq!(config.webhook_bind().unwrap().ip(), std::net::Ipv6Addr::LOCALHOST);

  let overlapping = valid_configuration().replace(
    "management_bind = \"127.0.0.1:0\"",
    "management_bind = \"127.0.0.1:8080\"\nagent_bind = \"0.0.0.0:8080\"\nacknowledge_unauthenticated_management = true",
  );
  let error = ServerConfig::parse_toml(&overlapping).unwrap_err();
  assert!(error.to_string().contains("independently bindable"));
}

fn valid_configuration() -> &'static str {
  r#"
management_bind = "127.0.0.1:0"
shutdown_grace_milliseconds = 1000

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

  runtime.shutdown().await.unwrap();
}

fn production_test_config(directory: &std::path::Path, unavailable_port: u16) -> ServerConfig {
  let postgres_url = directory.join("postgres-url");
  let access_key = directory.join("object-access-key");
  let secret_key = directory.join("object-secret-key");
  let signing_key = directory.join("signing-key");
  std::fs::write(
    &postgres_url,
    format!("postgres://octacity:secret@127.0.0.1:{unavailable_port}/octacity"),
  )
  .unwrap();
  std::fs::write(&access_key, "access").unwrap();
  std::fs::write(&secret_key, "secret").unwrap();
  std::fs::write(&signing_key, STANDARD.encode([7_u8; 32])).unwrap();
  #[cfg(unix)]
  for path in [&postgres_url, &access_key, &secret_key, &signing_key] {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
  }

  let path = |path: &std::path::Path| path.display().to_string().replace('\\', "\\\\");
  ServerConfig::parse_toml(&format!(
    r#"
management_bind = "127.0.0.1:0"
shutdown_grace_milliseconds = 1000
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
"#,
    path(&postgres_url),
    path(&access_key),
    path(&secret_key),
    path(&signing_key),
  ))
  .unwrap()
}
