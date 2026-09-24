use std::net::SocketAddr;

use thiserror::Error;

/// Failure to start, serve, or gracefully stop the server process.
#[derive(Debug, Error)]
pub enum ServerRuntimeError {
  /// Static runtime resources could not be assembled safely.
  #[error("failed to assemble runtime resources: {0}")]
  Assembly(crate::RuntimeAssemblyError),
  /// Agent registration policy was invalid after configuration loading.
  #[error("agent registration policy is invalid")]
  InvalidAgentPolicy,
  /// Management input or executable-policy configuration was invalid.
  #[error("management application policy is invalid")]
  InvalidManagementPolicy,
  /// Operator-installed webhook adapter registry failed strict verification.
  #[error("webhook adapter registry is invalid: {0}")]
  WebhookRegistry(octacity_server_webhook::RegistryError),
  /// Operator-installed VCS adapter registry failed strict verification.
  #[error("VCS adapter registry is invalid: {0}")]
  VcsRegistry(octacity_server_vcs::RegistryError),
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
  /// The supervised durable-worker group panicked or was cancelled unexpectedly.
  #[error("durable worker task failed: {0}")]
  WorkerTask(tokio::task::JoinError),
  /// The process-local PostgreSQL notification listener panicked or was cancelled unexpectedly.
  #[error("ready-Job notification task failed: {0}")]
  NotificationTask(tokio::task::JoinError),
  /// A supervised process task returned without a shutdown request.
  #[error("supervised task {task} exited unexpectedly")]
  SupervisedTaskUnexpectedExit {
    /// Stable non-sensitive task name.
    task: &'static str,
  },
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
