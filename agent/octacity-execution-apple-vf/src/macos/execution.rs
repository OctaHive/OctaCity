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
  usage: ResourceUsage,
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
      let disk_current_bytes = filesystem_usage(&filesystem_root)?;
      Ok::<_, ExecutionError>((stdin, stdout, stderr, disk_current_bytes))
    })();
    let (stdin, stdout, stderr, disk_current_bytes) = match result {
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
      usage: ResourceUsage {
        disk_current_bytes,
        disk_peak_bytes: disk_current_bytes,
        network_received_bytes: Some(0),
        network_transmitted_bytes: Some(0),
        ..ResourceUsage::default()
      },
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
    let statistics = parse_statistics(&output.stdout, &self.plan.container_name)?;
    apply_statistics(&mut self.usage, statistics.as_ref());
    let disk_current_bytes = filesystem_usage(&self.filesystem_root)?;
    self.usage.elapsed_ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
    self.usage.disk_current_bytes = disk_current_bytes;
    self.usage.disk_peak_bytes = self.usage.disk_peak_bytes.max(disk_current_bytes);
    Ok(self.usage.clone())
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

fn parse_statistics(output: &[u8], expected_container: &str) -> Result<Option<AppleContainerStats>, ExecutionError> {
  let mut statistics: Vec<AppleContainerStats> =
    serde_json::from_slice(output).map_err(|error| backend(format!("parse Apple VF resource statistics: {error}")))?;
  if statistics.is_empty() {
    return Ok(None);
  }
  if statistics.len() != 1 || statistics[0].id != expected_container {
    return Err(backend(
      "Apple container returned statistics for an unexpected workload",
    ));
  }
  Ok(statistics.pop())
}

fn apply_statistics(usage: &mut ResourceUsage, statistics: Option<&AppleContainerStats>) {
  usage.memory_current_bytes = 0;
  let Some(statistics) = statistics else {
    return;
  };
  usage.cpu_time_ms = usage.cpu_time_ms.max(statistics.cpu_usage_usec / 1000);
  usage.memory_current_bytes = statistics.memory_usage_bytes;
  usage.memory_peak_bytes = usage.memory_peak_bytes.max(statistics.memory_usage_bytes);
  usage.io_read_bytes = usage.io_read_bytes.max(statistics.block_read_bytes);
  usage.io_written_bytes = usage.io_written_bytes.max(statistics.block_write_bytes);
  usage.network_received_bytes = Some(
    usage
      .network_received_bytes
      .unwrap_or_default()
      .max(statistics.network_rx_bytes),
  );
  usage.network_transmitted_bytes = Some(
    usage
      .network_transmitted_bytes
      .unwrap_or_default()
      .max(statistics.network_tx_bytes),
  );
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn accepts_missing_statistics_after_the_container_exits() {
    assert!(matches!(parse_statistics(b"[]", "octacity-job"), Ok(None)));
  }

  #[test]
  fn rejects_statistics_for_an_unexpected_container() {
    let output = br#"[{"id":"other","memoryUsageBytes":1}]"#;

    assert!(parse_statistics(output, "octacity-job").is_err());
  }

  #[test]
  fn missing_final_statistics_preserve_cumulative_counters() {
    let output = br#"[{"id":"octacity-job","memoryUsageBytes":4096,"cpuUsageUsec":3000,"networkRxBytes":5,"networkTxBytes":6,"blockReadBytes":7,"blockWriteBytes":8}]"#;
    let statistics = parse_statistics(output, "octacity-job").unwrap();
    let mut usage = ResourceUsage::default();

    apply_statistics(&mut usage, statistics.as_ref());
    apply_statistics(&mut usage, None);

    assert_eq!(usage.memory_current_bytes, 0);
    assert_eq!(usage.memory_peak_bytes, 4096);
    assert_eq!(usage.cpu_time_ms, 3);
    assert_eq!((usage.io_read_bytes, usage.io_written_bytes), (7, 8));
    assert_eq!(
      (usage.network_received_bytes, usage.network_transmitted_bytes),
      (Some(5), Some(6))
    );
  }
}
