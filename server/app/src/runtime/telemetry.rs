use std::{
  sync::{Mutex, OnceLock},
  time::Duration,
};

use async_trait::async_trait;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use octacity_observability::MetricScope;
use octacity_server_application::{AgentTelemetryBatch, AgentTelemetryExportError, AgentTelemetryExporter};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::warn;

static SERVER_METRICS: OnceLock<PrometheusHandle> = OnceLock::new();
static SERVER_METRICS_INSTALL: Mutex<()> = Mutex::new(());
const METRICS_UPKEEP_INTERVAL: Duration = Duration::from_secs(5);

pub(super) fn install_server_metrics() -> Option<PrometheusHandle> {
  if let Some(handle) = SERVER_METRICS.get() {
    return Some(handle.clone());
  }
  let Ok(_installation) = SERVER_METRICS_INSTALL.lock() else {
    warn!("server metrics recorder unavailable");
    return None;
  };
  if let Some(handle) = SERVER_METRICS.get() {
    return Some(handle.clone());
  }
  let Ok(handle) = PrometheusBuilder::new()
    .with_recommended_naming(true)
    .install_recorder()
  else {
    warn!("server metrics recorder unavailable");
    return None;
  };
  if SERVER_METRICS.set(handle.clone()).is_err() {
    return SERVER_METRICS.get().cloned();
  }
  octacity_observability::describe_metrics(MetricScope::Server);
  Some(handle)
}

pub(super) fn spawn_metrics_upkeep(handle: PrometheusHandle, cancellation: CancellationToken) -> JoinHandle<()> {
  tokio::spawn(async move {
    let mut interval = tokio::time::interval(METRICS_UPKEEP_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
      tokio::select! {
        biased;
        () = cancellation.cancelled() => return,
        _ = interval.tick() => handle.run_upkeep(),
      }
    }
  })
}

/// Explicit null adapter used until a diagnostic exporter is configured.
pub(super) struct UnavailableAgentTelemetryExporter;

#[async_trait]
impl AgentTelemetryExporter for UnavailableAgentTelemetryExporter {
  async fn export(&self, _batch: AgentTelemetryBatch) -> Result<(), AgentTelemetryExportError> {
    Err(AgentTelemetryExportError)
  }
}
