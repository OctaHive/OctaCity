//! Owns one attached Apple container process and its durable cleanup marker.

use std::{path::PathBuf, time::Duration};

use async_trait::async_trait;
use octacity_execution::{ExecutionError, ExecutionExit, ExecutionIo, ExecutionPaths, ResourceUsage, RunningExecution};
use serde::Deserialize;
use tokio::{process::Child, time::Instant};

use super::{AppleVfPlan, backend, control_output, delete_container, filesystem::filesystem_usage};

pub(super) struct AppleVfExecution {
  executable: PathBuf,
  plan: AppleVfPlan,
  child: Option<Child>,
  io: Option<ExecutionIo>,
  filesystem_root: PathBuf,
  cleanup_timeout: Duration,
  started: Instant,
  memory_peak_bytes: u64,
  disk_peak_bytes: u64,
  removed: bool,
}

impl AppleVfExecution {
  pub(super) fn new(
    executable: PathBuf,
    plan: AppleVfPlan,
    mut child: Child,
    filesystem_root: PathBuf,
    cleanup_timeout: Duration,
  ) -> Result<Self, Box<(AppleVfPlan, ExecutionError)>> {
    let result = (|| {
      let stdin = child.stdin.take().ok_or(ExecutionError::IoTaken)?;
      let stdout = child.stdout.take().ok_or(ExecutionError::IoTaken)?;
      let stderr = child.stderr.take().ok_or(ExecutionError::IoTaken)?;
      let disk_peak_bytes = filesystem_usage(&filesystem_root)?;
      Ok::<_, ExecutionError>((stdin, stdout, stderr, disk_peak_bytes))
    })();
    let (stdin, stdout, stderr, disk_peak_bytes) = match result {
      Ok(streams) => streams,
      Err(error) => return Err(Box::new((plan, error))),
    };
    Ok(Self {
      executable,
      plan,
      child: Some(child),
      io: Some(ExecutionIo {
        stdin: Box::pin(stdin),
        stdout: Box::pin(stdout),
        stderr: Box::pin(stderr),
      }),
      filesystem_root,
      cleanup_timeout,
      started: Instant::now(),
      memory_peak_bytes: 0,
      disk_peak_bytes,
      removed: false,
    })
  }
}

#[async_trait]
impl RunningExecution for AppleVfExecution {
  fn paths(&self) -> &ExecutionPaths {
    &self.plan.paths
  }

  fn take_io(&mut self) -> Result<ExecutionIo, ExecutionError> {
    self.io.take().ok_or(ExecutionError::IoTaken)
  }

  async fn sample_usage(&mut self) -> Result<ResourceUsage, ExecutionError> {
    let output = control_output(
      &self.executable,
      &["stats", "--format", "json", "--no-stream", &self.plan.container_name],
      self.cleanup_timeout,
      None,
      "sample Apple VF container usage",
    )
    .await?;
    let mut statistics: Vec<AppleContainerStats> = serde_json::from_slice(&output.stdout)
      .map_err(|error| backend(format!("parse Apple VF resource statistics: {error}")))?;
    if statistics.len() != 1 || statistics[0].id != self.plan.container_name {
      return Err(backend(
        "Apple container returned statistics for an unexpected workload",
      ));
    }
    let statistics = statistics.pop().expect("length checked above");
    self.memory_peak_bytes = self.memory_peak_bytes.max(statistics.memory_usage_bytes);
    let disk_current_bytes = filesystem_usage(&self.filesystem_root)?;
    self.disk_peak_bytes = self.disk_peak_bytes.max(disk_current_bytes);
    Ok(ResourceUsage {
      elapsed_ms: u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX),
      cpu_time_ms: statistics.cpu_usage_usec / 1000,
      memory_current_bytes: statistics.memory_usage_bytes,
      memory_peak_bytes: self.memory_peak_bytes,
      disk_current_bytes,
      disk_peak_bytes: self.disk_peak_bytes,
      io_read_bytes: statistics.block_read_bytes,
      io_written_bytes: statistics.block_write_bytes,
      network_received_bytes: Some(statistics.network_rx_bytes),
      network_transmitted_bytes: Some(statistics.network_tx_bytes),
    })
  }

  async fn wait(&mut self) -> Result<ExecutionExit, ExecutionError> {
    let mut child = self.child.take().ok_or(ExecutionError::Reaped)?;
    let status = child.wait().await.map_err(ExecutionError::Io)?;
    Ok(ExecutionExit { code: status.code() })
  }

  async fn kill(&mut self) -> Result<(), ExecutionError> {
    if self.removed {
      return Ok(());
    }
    delete_container(&self.executable, &self.plan.container_name, self.cleanup_timeout).await?;
    self.removed = true;
    Ok(())
  }

  async fn destroy(mut self: Box<Self>) -> Result<(), ExecutionError> {
    let resource = if self.removed {
      Ok(())
    } else {
      delete_container(&self.executable, &self.plan.container_name, self.cleanup_timeout).await
    };
    self.removed = true;
    let marker = self.plan.remove_marker();
    match (resource, marker) {
      (Ok(()), Ok(())) => Ok(()),
      (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
      (Err(resource), Err(marker)) => Err(backend(format!("{resource}; marker cleanup also failed: {marker}"))),
    }
  }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppleContainerStats {
  id: String,
  memory_usage_bytes: u64,
  #[serde(default)]
  cpu_usage_usec: u64,
  #[serde(default)]
  network_rx_bytes: u64,
  #[serde(default)]
  network_tx_bytes: u64,
  #[serde(default)]
  block_read_bytes: u64,
  #[serde(default)]
  block_write_bytes: u64,
}
