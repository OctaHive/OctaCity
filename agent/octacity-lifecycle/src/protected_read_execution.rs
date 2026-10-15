//! Couples the private listener to the existing backend execution lifetime.

use async_trait::async_trait;
use octacity_execution::{ExecutionError, ExecutionExit, ExecutionIo, ExecutionPaths, ResourceUsage, RunningExecution};

use super::ProtectedReadChannel;

impl ProtectedReadChannel {
  /// Transfers the listener to the main execution's existing owner. Completion,
  /// kill and destruction all stop pending exchanges and remove the endpoint.
  /// Accounting still comes from the main backend; this does not qualify a
  /// separately confined helper or account for its physical resource usage.
  pub fn attach(self, execution: Box<dyn RunningExecution>) -> Box<dyn RunningExecution> {
    Box::new(ReadChannelExecution {
      execution,
      channel: Some(self),
    })
  }
}

struct ReadChannelExecution {
  execution: Box<dyn RunningExecution>,
  channel: Option<ProtectedReadChannel>,
}

impl ReadChannelExecution {
  async fn stop_channel(&mut self) -> Result<(), ExecutionError> {
    match self.channel.take() {
      Some(channel) => channel.shutdown().await,
      None => Ok(()),
    }
  }
}

#[async_trait]
impl RunningExecution for ReadChannelExecution {
  fn paths(&self) -> &ExecutionPaths {
    self.execution.paths()
  }

  fn take_io(&mut self) -> Result<ExecutionIo, ExecutionError> {
    self.execution.take_io()
  }

  async fn sample_usage(&mut self) -> Result<ResourceUsage, ExecutionError> {
    self.execution.sample_usage().await
  }

  async fn sample_final_usage(&mut self) -> Result<ResourceUsage, ExecutionError> {
    self.stop_channel().await?;
    self.execution.sample_final_usage().await
  }

  async fn close_input(&mut self) -> Result<(), ExecutionError> {
    self.execution.close_input().await
  }

  async fn wait(&mut self) -> Result<ExecutionExit, ExecutionError> {
    let result = self.execution.wait().await;
    let cleanup = self.stop_channel().await;
    with_channel_cleanup(result, cleanup)
  }

  async fn kill(&mut self) -> Result<(), ExecutionError> {
    let cleanup = self.stop_channel().await;
    let result = self.execution.kill().await;
    with_channel_cleanup(result, cleanup)
  }

  async fn destroy(mut self: Box<Self>) -> Result<(), ExecutionError> {
    let cleanup = self.stop_channel().await;
    let result = self.execution.destroy().await;
    with_channel_cleanup(result, cleanup)
  }
}

fn with_channel_cleanup<T>(
  result: Result<T, ExecutionError>,
  cleanup: Result<(), ExecutionError>,
) -> Result<T, ExecutionError> {
  match (result, cleanup) {
    (Ok(value), Ok(())) => Ok(value),
    (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
    (Err(error), Err(cleanup)) => Err(error.with_cleanup(cleanup)),
  }
}
