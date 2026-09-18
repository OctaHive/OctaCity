use std::{net::SocketAddr, sync::Arc};

use thiserror::Error;
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::{
  ServerConfig,
  readiness::{ReadinessChecks, ReadinessMonitor, ReadinessState},
};

/// Running server process and ownership handle for its cancellation tree.
#[must_use = "dropping the runtime aborts the listener; call shutdown for graceful drain"]
pub struct ServerRuntime {
  management_addr: SocketAddr,
  shutdown_grace: std::time::Duration,
  readiness: Arc<ReadinessState>,
  cancellation: CancellationToken,
  listener_task: Option<JoinHandle<Result<(), std::io::Error>>>,
  readiness_task: Option<JoinHandle<()>>,
}

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
    let listener = TcpListener::bind(config.management_bind())
      .await
      .map_err(|source| ServerRuntimeError::Bind {
        address: config.management_bind(),
        source,
      })?;
    let management_addr = listener.local_addr().map_err(ServerRuntimeError::InspectListener)?;
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
    let listener_cancellation = cancellation.child_token();
    let router_readiness = readiness.clone();
    let router = octacity_server_api_rest::management_router(move || router_readiness.is_ready());
    let listener_readiness = readiness.clone();
    let listener_task = tokio::spawn(async move {
      let result = axum::serve(listener, router)
        .with_graceful_shutdown(listener_cancellation.cancelled_owned())
        .await;
      listener_readiness.set(false);
      result
    });
    info!(%management_addr, "server management listener ready");
    Ok(Self {
      management_addr,
      shutdown_grace: config.shutdown_grace(),
      readiness,
      cancellation,
      listener_task: Some(listener_task),
      readiness_task: Some(readiness_task),
    })
  }

  /// Actual bound management address, including an OS-assigned port.
  pub const fn management_addr(&self) -> SocketAddr {
    self.management_addr
  }

  /// Waits until the management listener exits unexpectedly.
  ///
  /// Process entry points should select this future against their shutdown
  /// signal so a failed listener cannot leave an apparently healthy process.
  pub async fn wait(&mut self) -> Result<(), ServerRuntimeError> {
    let Some(listener_task) = self.listener_task.as_mut() else {
      return Err(ServerRuntimeError::UnexpectedExit);
    };
    let result = listener_task.await;
    self.listener_task = None;
    self.readiness.set(false);
    self.cancellation.cancel();
    if let Some(readiness_task) = self.readiness_task.take()
      && let Err(source) = readiness_task.await
    {
      return Err(ServerRuntimeError::ReadinessTask(source));
    }
    match result {
      Ok(Ok(())) => Err(ServerRuntimeError::UnexpectedExit),
      Ok(Err(source)) => Err(ServerRuntimeError::Serve(source)),
      Err(source) => Err(ServerRuntimeError::ListenerTask(source)),
    }
  }

  /// Stops admission, cancels the process tree, and waits for bounded drain.
  pub async fn shutdown(mut self) -> Result<(), ServerRuntimeError> {
    self.readiness.set(false);
    self.cancellation.cancel();
    let Some(mut listener_task) = self.listener_task.take() else {
      return Err(ServerRuntimeError::UnexpectedExit);
    };
    let Some(mut readiness_task) = self.readiness_task.take() else {
      return Err(ServerRuntimeError::UnexpectedExit);
    };
    match tokio::time::timeout(self.shutdown_grace, async {
      tokio::join!(&mut listener_task, &mut readiness_task)
    })
    .await
    {
      Ok((Ok(Ok(())), Ok(()))) => {
        info!(%self.management_addr, "server shutdown complete");
        Ok(())
      }
      Ok((Ok(Err(source)), _)) => Err(ServerRuntimeError::Serve(source)),
      Ok((Err(source), _)) => Err(ServerRuntimeError::ListenerTask(source)),
      Ok((_, Err(source))) => Err(ServerRuntimeError::ReadinessTask(source)),
      Err(_) => {
        listener_task.abort();
        readiness_task.abort();
        let _ = listener_task.await;
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
    if let Some(listener_task) = self.listener_task.take() {
      listener_task.abort();
    }
    if let Some(readiness_task) = self.readiness_task.take() {
      readiness_task.abort();
    }
  }
}

/// Failure to start, serve, or gracefully stop the server process.
#[derive(Debug, Error)]
pub enum ServerRuntimeError {
  /// Static dependency material could not be loaded safely.
  #[error("failed to configure readiness dependencies: {0}")]
  ReadinessSetup(crate::ReadinessSetupError),
  /// The management listener could not bind its configured address.
  #[error("failed to bind management listener at {address}: {source}")]
  Bind {
    /// Requested listener address.
    address: SocketAddr,
    /// Underlying socket error.
    source: std::io::Error,
  },
  /// The bound listener address could not be inspected.
  #[error("failed to inspect bound management listener: {0}")]
  InspectListener(std::io::Error),
  /// The HTTP server returned an I/O failure.
  #[error("management listener failed: {0}")]
  Serve(std::io::Error),
  /// The supervised listener task panicked or was cancelled unexpectedly.
  #[error("management listener task failed: {0}")]
  ListenerTask(tokio::task::JoinError),
  /// The supervised readiness monitor panicked or was cancelled unexpectedly.
  #[error("readiness monitor task failed: {0}")]
  ReadinessTask(tokio::task::JoinError),
  /// The listener stopped without a requested shutdown.
  #[error("management listener exited unexpectedly")]
  UnexpectedExit,
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
  async fn startup_exposes_only_bounded_health_routes_with_request_ids() {
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

    runtime.shutdown().await.unwrap();
  }

  #[tokio::test]
  async fn shutdown_cancels_the_listener_and_waits_for_its_task() {
    let runtime = ServerRuntime::start_with_readiness(test_config(), healthy_checks())
      .await
      .unwrap();
    let address = runtime.management_addr();
    assert!(tokio::net::TcpListener::bind(address).await.is_err());

    runtime.shutdown().await.unwrap();

    let replacement = tokio::net::TcpListener::bind(address)
      .await
      .expect("shutdown must release the management listener");
    assert_eq!(replacement.local_addr().unwrap(), address);
  }

  #[tokio::test]
  async fn wait_reports_a_listener_that_stops_without_shutdown() {
    let cancellation = CancellationToken::new();
    let readiness_cancellation = cancellation.child_token();
    let mut runtime = ServerRuntime {
      management_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
      shutdown_grace: std::time::Duration::from_secs(1),
      readiness: Arc::new(ReadinessState::default()),
      cancellation,
      listener_task: Some(tokio::spawn(async { Ok(()) })),
      readiness_task: Some(tokio::spawn(async move {
        readiness_cancellation.cancelled().await;
      })),
    };

    assert!(matches!(runtime.wait().await, Err(ServerRuntimeError::UnexpectedExit)));
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
    let listener_task = tokio::spawn(async move {
      let _drop_signal = DropSignal(Some(dropped_sender));
      let _ = started_sender.send(());
      std::future::pending::<()>().await;
      Ok(())
    });
    let runtime = ServerRuntime {
      management_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
      shutdown_grace: std::time::Duration::from_secs(1),
      readiness: Arc::new(ReadinessState::default()),
      cancellation: CancellationToken::new(),
      listener_task: Some(listener_task),
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
