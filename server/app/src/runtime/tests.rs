use std::{net::SocketAddr, sync::Arc};

use async_trait::async_trait;
use reqwest::StatusCode;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::{
  ServerConfig,
  readiness::{ReadinessCheck, ReadinessChecks, ReadinessState},
};

struct HealthyCheck;

#[async_trait]
impl ReadinessCheck for HealthyCheck {
  fn name(&self) -> &'static str {
    "test-dependency"
  }

  async fn check(&self) -> bool {
    true
  }
}

fn healthy_checks() -> ReadinessChecks {
  let check = || Arc::new(HealthyCheck) as Arc<dyn ReadinessCheck>;
  ReadinessChecks::new(check(), check(), check(), check(), std::iter::empty())
}

fn test_config() -> ServerConfig {
  ServerConfig::parse_toml(
    r#"
management_bind = "127.0.0.1:0"
agent_bind = "127.0.0.1:0"
shutdown_grace_milliseconds = 1000
supported_pipeline_capabilities = ["native"]
readiness_check_interval_milliseconds = 10
readiness_check_timeout_milliseconds = 100

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
"#,
  )
  .unwrap()
}

#[tokio::test]
async fn startup_separates_ingress_and_exposes_operational_metadata() {
  let runtime = ServerRuntime::start_with_readiness(test_config(), healthy_checks())
    .await
    .unwrap();
  let client = reqwest::Client::new();
  let origin = format!("http://{}", runtime.management_addr());

  for (path, expected_body) in [
    ("/health/live", r#"{"status":"live"}"#),
    ("/health/ready", r#"{"status":"ready"}"#),
  ] {
    let response = client.get(format!("{origin}{path}")).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().contains_key("x-request-id"));
    assert_eq!(response.text().await.unwrap(), expected_body);
  }

  let metadata: serde_json::Value = client
    .get(format!("{origin}/api/v1/operations/metadata"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(
    metadata["security"]["management"]["mode"],
    "trusted_network_unauthenticated"
  );
  assert_eq!(metadata["security"]["management"]["operator_authentication"], false);
  assert_eq!(metadata["security"]["agent"]["authentication_required"], true);
  assert_eq!(metadata["security"]["webhook"]["authentication_required"], true);
  assert_eq!(metadata["ingress"]["agent_enabled"], true);
  assert_eq!(metadata["ingress"]["webhook_enabled"], false);
  assert_eq!(metadata["ingress"]["listeners_separate"], true);

  let response = client
    .get(format!("http://{}/health/live", runtime.agent_addr().unwrap()))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::NOT_FOUND);

  runtime.shutdown().await.unwrap();
}

#[tokio::test]
async fn shutdown_cancels_the_listener_and_waits_for_its_task() {
  let runtime = ServerRuntime::start_with_readiness(test_config(), healthy_checks())
    .await
    .unwrap();
  let addresses = [runtime.management_addr(), runtime.agent_addr().unwrap()];
  for address in addresses {
    assert!(tokio::net::TcpListener::bind(address).await.is_err());
  }

  runtime.shutdown().await.unwrap();

  for address in addresses {
    let replacement = tokio::net::TcpListener::bind(address)
      .await
      .expect("shutdown must release every ingress listener");
    assert_eq!(replacement.local_addr().unwrap(), address);
  }
}

#[tokio::test]
async fn wait_reports_a_listener_that_stops_without_shutdown() {
  let cancellation = CancellationToken::new();
  let readiness_cancellation = cancellation.child_token();
  let mut listener_tasks = JoinSet::new();
  listener_tasks.spawn(async { Ok("management") });
  let mut runtime = ServerRuntime {
    management_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
    agent_addr: None,
    cache_addr: None,
    webhook_addr: None,
    shutdown_grace: std::time::Duration::from_secs(1),
    readiness: Arc::new(ReadinessState::default()),
    cancellation,
    listener_tasks,
    readiness_task: Some(tokio::spawn(async move {
      readiness_cancellation.cancelled().await;
    })),
    worker_task: None,
    notification_task: None,
  };

  assert!(matches!(
    runtime.wait().await,
    Err(ServerRuntimeError::ListenerUnexpectedExit { ingress: "management" })
  ));
}

#[tokio::test]
async fn wait_reports_a_worker_failure_without_waiting_for_a_listener() {
  let cancellation = CancellationToken::new();
  let readiness_cancellation = cancellation.child_token();
  let listener_cancellation = cancellation.child_token();
  let mut listener_tasks = JoinSet::new();
  listener_tasks.spawn(async move {
    listener_cancellation.cancelled().await;
    Ok("management")
  });
  let mut runtime = ServerRuntime {
    management_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
    agent_addr: None,
    cache_addr: None,
    webhook_addr: None,
    shutdown_grace: std::time::Duration::from_secs(1),
    readiness: Arc::new(ReadinessState::default()),
    cancellation,
    listener_tasks,
    readiness_task: Some(tokio::spawn(async move {
      readiness_cancellation.cancelled().await;
    })),
    worker_task: Some(tokio::spawn(async {
      panic!("worker failure");
    })),
    notification_task: None,
  };

  let result = tokio::time::timeout(std::time::Duration::from_secs(1), runtime.wait())
    .await
    .expect("worker failure must reach the runtime immediately");
  assert!(matches!(result, Err(ServerRuntimeError::WorkerTask(_))));
}

#[tokio::test]
async fn dropping_runtime_aborts_owned_listener_work() {
  struct DropSignal(Option<tokio::sync::oneshot::Sender<()>>);

  impl Drop for DropSignal {
    fn drop(&mut self) {
      if let Some(sender) = self.0.take() {
        let _ = sender.send(());
      }
    }
  }

  let (started_sender, started_receiver) = tokio::sync::oneshot::channel();
  let (dropped_sender, dropped_receiver) = tokio::sync::oneshot::channel();
  let mut listener_tasks = JoinSet::new();
  listener_tasks.spawn(async move {
    let _drop_signal = DropSignal(Some(dropped_sender));
    let _ = started_sender.send(());
    std::future::pending::<()>().await;
    Ok("management")
  });
  let runtime = ServerRuntime {
    management_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
    agent_addr: None,
    cache_addr: None,
    webhook_addr: None,
    shutdown_grace: std::time::Duration::from_secs(1),
    readiness: Arc::new(ReadinessState::default()),
    cancellation: CancellationToken::new(),
    listener_tasks,
    readiness_task: Some(tokio::spawn(std::future::pending())),
    worker_task: None,
    notification_task: None,
  };

  started_receiver.await.unwrap();
  drop(runtime);
  tokio::time::timeout(std::time::Duration::from_secs(1), dropped_receiver)
    .await
    .expect("aborted listener future must be dropped")
    .unwrap();
}
