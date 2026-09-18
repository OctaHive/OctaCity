use std::{net::SocketAddr, sync::Arc};

use thiserror::Error;
use tokio::{
  net::TcpListener,
  task::{JoinHandle, JoinSet},
};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::{
  ServerConfig,
  readiness::{ReadinessChecks, ReadinessMonitor, ReadinessState},
};

/// Running server process and ownership handle for its cancellation tree.
#[must_use = "dropping the runtime aborts its listeners; call shutdown for graceful drain"]
pub struct ServerRuntime {
  management_addr: SocketAddr,
  agent_addr: Option<SocketAddr>,
  webhook_addr: Option<SocketAddr>,
  shutdown_grace: std::time::Duration,
  readiness: Arc<ReadinessState>,
  cancellation: CancellationToken,
  listener_tasks: JoinSet<ListenerTaskResult>,
  readiness_task: Option<JoinHandle<()>>,
}

type ListenerTaskResult = Result<&'static str, (&'static str, std::io::Error)>;

impl ServerRuntime {
  /// Binds the listener and starts supervised work from validated configuration.
  pub async fn start(config: ServerConfig) -> Result<Self, ServerRuntimeError> {
    let checks = ReadinessChecks::from_config(&config)
      .await
      .map_err(ServerRuntimeError::ReadinessSetup)?;
    Self::start_with_readiness(config, checks).await
  }

  /// Starts the process with concrete dependency and worker health checks.
  pub(crate) async fn start_with_readiness(
    config: ServerConfig,
    checks: ReadinessChecks,
  ) -> Result<Self, ServerRuntimeError> {
    let management_listener = bind_listener("management", config.management_bind()).await?;
    let agent_listener = bind_optional_listener("agent", config.agent_bind()).await?;
    let webhook_listener = bind_optional_listener("webhook", config.webhook_bind()).await?;
    let management_addr = inspect_listener("management", &management_listener)?;
    let agent_addr = agent_listener
      .as_ref()
      .map(|listener| inspect_listener("agent", listener))
      .transpose()?;
    let webhook_addr = webhook_listener
      .as_ref()
      .map(|listener| inspect_listener("webhook", listener))
      .transpose()?;
    let cancellation = CancellationToken::new();
    let monitor = ReadinessMonitor::start(
      checks,
      config.readiness_check_interval(),
      config.readiness_check_timeout(),
      cancellation.child_token(),
    )
    .await;
    let readiness = monitor.state();
    let readiness_task = monitor.into_task();
    let router_readiness = readiness.clone();
    let metadata = octacity_server_api_rest::v1::OperationalMetadata::trusted_network(
      config.management_externally_reachable(),
      config.unauthenticated_management_acknowledged(),
      agent_addr.is_some(),
      webhook_addr.is_some(),
    );
    let management_router =
      octacity_server_api_rest::management_router_with_metadata(move || router_readiness.is_ready(), metadata);
    let mut listener_tasks = JoinSet::new();
    spawn_listener(
      &mut listener_tasks,
      "management",
      management_listener,
      management_router,
      cancellation.child_token(),
    );
    if let Some(listener) = agent_listener {
      spawn_listener(
        &mut listener_tasks,
        "agent",
        listener,
        axum::Router::new(),
        cancellation.child_token(),
      );
    }
    if let Some(listener) = webhook_listener {
      spawn_listener(
        &mut listener_tasks,
        "webhook",
        listener,
        axum::Router::new(),
        cancellation.child_token(),
      );
    }
    info!(%management_addr, ?agent_addr, ?webhook_addr, "server listeners ready");
    Ok(Self {
      management_addr,
      agent_addr,
      webhook_addr,
      shutdown_grace: config.shutdown_grace(),
      readiness,
      cancellation,
      listener_tasks,
      readiness_task: Some(readiness_task),
    })
  }

  /// Actual bound management address, including an OS-assigned port.
  pub const fn management_addr(&self) -> SocketAddr {
    self.management_addr
  }

  /// Actual bound Agent address when authenticated Agent ingress is configured.
  pub const fn agent_addr(&self) -> Option<SocketAddr> {
    self.agent_addr
  }

  /// Actual bound webhook address when authenticated webhook ingress is configured.
  pub const fn webhook_addr(&self) -> Option<SocketAddr> {
    self.webhook_addr
  }

  /// Waits until any configured ingress listener exits unexpectedly.
  ///
  /// Process entry points should select this future against their shutdown
  /// signal so a failed listener cannot leave an apparently healthy process.
  pub async fn wait(&mut self) -> Result<(), ServerRuntimeError> {
    let Some(result) = self.listener_tasks.join_next().await else {
      return Err(ServerRuntimeError::UnexpectedExit);
    };
    self.readiness.set(false);
    self.cancellation.cancel();
    while self.listener_tasks.join_next().await.is_some() {}
    if let Some(readiness_task) = self.readiness_task.take()
      && let Err(source) = readiness_task.await
    {
      return Err(ServerRuntimeError::ReadinessTask(source));
    }
    match result {
      Ok(Ok(ingress)) => Err(ServerRuntimeError::ListenerUnexpectedExit { ingress }),
      Ok(Err((ingress, source))) => Err(ServerRuntimeError::Serve { ingress, source }),
      Err(source) => Err(ServerRuntimeError::ListenerTask(source)),
    }
  }

  /// Stops admission, cancels the process tree, and waits for bounded drain.
  pub async fn shutdown(mut self) -> Result<(), ServerRuntimeError> {
    self.readiness.set(false);
    self.cancellation.cancel();
    if self.listener_tasks.is_empty() {
      return Err(ServerRuntimeError::UnexpectedExit);
    }
    let Some(mut readiness_task) = self.readiness_task.take() else {
      return Err(ServerRuntimeError::UnexpectedExit);
    };
    let result = tokio::time::timeout(self.shutdown_grace, async {
      let mut listener_error = None;
      while let Some(result) = self.listener_tasks.join_next().await {
        match result {
          Ok(Ok(_)) => {}
          Ok(Err((ingress, source))) if listener_error.is_none() => {
            listener_error = Some(ServerRuntimeError::Serve { ingress, source });
          }
          Err(source) if listener_error.is_none() => {
            listener_error = Some(ServerRuntimeError::ListenerTask(source));
          }
          _ => {}
        }
      }
      if let Err(source) = (&mut readiness_task).await {
        return Err(ServerRuntimeError::ReadinessTask(source));
      }
      listener_error.map_or(Ok(()), Err)
    })
    .await;
    match result {
      Ok(Ok(())) => {
        info!(%self.management_addr, "server shutdown complete");
        Ok(())
      }
      Ok(Err(error)) => Err(error),
      Err(_) => {
        self.listener_tasks.abort_all();
        readiness_task.abort();
        while self.listener_tasks.join_next().await.is_some() {}
        let _ = readiness_task.await;
        Err(ServerRuntimeError::ShutdownTimeout(self.shutdown_grace))
      }
    }
  }
}

impl Drop for ServerRuntime {
  fn drop(&mut self) {
    self.readiness.set(false);
    self.cancellation.cancel();
    self.listener_tasks.abort_all();
    if let Some(readiness_task) = self.readiness_task.take() {
      readiness_task.abort();
    }
  }
}

async fn bind_listener(ingress: &'static str, address: SocketAddr) -> Result<TcpListener, ServerRuntimeError> {
  TcpListener::bind(address)
    .await
    .map_err(|source| ServerRuntimeError::Bind {
      ingress,
      address,
      source,
    })
}

async fn bind_optional_listener(
  ingress: &'static str,
  address: Option<SocketAddr>,
) -> Result<Option<TcpListener>, ServerRuntimeError> {
  match address {
    Some(address) => bind_listener(ingress, address).await.map(Some),
    None => Ok(None),
  }
}

fn inspect_listener(ingress: &'static str, listener: &TcpListener) -> Result<SocketAddr, ServerRuntimeError> {
  listener
    .local_addr()
    .map_err(|source| ServerRuntimeError::InspectListener { ingress, source })
}

fn spawn_listener(
  tasks: &mut JoinSet<ListenerTaskResult>,
  ingress: &'static str,
  listener: TcpListener,
  router: axum::Router,
  cancellation: CancellationToken,
) {
  tasks.spawn(async move {
    axum::serve(listener, router)
      .with_graceful_shutdown(cancellation.cancelled_owned())
      .await
      .map_err(|source| (ingress, source))?;
    Ok(ingress)
  });
}

/// Failure to start, serve, or gracefully stop the server process.
#[derive(Debug, Error)]
pub enum ServerRuntimeError {
  /// Static dependency material could not be loaded safely.
  #[error("failed to configure readiness dependencies: {0}")]
  ReadinessSetup(crate::ReadinessSetupError),
  /// One independently configured listener could not bind its address.
  #[error("failed to bind {ingress} listener at {address}: {source}")]
  Bind {
    /// Independently configured ingress whose listener could not bind.
    ingress: &'static str,
    /// Requested listener address.
    address: SocketAddr,
    /// Underlying socket error.
    source: std::io::Error,
  },
  /// The bound listener address could not be inspected.
  #[error("failed to inspect bound {ingress} listener: {source}")]
  InspectListener {
    /// Independently configured ingress whose listener could not be inspected.
    ingress: &'static str,
    /// Underlying socket error.
    source: std::io::Error,
  },
  /// The HTTP server returned an I/O failure.
  #[error("{ingress} listener failed: {source}")]
  Serve {
    /// Independently configured ingress whose server failed.
    ingress: &'static str,
    /// Underlying server error.
    source: std::io::Error,
  },
  /// A supervised listener task panicked or was cancelled unexpectedly.
  #[error("listener task failed: {0}")]
  ListenerTask(tokio::task::JoinError),
  /// The supervised readiness monitor panicked or was cancelled unexpectedly.
  #[error("readiness monitor task failed: {0}")]
  ReadinessTask(tokio::task::JoinError),
  /// The listener stopped without a requested shutdown.
  #[error("management listener exited unexpectedly")]
  UnexpectedExit,
  /// One specific independently configured listener stopped without shutdown.
  #[error("{ingress} listener exited unexpectedly")]
  ListenerUnexpectedExit {
    /// Ingress whose listener stopped.
    ingress: &'static str,
  },
  /// In-flight work did not drain within the configured grace period.
  #[error("server did not shut down within {0:?}")]
  ShutdownTimeout(std::time::Duration),
}

#[cfg(test)]
mod tests {
  use async_trait::async_trait;
  use reqwest::StatusCode;

  use super::*;
  use crate::readiness::ReadinessCheck;

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
webhook_bind = "127.0.0.1:0"
shutdown_grace_milliseconds = 1000
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
    assert_eq!(metadata["ingress"]["webhook_enabled"], true);
    assert_eq!(metadata["ingress"]["listeners_separate"], true);

    for address in [runtime.agent_addr().unwrap(), runtime.webhook_addr().unwrap()] {
      let response = client
        .get(format!("http://{address}/health/live"))
        .send()
        .await
        .unwrap();
      assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    runtime.shutdown().await.unwrap();
  }

  #[tokio::test]
  async fn shutdown_cancels_the_listener_and_waits_for_its_task() {
    let runtime = ServerRuntime::start_with_readiness(test_config(), healthy_checks())
      .await
      .unwrap();
    let addresses = [
      runtime.management_addr(),
      runtime.agent_addr().unwrap(),
      runtime.webhook_addr().unwrap(),
    ];
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
      webhook_addr: None,
      shutdown_grace: std::time::Duration::from_secs(1),
      readiness: Arc::new(ReadinessState::default()),
      cancellation,
      listener_tasks,
      readiness_task: Some(tokio::spawn(async move {
        readiness_cancellation.cancelled().await;
      })),
    };

    assert!(matches!(
      runtime.wait().await,
      Err(ServerRuntimeError::ListenerUnexpectedExit { ingress: "management" })
    ));
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
      webhook_addr: None,
      shutdown_grace: std::time::Duration::from_secs(1),
      readiness: Arc::new(ReadinessState::default()),
      cancellation: CancellationToken::new(),
      listener_tasks,
      readiness_task: Some(tokio::spawn(std::future::pending())),
    };

    started_receiver.await.unwrap();
    drop(runtime);
    tokio::time::timeout(std::time::Duration::from_secs(1), dropped_receiver)
      .await
      .expect("aborted listener future must be dropped")
      .unwrap();
  }
}
