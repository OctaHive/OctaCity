use std::{net::SocketAddr, sync::Arc};

use thiserror::Error;
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::ServerConfig;

/// Running server process and ownership handle for its cancellation tree.
#[must_use = "dropping the runtime aborts the listener; call shutdown for graceful drain"]
pub struct ServerRuntime {
  management_addr: SocketAddr,
  shutdown_grace: std::time::Duration,
  health: Arc<HealthState>,
  cancellation: CancellationToken,
  listener_task: Option<JoinHandle<Result<(), std::io::Error>>>,
}

impl ServerRuntime {
  /// Binds the listener and starts supervised work from validated configuration.
  pub async fn start(config: ServerConfig) -> Result<Self, ServerRuntimeError> {
    let listener = TcpListener::bind(config.management_bind())
      .await
      .map_err(|source| ServerRuntimeError::Bind {
        address: config.management_bind(),
        source,
      })?;
    let management_addr = listener.local_addr().map_err(ServerRuntimeError::InspectListener)?;
    let health = Arc::new(HealthState::default());
    let cancellation = CancellationToken::new();
    let listener_cancellation = cancellation.child_token();
    let router_health = health.clone();
    let router = octacity_server_api_rest::management_router(move || router_health.is_ready());
    let listener_health = health.clone();
    let listener_task = tokio::spawn(async move {
      let result = axum::serve(listener, router)
        .with_graceful_shutdown(listener_cancellation.cancelled_owned())
        .await;
      listener_health.ready.store(false, std::sync::atomic::Ordering::Release);
      result
    });
    health.ready.store(true, std::sync::atomic::Ordering::Release);
    info!(%management_addr, "server management listener ready");
    Ok(Self {
      management_addr,
      shutdown_grace: config.shutdown_grace(),
      health,
      cancellation,
      listener_task: Some(listener_task),
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
    match result {
      Ok(Ok(())) => Err(ServerRuntimeError::UnexpectedExit),
      Ok(Err(source)) => Err(ServerRuntimeError::Serve(source)),
      Err(source) => Err(ServerRuntimeError::ListenerTask(source)),
    }
  }

  /// Stops admission, cancels the process tree, and waits for bounded drain.
  pub async fn shutdown(mut self) -> Result<(), ServerRuntimeError> {
    self.health.ready.store(false, std::sync::atomic::Ordering::Release);
    self.cancellation.cancel();
    let Some(mut listener_task) = self.listener_task.take() else {
      return Err(ServerRuntimeError::UnexpectedExit);
    };
    match tokio::time::timeout(self.shutdown_grace, &mut listener_task).await {
      Ok(Ok(Ok(()))) => {
        info!(%self.management_addr, "server shutdown complete");
        Ok(())
      }
      Ok(Ok(Err(source))) => Err(ServerRuntimeError::Serve(source)),
      Ok(Err(source)) => Err(ServerRuntimeError::ListenerTask(source)),
      Err(_) => {
        listener_task.abort();
        let _ = listener_task.await;
        Err(ServerRuntimeError::ShutdownTimeout(self.shutdown_grace))
      }
    }
  }
}

impl Drop for ServerRuntime {
  fn drop(&mut self) {
    self.health.ready.store(false, std::sync::atomic::Ordering::Release);
    self.cancellation.cancel();
    if let Some(listener_task) = self.listener_task.take() {
      listener_task.abort();
    }
  }
}

#[derive(Default)]
struct HealthState {
  ready: std::sync::atomic::AtomicBool,
}

impl HealthState {
  fn is_ready(&self) -> bool {
    self.ready.load(std::sync::atomic::Ordering::Acquire)
  }
}

/// Failure to start, serve, or gracefully stop the server process.
#[derive(Debug, Error)]
pub enum ServerRuntimeError {
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
  /// The listener stopped without a requested shutdown.
  #[error("management listener exited unexpectedly")]
  UnexpectedExit,
  /// In-flight work did not drain within the configured grace period.
  #[error("server did not shut down within {0:?}")]
  ShutdownTimeout(std::time::Duration),
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn wait_reports_a_listener_that_stops_without_shutdown() {
    let mut runtime = ServerRuntime {
      management_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
      shutdown_grace: std::time::Duration::from_secs(1),
      health: Arc::new(HealthState::default()),
      cancellation: CancellationToken::new(),
      listener_task: Some(tokio::spawn(async { Ok(()) })),
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
      health: Arc::new(HealthState::default()),
      cancellation: CancellationToken::new(),
      listener_task: Some(listener_task),
    };

    started_receiver.await.unwrap();
    drop(runtime);
    tokio::time::timeout(std::time::Duration::from_secs(1), dropped_receiver)
      .await
      .expect("aborted listener future must be dropped")
      .unwrap();
  }
}
