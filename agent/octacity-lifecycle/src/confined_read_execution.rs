//! Transfers physical reader teardown to the existing main execution owner.

use std::sync::{Arc, atomic::Ordering};

use async_trait::async_trait;
use octacity_execution::{ExecutionError, ExecutionExit, ExecutionIo, ExecutionPaths, ResourceUsage, RunningExecution};

use super::ConfinedProtectedReadBackend;

impl ConfinedProtectedReadBackend {
  /// Couples physical helper teardown to the main execution's existing owner.
  /// Wait, kill and destruction await every reader before returning. This
  /// attachment alone does not provide live combined resource accounting.
  pub fn attach(self: Arc<Self>, execution: Box<dyn RunningExecution>) -> Box<dyn RunningExecution> {
    Box::new(ReaderOwnedExecution {
      execution: Some(execution),
      reader: self,
    })
  }
}

struct ReaderOwnedExecution {
  execution: Option<Box<dyn RunningExecution>>,
  reader: Arc<ConfinedProtectedReadBackend>,
}

impl ReaderOwnedExecution {
  fn main(&mut self) -> &mut dyn RunningExecution {
    self
      .execution
      .as_deref_mut()
      .expect("main execution retained until destruction")
  }
}

impl Drop for ReaderOwnedExecution {
  fn drop(&mut self) {
    // A dropped owner cannot join asynchronously. The tracked helper task
    // retains its real handle and completes teardown; normal supervision
    // always awaits destroy to observe failures before reporting completion.
    self.reader.shared.stopped.store(true, Ordering::SeqCst);
    self.reader.shared.cancellation.cancel();
  }
}

#[async_trait]
impl RunningExecution for ReaderOwnedExecution {
  fn paths(&self) -> &ExecutionPaths {
    self
      .execution
      .as_deref()
      .expect("main execution retained until destruction")
      .paths()
  }
  fn take_io(&mut self) -> Result<ExecutionIo, ExecutionError> {
    self.main().take_io()
  }
  async fn sample_usage(&mut self) -> Result<ResourceUsage, ExecutionError> {
    self.main().sample_usage().await
  }
  async fn sample_final_usage(&mut self) -> Result<ResourceUsage, ExecutionError> {
    self.reader.shutdown().await?;
    let mut main = self.main().sample_usage().await?;
    let completed = self.reader.shared.usage.lock().map_err(|_| super::cleanup_error())?;
    if let Some(helper) = completed.as_ref() {
      let add = |left: u64, right: u64| left.checked_add(right).ok_or_else(super::cleanup_error);
      let network = |left: Option<u64>, right: Option<u64>| match (left, right) {
        (Some(left), Some(right)) => add(left, right).map(Some),
        _ => Ok(None),
      };
      // Concurrent durations do not add to the Job wall clock. Distinct
      // writable mounts do add; summed part peaks conservatively account
      // for the concurrently reserved main/helper allocations.
      main.cpu_time_ms = add(main.cpu_time_ms, helper.cpu_time_ms)?;
      main.io_read_bytes = add(main.io_read_bytes, helper.io_read_bytes)?;
      main.io_written_bytes = add(main.io_written_bytes, helper.io_written_bytes)?;
      main.memory_peak_bytes = add(main.memory_peak_bytes, helper.memory_peak_bytes)?;
      main.disk_current_bytes = add(main.disk_current_bytes, helper.disk_current_bytes)?;
      main.disk_peak_bytes = add(main.disk_peak_bytes, helper.disk_peak_bytes)?;
      main.network_received_bytes = network(main.network_received_bytes, helper.network_received_bytes)?;
      main.network_transmitted_bytes = network(main.network_transmitted_bytes, helper.network_transmitted_bytes)?;
    }
    Ok(main)
  }
  async fn close_input(&mut self) -> Result<(), ExecutionError> {
    self.main().close_input().await
  }
  async fn wait(&mut self) -> Result<ExecutionExit, ExecutionError> {
    let result = self.main().wait().await;
    combine(result, self.reader.shutdown().await)
  }
  async fn kill(&mut self) -> Result<(), ExecutionError> {
    let result = self.main().kill().await;
    combine(result, self.reader.shutdown().await)
  }
  async fn destroy(mut self: Box<Self>) -> Result<(), ExecutionError> {
    let cleanup = self.reader.shutdown().await;
    let result = self
      .execution
      .take()
      .expect("main execution retained until destruction")
      .destroy()
      .await;
    combine(result, cleanup)
  }
}

fn combine<T>(result: Result<T, ExecutionError>, cleanup: Result<(), ExecutionError>) -> Result<T, ExecutionError> {
  match (result, cleanup) {
    (Ok(value), Ok(())) => Ok(value),
    (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
    (Err(error), Err(cleanup)) => Err(error.with_cleanup(cleanup)),
  }
}
