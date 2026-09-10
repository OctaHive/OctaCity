use std::{path::PathBuf, pin::Pin};

use async_trait::async_trait;
use octacity_protocol::OctaSpec;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite};

pub type ExecutionReader = Pin<Box<dyn AsyncRead + Send>>;
pub type ExecutionWriter = Pin<Box<dyn AsyncWrite + Send>>;

pub struct ExecutionIo {
  pub stdin: ExecutionWriter,
  pub stdout: ExecutionReader,
  pub stderr: ExecutionReader,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartExecution {
  pub execution_id: String,
  pub workspace: PathBuf,
  pub data_dir: PathBuf,
  pub octa: OctaSpec,
  pub cpu_millis: u32,
  pub memory_bytes: u64,
  pub writable_disk_bytes: u64,
}

impl StartExecution {
  pub fn validate(&self) -> Result<(), ExecutionError> {
    if self.execution_id.is_empty() || self.execution_id.chars().any(char::is_control) {
      return Err(ExecutionError::Invalid(
        "execution_id must not be empty or contain control characters".to_owned(),
      ));
    }
    if !self.workspace.is_absolute() || !self.workspace.is_dir() {
      return Err(ExecutionError::Invalid(
        "workspace must be an existing absolute directory".to_owned(),
      ));
    }
    if !self.data_dir.is_absolute() || !self.data_dir.is_dir() || !self.data_dir.starts_with(&self.workspace) {
      return Err(ExecutionError::Invalid(
        "data_dir must be an existing absolute directory inside workspace".to_owned(),
      ));
    }
    if self.cpu_millis == 0 || self.memory_bytes == 0 || self.writable_disk_bytes == 0 {
      return Err(ExecutionError::Invalid(
        "CPU, memory, and writable disk limits must be greater than zero".to_owned(),
      ));
    }
    Ok(())
  }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionPaths {
  pub workspace: PathBuf,
  pub data_dir: PathBuf,
  pub plugins_dir: PathBuf,
  pub plugin_lock: PathBuf,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResourceUsage {
  pub elapsed_ms: u64,
  pub cpu_time_ms: u64,
  pub memory_current_bytes: u64,
  pub memory_peak_bytes: u64,
  pub disk_current_bytes: u64,
  pub disk_peak_bytes: u64,
  pub io_read_bytes: u64,
  pub io_written_bytes: u64,
  pub network_received_bytes: Option<u64>,
  pub network_transmitted_bytes: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionExit {
  pub code: Option<i32>,
}

#[derive(Debug, Error)]
pub enum ExecutionError {
  #[error("invalid execution request: {0}")]
  Invalid(String),
  #[error("execution backend is unavailable: {0}")]
  Unavailable(String),
  #[error("execution backend failed: {0}")]
  Backend(String),
  #[error("execution I/O failed: {0}")]
  Io(#[source] std::io::Error),
  #[error("execution I/O has already been taken")]
  IoTaken,
  #[error("execution process has already been reaped")]
  Reaped,
}

#[async_trait]
pub trait ExecutionBackend: Send + Sync {
  async fn start(&self, request: StartExecution) -> Result<Box<dyn RunningExecution>, ExecutionError>;
  async fn cleanup_orphans(&self) -> Result<(), ExecutionError>;
}

#[async_trait]
pub trait RunningExecution: Send {
  fn paths(&self) -> &ExecutionPaths;
  fn take_io(&mut self) -> Result<ExecutionIo, ExecutionError>;
  async fn sample_usage(&mut self) -> Result<ResourceUsage, ExecutionError>;
  async fn wait(&mut self) -> Result<ExecutionExit, ExecutionError>;
  async fn kill(&mut self) -> Result<(), ExecutionError>;
  async fn destroy(self: Box<Self>) -> Result<(), ExecutionError>;
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn validates_execution_boundaries() {
    let temporary = tempfile::tempdir().unwrap();
    let request = StartExecution {
      execution_id: "job-1-attempt-1".to_owned(),
      workspace: temporary.path().canonicalize().unwrap(),
      data_dir: temporary.path().canonicalize().unwrap().join("data"),
      octa: OctaSpec {
        version: "0.3.0".to_owned(),
        runner_sha256: "0".repeat(64),
        runner_protocol: 1,
        event_schema: 3,
        plugin_protocol: 1,
        plugin_digests: std::collections::BTreeMap::new(),
      },
      cpu_millis: 1000,
      memory_bytes: 512 * 1024 * 1024,
      writable_disk_bytes: 1024 * 1024 * 1024,
    };
    std::fs::create_dir(&request.data_dir).unwrap();
    assert!(request.validate().is_ok());

    let mut invalid = request.clone();
    invalid.execution_id.clear();
    assert!(invalid.validate().is_err());
    invalid = request;
    invalid.memory_bytes = 0;
    assert!(invalid.validate().is_err());
  }
}
