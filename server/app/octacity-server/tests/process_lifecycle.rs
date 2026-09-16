use octacity_server::{ServerConfig, ServerRuntime};
use reqwest::StatusCode;

#[test]
fn configuration_is_strict_and_validated_before_startup() {
  let valid = ServerConfig::parse_toml(
    r#"
management_bind = "127.0.0.1:0"
shutdown_grace_milliseconds = 1000
"#,
  )
  .unwrap();
  assert_eq!(valid.management_bind().port(), 0);

  let unknown = r#"
management_bind = "127.0.0.1:0"
shutdown_grace_milliseconds = 1000
surprise = true
"#;
  assert!(ServerConfig::parse_toml(unknown).is_err());

  let zero_grace = r#"
management_bind = "127.0.0.1:0"
shutdown_grace_milliseconds = 0
"#;
  assert!(ServerConfig::parse_toml(zero_grace).is_err());

  let oversized = "x".repeat(1024 * 1024 + 1);
  assert!(matches!(
    ServerConfig::parse_toml(&oversized),
    Err(octacity_server::ServerConfigError::TooLarge { .. })
  ));
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
async fn startup_exposes_only_bounded_health_routes_with_request_ids() {
  let runtime = ServerRuntime::start(test_config()).await.unwrap();
  let client = reqwest::Client::builder().build().unwrap();
  let origin = format!("http://{}", runtime.management_addr());

  for (path, expected_body) in [
    ("/health/live", r#"{"status":"live"}"#),
    ("/health/ready", r#"{"status":"ready"}"#),
  ] {
    let response = client.get(format!("{origin}{path}")).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let request_id = response.headers().get("x-request-id").unwrap().to_str().unwrap();
    assert!(uuid::Uuid::parse_str(request_id).is_ok());
    assert_eq!(response.text().await.unwrap(), expected_body);
  }

  let mutation = client
    .post(format!("{origin}/api/v1/builds"))
    .body("{}")
    .send()
    .await
    .unwrap();
  assert_eq!(mutation.status(), StatusCode::NOT_FOUND);
  assert!(mutation.headers().contains_key("x-request-id"));

  runtime.shutdown().await.unwrap();
}

#[tokio::test]
async fn shutdown_cancels_the_listener_and_waits_for_its_task() {
  let runtime = ServerRuntime::start(test_config()).await.unwrap();
  let addr = runtime.management_addr();

  assert!(
    tokio::net::TcpListener::bind(addr).await.is_err(),
    "the running management listener must own its address exclusively"
  );

  runtime.shutdown().await.unwrap();

  let replacement = tokio::net::TcpListener::bind(addr)
    .await
    .expect("shutdown must release the management listener");
  assert_eq!(replacement.local_addr().unwrap(), addr);
}

fn test_config() -> ServerConfig {
  ServerConfig::parse_toml(
    r#"
management_bind = "127.0.0.1:0"
shutdown_grace_milliseconds = 1000
"#,
  )
  .unwrap()
}
