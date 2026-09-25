use std::{net::SocketAddr, sync::Arc};

use tokio::task::{JoinHandle, JoinSet};
use tokio_util::sync::CancellationToken;

use crate::readiness::ReadinessState;

mod application;
mod assembly;
mod composition;
mod error;
mod lifecycle;
mod listeners;
mod maintenance;
mod notifications;
mod telemetry;
mod vcs;
mod webhook;
mod workers;

pub use assembly::RuntimeAssemblyError;
pub use error::{DurableWorkerError, ServerRuntimeError};
pub use maintenance::{LogSearchMaintenanceError, rebuild_log_search};

type ListenerTaskResult = Result<&'static str, (&'static str, std::io::Error)>;

/// Running server process and ownership handle for its cancellation tree.
#[must_use = "dropping the runtime aborts its listeners; call shutdown for graceful drain"]
pub struct ServerRuntime {
  management_addr: SocketAddr,
  agent_addr: Option<SocketAddr>,
  cache_addr: Option<SocketAddr>,
  webhook_addr: Option<SocketAddr>,
  shutdown_grace: std::time::Duration,
  readiness: Arc<ReadinessState>,
  cancellation: CancellationToken,
  listener_tasks: JoinSet<ListenerTaskResult>,
  readiness_task: Option<JoinHandle<()>>,
  worker_task: Option<JoinHandle<Result<(), DurableWorkerError>>>,
  notification_task: Option<JoinHandle<()>>,
  metrics_task: Option<JoinHandle<()>>,
}

impl ServerRuntime {
  /// Actual bound management address, including an OS-assigned port.
  pub const fn management_addr(&self) -> SocketAddr {
    self.management_addr
  }

  /// Actual bound Agent address when authenticated Agent ingress is configured.
  pub const fn agent_addr(&self) -> Option<SocketAddr> {
    self.agent_addr
  }

  /// Actual bound cache address when authenticated Octa cache ingress is configured.
  pub const fn cache_addr(&self) -> Option<SocketAddr> {
    self.cache_addr
  }

  /// Actual bound webhook address when authenticated webhook ingress is configured.
  pub const fn webhook_addr(&self) -> Option<SocketAddr> {
    self.webhook_addr
  }
}

#[cfg(test)]
mod tests;
