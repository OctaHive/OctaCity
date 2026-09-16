//! Owns the protocol streams and lifecycle of one Microsandbox execution.

use super::*;

/// Owns a live microVM, its runner stream bridge, and accumulated accounting.
///
/// Destruction consumes this handle so no successful path can accidentally
/// retain the VM or its forwarding tasks.
pub(super) struct MicrosandboxExecution {
  pub(super) sandbox: Sandbox,
  pub(super) control: ExecControl,
  pub(super) io: Option<ExecutionIo>,
  pub(super) paths: ExecutionPaths,
  pub(super) exit_receiver: Option<oneshot::Receiver<Result<i32, String>>>,
  pub(super) exit_code: Option<i32>,
  pub(super) stdin_task: JoinHandle<()>,
  pub(super) event_task: JoinHandle<()>,
  pub(super) cleanup_timeout: Duration,
  pub(super) started: Instant,
  pub(super) host_workspace: PathBuf,
  pub(super) memory_peak_bytes: u64,
  pub(super) disk_peak_bytes: u64,
}

#[async_trait]
impl RunningExecution for MicrosandboxExecution {
  fn paths(&self) -> &ExecutionPaths {
    &self.paths
  }

  fn take_io(&mut self) -> Result<ExecutionIo, ExecutionError> {
    self.io.take().ok_or(ExecutionError::IoTaken)
  }

  async fn sample_usage(&mut self) -> Result<ResourceUsage, ExecutionError> {
    let metrics = self.sandbox.metrics().await.map_err(msb_error)?;
    self.memory_peak_bytes = self.memory_peak_bytes.max(metrics.memory_bytes);
    let disk_current_bytes = directory_size_async(self.host_workspace.clone()).await?;
    self.disk_peak_bytes = self.disk_peak_bytes.max(disk_current_bytes);
    Ok(ResourceUsage {
      elapsed_ms: millis(self.started.elapsed()),
      cpu_time_ms: metrics.vcpu_time_ns / 1_000_000,
      memory_current_bytes: metrics.memory_bytes,
      memory_peak_bytes: self.memory_peak_bytes,
      disk_current_bytes,
      disk_peak_bytes: self.disk_peak_bytes,
      io_read_bytes: metrics.disk_read_bytes,
      io_written_bytes: metrics.disk_write_bytes,
      network_received_bytes: Some(metrics.net_rx_bytes),
      network_transmitted_bytes: Some(metrics.net_tx_bytes),
    })
  }

  async fn wait(&mut self) -> Result<ExecutionExit, ExecutionError> {
    if self.exit_code.is_some() {
      return Err(ExecutionError::Reaped);
    }
    let receiver = self.exit_receiver.take().ok_or(ExecutionError::Reaped)?;
    let code = receiver
      .await
      .map_err(|_| backend("Microsandbox exec stream ended without a terminal result"))?
      .map_err(backend)?;
    self.exit_code = Some(code);
    Ok(ExecutionExit { code: Some(code) })
  }

  async fn kill(&mut self) -> Result<(), ExecutionError> {
    if let Err(error) = self.control.kill().await {
      debug!(%error, "Microsandbox runner did not accept cooperative termination");
    }
    self.sandbox.kill().await.map_err(msb_error)
  }

  async fn destroy(self: Box<Self>) -> Result<(), ExecutionError> {
    let MicrosandboxExecution {
      sandbox,
      stdin_task,
      event_task,
      cleanup_timeout,
      ..
    } = *self;
    // Stop bridges before removing the sandbox; otherwise an SDK stream may
    // keep a task alive while backend state is being destroyed.
    stdin_task.abort();
    event_task.abort();
    let task_cleanup = timeout(cleanup_timeout, async {
      let _ = stdin_task.await;
      let _ = event_task.await;
    })
    .await
    .map_err(|_| backend("Microsandbox stream tasks did not stop within the cleanup timeout"));
    let sandbox_cleanup = cleanup_sandbox_result(&sandbox, cleanup_timeout).await;
    match (task_cleanup, sandbox_cleanup) {
      (Ok(()), Ok(())) => Ok(()),
      (Err(tasks), Ok(())) => Err(tasks),
      (Ok(()), Err(sandbox)) => Err(sandbox),
      (Err(tasks), Err(sandbox)) => Err(backend(format!("{tasks}; sandbox cleanup also failed: {sandbox}"))),
    }
  }
}

/// Copies runner protocol input into the SDK exec stream until either side closes.
pub(super) async fn pump_stdin(mut input: tokio::io::DuplexStream, sink: microsandbox::sandbox::exec::ExecSink) {
  let mut buffer = [0_u8; 8192];
  loop {
    match input.read(&mut buffer).await {
      Ok(0) => {
        let _ = sink.close().await;
        return;
      }
      Ok(read) => {
        if sink.write(&buffer[..read]).await.is_err() {
          return;
        }
      }
      Err(_) => return,
    }
  }
}

/// Separates SDK stdout/stderr events and reports exactly one terminal result.
pub(super) async fn pump_events(
  mut handle: microsandbox::ExecHandle,
  mut stdout: tokio::io::DuplexStream,
  mut stderr: tokio::io::DuplexStream,
  exit: oneshot::Sender<Result<i32, String>>,
) {
  let result = loop {
    match handle.recv().await {
      Some(ExecEvent::Started { .. } | ExecEvent::StdinError(_)) => {}
      Some(ExecEvent::Stdout(bytes)) => {
        if let Err(error) = stdout.write_all(&bytes).await {
          break Err(format!("forward runner stdout: {error}"));
        }
      }
      Some(ExecEvent::Stderr(bytes)) => {
        if let Err(error) = stderr.write_all(&bytes).await {
          break Err(format!("forward runner stderr: {error}"));
        }
      }
      Some(ExecEvent::Exited { code }) => break Ok(code),
      Some(ExecEvent::Failed(error)) => break Err(format!("start runner in Microsandbox: {error:?}")),
      None => break Err("Microsandbox exec stream closed before exit".to_owned()),
    }
  };
  let _ = exit.send(result);
}
