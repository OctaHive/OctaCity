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

struct DropSignal(Option<tokio::sync::oneshot::Sender<()>>);

impl Drop for DropSignal {
  fn drop(&mut self) {
    if let Some(sender) = self.0.take() {
      let _ = sender.send(());
    }
  }
}

static LISTENER_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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

fn test_config_source() -> &'static str {
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
"#
}
fn test_config() -> ServerConfig {
  ServerConfig::parse_toml(test_config_source()).unwrap()
}

#[cfg(unix)]
#[tokio::test]
async fn local_integration_ingress_verifies_os_peer_without_upgrading_tcp_headers() {
  use std::os::unix::fs::MetadataExt;
  for authorized in [true, false] {
    let root = tempfile::tempdir().unwrap();
    let socket = root.path().join("management.sock");
    let uid = std::fs::metadata(root.path()).unwrap().uid();
    let allowed = if authorized { uid } else { uid.checked_add(1).unwrap() };
    let extra = format!(
      "[integration_management]\nsocket_path={:?}\npeer_uid={allowed}\nprincipal='local-stand-gateway'\n",
      socket.to_str().unwrap()
    );
    let config = ServerConfig::parse_toml(&super::test_runtime_config::runtime_config_source(
      root.path(),
      "postgres://test@127.0.0.1:1/unused",
      "http://127.0.0.1:9",
      &extra,
    ))
    .expect("explicit protected local management must be configurable");
    let runtime = ServerRuntime::start(config).await.unwrap();
    let tcp = reqwest::Client::new()
      .get(format!("http://{}/api/v1/integrations", runtime.management_addr()))
      .header("authorization", "Bearer forged")
      .header("x-forwarded-proto", "https")
      .header("x-octacity-actor", "local-stand-gateway")
      .send()
      .await
      .unwrap();
    assert_eq!(tcp.status(), StatusCode::FORBIDDEN);
    let local = reqwest::Client::builder()
      .unix_socket(socket.as_path())
      .build()
      .unwrap()
      .get("http://localhost/api/v1/integrations")
      .send()
      .await
      .unwrap();
    assert_eq!(
      local.status(),
      if authorized {
        StatusCode::SERVICE_UNAVAILABLE
      } else {
        StatusCode::FORBIDDEN
      },
      "only the verified OS peer may reach the separately installed service"
    );
    runtime.shutdown().await.unwrap();
  }
}

#[test]
fn signal_connector_file_requires_an_explicit_factory_profile_installation() {
  let profile = "factory_profiles_file='/run/secrets/factory-profiles.json'\n";
  let connector = "factory_signal_connectors_file='/run/secrets/signal-connectors.json'\n";
  let workers =
    "[factory]\npoll_interval_milliseconds=10\nclaim_lifetime_milliseconds=1000\nbatch_size=4\nconcurrency=2\n";
  let source = format!("{profile}{connector}{}{workers}", test_config_source());
  assert!(
    ServerConfig::parse_toml(&source).is_ok(),
    "explicit separate connectors must be accepted"
  );
  for invalid in [
    source.replace(profile, ""),
    source.replace(workers, ""),
    source.replace("/run/secrets/signal-connectors.json", ""),
  ] {
    assert!(ServerConfig::parse_toml(&invalid).is_err());
  }
}

#[test]
fn relative_offline_roots_do_not_rebase_installed_factory_files() {
  let source = format!(
    "factory_profiles_file='/run/secrets/factory-profiles.json'\nfactory_signal_connectors_file='/run/secrets/signal-connectors.json'\n{}\n[factory]\npoll_interval_milliseconds=10\nclaim_lifetime_milliseconds=1000\nbatch_size=4\nconcurrency=2\n",
    test_config_source()
  );
  let mut config = ServerConfig::parse_toml(&source).unwrap();
  assert_eq!(
    config.rebase_runtime_files(std::path::Path::new("offline-relative")),
    Err("runtime-file validation root must be absolute".to_owned())
  );
  assert_eq!(
    config.factory_profiles_file().unwrap(),
    std::path::Path::new("/run/secrets/factory-profiles.json")
  );
  assert_eq!(
    config.factory_signal_connectors_file().unwrap(),
    std::path::Path::new("/run/secrets/signal-connectors.json")
  );
}

#[test]
fn deployment_factory_profile_file_is_explicit_and_rebased_with_runtime_files() {
  let workers =
    "[factory]\npoll_interval_milliseconds=10\nclaim_lifetime_milliseconds=1000\nbatch_size=4\nconcurrency=2\n";
  let source = format!(
    "factory_profiles_file='/run/secrets/factory-profiles.json'\nfactory_signal_connectors_file='/run/secrets/signal-connectors.json'\n{}{workers}",
    test_config_source()
  );
  let absolute = source
    .replace(
      "url_file = \"postgres-url\"",
      "url_file = \"/run/secrets/postgres-url\"",
    )
    .replace("= \"object-access-key\"", "= \"/run/secrets/object-access-key\"")
    .replace("= \"object-secret-key\"", "= \"/run/secrets/object-secret-key\"")
    .replace("= \"signing-key\"", "= \"/run/secrets/signing-key\"")
    .replace("= \"agent-enrollment-key\"", "= \"/run/secrets/agent-enrollment-key\"")
    .replace("= \"cache-credential-key\"", "= \"/run/secrets/cache-credential-key\"")
    .replace("= \"job-spec-policy.json\"", "= \"/run/secrets/job-spec-policy.json\"");
  let mut config = ServerConfig::parse_toml(&absolute).unwrap();
  assert_eq!(
    config.factory_profiles_file().unwrap(),
    std::path::Path::new("/run/secrets/factory-profiles.json")
  );
  if cfg!(windows) {
    // A Linux image root is not an absolute Windows validation root. Keep the
    // configured files unchanged instead of admitting a drive-relative path.
    assert_eq!(
      config.rebase_runtime_files(std::path::Path::new("/offline")),
      Err("runtime-file validation root must be absolute".to_owned())
    );
    assert_eq!(
      config.factory_profiles_file().unwrap(),
      std::path::Path::new("/run/secrets/factory-profiles.json")
    );
    assert_eq!(
      config.factory_signal_connectors_file().unwrap(),
      std::path::Path::new("/run/secrets/signal-connectors.json")
    );
  } else {
    // Offline image validation relocates every absolute runtime file together.
    config.rebase_runtime_files(std::path::Path::new("/offline")).unwrap();
    assert_eq!(
      config.factory_profiles_file().unwrap(),
      std::path::Path::new("/offline/run/secrets/factory-profiles.json")
    );
    assert_eq!(
      config.factory_signal_connectors_file().unwrap(),
      std::path::Path::new("/offline/run/secrets/signal-connectors.json")
    );
  }
  assert!(
    ServerConfig::parse_toml(&format!(
      "factory_profiles_file='/private/config'\n{}",
      test_config_source()
    ))
    .is_err()
  );
  assert!(ServerConfig::parse_toml(&source.replace("/run/secrets/factory-profiles.json", "")).is_err());
  assert!(test_config().factory_profiles_file().is_none());
  assert!(test_config().factory_signal_connectors_file().is_none());
}

#[tokio::test]
async fn factory_requires_explicit_installation_before_loading_resources_or_binding() {
  let source = format!(
    "{}\n[factory]\npoll_interval_milliseconds = 10\nclaim_lifetime_milliseconds = 1000\nbatch_size = 4\nconcurrency = 2\n",
    test_config_source()
  );
  let config = ServerConfig::parse_toml(&source).unwrap();
  let error = match ServerRuntime::start(config).await {
    Ok(runtime) => {
      runtime.shutdown().await.unwrap();
      panic!("enabled Factory must require exact installed capabilities");
    }
    Err(error) => error,
  };
  assert_eq!(
    error.to_string(),
    "Factory runtime requires explicit installed capabilities"
  );
}

#[tokio::test]
async fn startup_rejects_invalid_connector_catalog_before_other_resources() {
  let directory = tempfile::tempdir().unwrap();
  let profiles = directory.path().join("profiles.json");
  let connectors = directory.path().join("private-connector-canary.json");
  std::fs::write(
    &profiles,
    serde_json::to_vec(&serde_json::json!({
      "version":1,"projects":[{"project_id":octacity_server_domain::ProjectId::generate(),
        "references":[],"build_configurations":[]}],"codex":[],
    }))
    .unwrap(),
  )
  .unwrap();
  std::fs::write(&connectors, "{\"private-endpoint\":\"https://private-canary.invalid\"}").unwrap();
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    for path in [&profiles, &connectors] {
      std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
  }
  let source = format!(
    "factory_profiles_file={}\nfactory_signal_connectors_file={}\n{}\n[factory]\npoll_interval_milliseconds=10\nclaim_lifetime_milliseconds=1000\nbatch_size=4\nconcurrency=2\n",
    serde_json::to_string(&profiles).unwrap(),
    serde_json::to_string(&connectors).unwrap(),
    test_config_source(),
  );
  let error = ServerRuntime::start(ServerConfig::parse_toml(&source).unwrap())
    .await
    .err()
    .unwrap();
  assert!(
    matches!(error, ServerRuntimeError::InvalidFactoryInstallation),
    "connector installation must precede shared-resource I/O"
  );
  assert!(!format!("{error:?}").contains("canary"));
}

#[tokio::test]
async fn startup_reads_explicit_factory_catalog_before_resources_with_safe_diagnostics() {
  let directory = tempfile::tempdir().unwrap();
  let path = directory.path().join("private-instructions-canary.json");
  std::fs::write(&path, "{\"instructions\":\"provider-secret-canary\"}").unwrap();
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
  }
  let source = format!(
    "factory_profiles_file={}\n{}\n[factory]\npoll_interval_milliseconds=10\nclaim_lifetime_milliseconds=1000\nbatch_size=4\nconcurrency=2\n",
    serde_json::to_string(&path).unwrap(),
    test_config_source()
  );
  let config = ServerConfig::parse_toml(&source).unwrap();
  let error = ServerRuntime::start(config).await.err().unwrap();
  assert_eq!(error.to_string(), "Factory runtime installation failed");
  assert!(!format!("{error:?}").contains("canary"));
}

#[tokio::test]
async fn startup_separates_ingress_and_exposes_health_checks() {
  let _listener_guard = LISTENER_TEST_LOCK.lock().await;
  let runtime = ServerRuntime::start_with_readiness(test_config(), healthy_checks())
    .await
    .unwrap();
  let client = reqwest::Client::new();
  let origin = format!("http://{}", runtime.management_addr());

  for (path, expected_body) in [
    ("/health/live", r#"{"status":"live"}"#),
    ("/health/ready", r#"{"status":"ready"}"#),
    ("/health/factory", r#"{"status":"disabled"}"#),
  ] {
    let response = client.get(format!("{origin}{path}")).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().contains_key("x-request-id"));
    assert_eq!(response.text().await.unwrap(), expected_body);
  }

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
  let cancellation = CancellationToken::new();
  let readiness_cancellation = cancellation.child_token();
  let listener_cancellation = cancellation.child_token();
  let (started_sender, started_receiver) = tokio::sync::oneshot::channel();
  let (cancelled_sender, cancelled_receiver) = tokio::sync::oneshot::channel();
  let (completion_sender, completion_receiver) = tokio::sync::oneshot::channel();
  let (dropped_sender, dropped_receiver) = tokio::sync::oneshot::channel();
  let mut listener_tasks = JoinSet::new();
  listener_tasks.spawn(async move {
    let _drop_signal = DropSignal(Some(dropped_sender));
    let _ = started_sender.send(());
    listener_cancellation.cancelled().await;
    let _ = cancelled_sender.send(());
    completion_receiver
      .await
      .expect("test must release listener task completion");
    Ok("management")
  });
  let runtime = ServerRuntime {
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
    metrics_task: None,
  };

  started_receiver.await.unwrap();
  let shutdown = tokio::spawn(runtime.shutdown());
  tokio::time::timeout(std::time::Duration::from_secs(1), cancelled_receiver)
    .await
    .expect("shutdown must cancel the listener task")
    .unwrap();
  assert!(
    !shutdown.is_finished(),
    "shutdown must wait for listener task completion"
  );

  completion_sender.send(()).unwrap();
  shutdown.await.unwrap().unwrap();
  dropped_receiver.await.unwrap();
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
    metrics_task: None,
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
    metrics_task: None,
  };

  let result = tokio::time::timeout(std::time::Duration::from_secs(1), runtime.wait())
    .await
    .expect("worker failure must reach the runtime immediately");
  assert!(matches!(result, Err(ServerRuntimeError::WorkerTask(_))));
}

#[tokio::test]
async fn dropping_runtime_aborts_owned_listener_work() {
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
    metrics_task: None,
  };

  started_receiver.await.unwrap();
  drop(runtime);
  tokio::time::timeout(std::time::Duration::from_secs(1), dropped_receiver)
    .await
    .expect("aborted listener future must be dropped")
    .unwrap();
}

#[tokio::test]
async fn unavailable_factory_dependency_does_not_block_ordinary_health_and_recovers() {
  struct FactoryCheck(std::sync::atomic::AtomicBool);
  #[async_trait]
  impl ReadinessCheck for FactoryCheck {
    fn name(&self) -> &'static str {
      "factory-test-provider"
    }
    async fn check(&self) -> bool {
      self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
  }
  let _listener_guard = LISTENER_TEST_LOCK.lock().await;
  let provider = Arc::new(FactoryCheck(std::sync::atomic::AtomicBool::new(false)));
  let mut checks = healthy_checks();
  checks.push_factory(provider.clone());
  let runtime = ServerRuntime::start_with_readiness(test_config(), checks)
    .await
    .unwrap();
  let client = reqwest::Client::new();
  let origin = format!("http://{}", runtime.management_addr());
  for path in ["/health/live", "/health/ready"] {
    assert_eq!(
      client.get(format!("{origin}{path}")).send().await.unwrap().status(),
      StatusCode::OK
    );
  }
  let factory = client.get(format!("{origin}/health/factory")).send().await.unwrap();
  assert_eq!(factory.status(), StatusCode::SERVICE_UNAVAILABLE);
  assert_eq!(factory.text().await.unwrap(), r#"{"status":"unavailable"}"#);
  provider.0.store(true, std::sync::atomic::Ordering::SeqCst);
  tokio::time::timeout(std::time::Duration::from_secs(1), async {
    loop {
      if client
        .get(format!("{origin}/health/factory"))
        .send()
        .await
        .unwrap()
        .status()
        == StatusCode::OK
      {
        break;
      }
      tokio::task::yield_now().await;
    }
  })
  .await
  .unwrap();
  assert_eq!(
    client
      .get(format!("{origin}/health/ready"))
      .send()
      .await
      .unwrap()
      .status(),
    StatusCode::OK
  );
  runtime.shutdown().await.unwrap();
}

#[tokio::test]
async fn installed_factory_outbox_health_tracks_failure_recovery_and_shutdown_over_http() {
  struct Dispatcher(std::sync::atomic::AtomicBool);
  #[async_trait]
  impl octacity_server_application::FactoryOutboxDispatcher for Dispatcher {
    async fn dispatch_once(
      &self,
      _: octacity_server_domain::Timestamp,
    ) -> Result<octacity_server_application::FactoryOutboxDispatchSummary, octacity_server_application::ApplicationError>
    {
      if self.0.load(std::sync::atomic::Ordering::SeqCst) {
        Err(octacity_server_application::ApplicationError::Store(
          octacity_server_store::StoreError::Unavailable,
        ))
      } else {
        Ok(Default::default())
      }
    }
  }
  let _listener_guard = LISTENER_TEST_LOCK.lock().await;
  let dispatcher = Arc::new(Dispatcher(std::sync::atomic::AtomicBool::new(true)));
  let policy: crate::FactoryRuntimeConfig = toml::from_str("poll_interval_milliseconds=10\nclaim_lifetime_milliseconds=1000\nbatch_size=4\nconcurrency=2\n[retry]\nmax_backoff_milliseconds=40\nconsecutive_failure_limit=3\ncircuit_open_milliseconds=200").unwrap();
  let mut checks = healthy_checks();
  let installed = factory::install_outbox(vec![dispatcher.clone()], policy, &mut checks)
    .pop()
    .unwrap();
  let cancel = CancellationToken::new();
  let task = workers::spawn_factory_outbox_worker(installed.dispatcher, installed.health, policy, cancel.clone());
  let runtime = ServerRuntime::start_with_readiness(test_config(), checks)
    .await
    .unwrap();
  let client = reqwest::Client::new();
  let origin = format!("http://{}", runtime.management_addr());
  assert_eq!(
    client
      .get(format!("{origin}/health/factory"))
      .send()
      .await
      .unwrap()
      .status(),
    StatusCode::SERVICE_UNAVAILABLE
  );
  assert_eq!(
    client
      .get(format!("{origin}/health/ready"))
      .send()
      .await
      .unwrap()
      .status(),
    StatusCode::OK
  );
  dispatcher.0.store(false, std::sync::atomic::Ordering::SeqCst);
  tokio::time::timeout(std::time::Duration::from_secs(3), async {
    loop {
      let response = client.get(format!("{origin}/health/factory")).send().await.unwrap();
      if response.status() == StatusCode::OK {
        assert_eq!(response.text().await.unwrap(), r#"{"status":"ready"}"#);
        break;
      }
      tokio::task::yield_now().await;
    }
  })
  .await
  .unwrap();
  cancel.cancel();
  task.await.unwrap();
  tokio::time::timeout(std::time::Duration::from_secs(3), async {
    while client
      .get(format!("{origin}/health/factory"))
      .send()
      .await
      .unwrap()
      .status()
      != StatusCode::SERVICE_UNAVAILABLE
    {
      tokio::task::yield_now().await;
    }
  })
  .await
  .unwrap();
  assert_eq!(
    client
      .get(format!("{origin}/health/ready"))
      .send()
      .await
      .unwrap()
      .status(),
    StatusCode::OK
  );
  runtime.shutdown().await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn shutdown_timeout_drains_only_tasks_that_have_not_already_completed() {
  for blocked in ["workers", "notifications", "metrics"] {
    let cancellation = CancellationToken::new();
    let listener_cancellation = cancellation.child_token();
    let readiness_cancellation = cancellation.child_token();
    let worker_cancellation = cancellation.child_token();
    let notification_cancellation = cancellation.child_token();
    let metrics_cancellation = cancellation.child_token();
    let (dropped_sender, dropped_receiver) = tokio::sync::oneshot::channel();
    let dropped_sender = Arc::new(std::sync::Mutex::new(Some(dropped_sender)));
    let worker_drop = dropped_sender.clone();
    let notification_drop = dropped_sender.clone();
    let metrics_drop = dropped_sender;
    let mut listener_tasks = JoinSet::new();
    listener_tasks.spawn(async move {
      listener_cancellation.cancelled().await;
      Ok("management")
    });
    let grace = std::time::Duration::from_millis(10);
    let runtime = ServerRuntime {
      management_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
      agent_addr: None,
      cache_addr: None,
      webhook_addr: None,
      shutdown_grace: grace,
      readiness: Arc::new(ReadinessState::default()),
      cancellation,
      listener_tasks,
      readiness_task: Some(tokio::spawn(async move {
        readiness_cancellation.cancelled().await;
      })),
      worker_task: Some(tokio::spawn(async move {
        worker_cancellation.cancelled().await;
        if blocked == "workers" {
          let _drop = DropSignal(worker_drop.lock().unwrap().take());
          std::future::pending::<()>().await;
        }
        Ok(())
      })),
      notification_task: Some(tokio::spawn(async move {
        notification_cancellation.cancelled().await;
        if blocked == "notifications" {
          let _drop = DropSignal(notification_drop.lock().unwrap().take());
          std::future::pending::<()>().await;
        }
      })),
      metrics_task: Some(tokio::spawn(async move {
        metrics_cancellation.cancelled().await;
        if blocked == "metrics" {
          let _drop = DropSignal(metrics_drop.lock().unwrap().take());
          std::future::pending::<()>().await;
        }
      })),
    };
    let result = tokio::spawn(runtime.shutdown())
      .await
      .expect("a completed earlier task must never be polled twice during bounded cleanup");
    assert!(matches!(result, Err(ServerRuntimeError::ShutdownTimeout(value)) if value == grace));
    dropped_receiver
      .await
      .expect("the still-blocked owned task must be aborted and drained before returning");
  }
}
