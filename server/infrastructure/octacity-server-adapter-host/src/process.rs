use std::{future::Future, process::Stdio, time::Duration};

use processkit::ProcessGroup;
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use tokio::{
  io::{AsyncBufRead, AsyncBufReadExt as _, AsyncRead, AsyncReadExt as _, AsyncWriteExt as _, BufReader},
  process::{Child, Command},
  time::{Instant, sleep_until, timeout, timeout_at},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::registry::{MAX_EXECUTABLE_BYTES, VerifiedExecutable, validate_regular_file};

const MAX_STDERR_BYTES: usize = 64 * 1024;

/// Stable classification for host-side failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostFailureClass {
  /// Caller input is invalid.
  InvalidRequest,
  /// The verified adapter did not advertise the operation.
  Unsupported,
  /// Installation or digest state requires operator intervention.
  Permanent,
  /// A crash, timeout, spawn, or I/O failure may succeed on bounded retry.
  Transient,
  /// The caller cancelled the operation.
  Cancelled,
  /// The adapter violated framing, correlation, or outcome semantics.
  ProtocolFault,
}

/// Failure to execute one verified adapter operation safely.
#[derive(Debug, Error)]
pub enum HostError {
  /// Request validation failed before the adapter was started.
  #[error("invalid adapter request: {message}")]
  InvalidRequest {
    /// Secret-free validation diagnostic.
    message: String,
  },
  /// The selected adapter does not advertise the requested operation.
  #[error("adapter '{adapter}' does not support '{operation}'")]
  Unsupported {
    /// Logical adapter identity.
    adapter: String,
    /// Stable operation name.
    operation: &'static str,
  },
  /// The executable no longer matches its verified registry entry.
  #[error("adapter '{adapter}' changed after registry verification")]
  ExecutableChanged {
    /// Logical adapter identity.
    adapter: String,
  },
  /// The verified adapter could not be started.
  #[error("failed to start adapter '{adapter}': {source}")]
  Spawn {
    /// Logical adapter identity.
    adapter: String,
    /// Underlying process error.
    source: std::io::Error,
  },
  /// A bounded process pipe failed.
  #[error("adapter '{adapter}' process I/O failed: {source}")]
  Io {
    /// Logical adapter identity.
    adapter: String,
    /// Underlying pipe error.
    source: std::io::Error,
  },
  /// The adapter violated framing, correlation, or outcome semantics.
  #[error("adapter '{adapter}' violated its protocol: {message}")]
  ProtocolFault {
    /// Logical adapter identity.
    adapter: String,
    /// Secret-free protocol diagnostic.
    message: String,
  },
  /// The adapter exited unsuccessfully without a valid terminal response.
  #[error("adapter '{adapter}' crashed")]
  Crashed {
    /// Logical adapter identity.
    adapter: String,
  },
  /// The complete operation exceeded its absolute timeout.
  #[error("adapter '{adapter}' timed out")]
  TimedOut {
    /// Logical adapter identity.
    adapter: String,
  },
  /// The caller cancelled the operation.
  #[error("adapter '{adapter}' was cancelled")]
  Cancelled {
    /// Logical adapter identity.
    adapter: String,
  },
}

impl HostError {
  /// Maps host mechanics onto the retry vocabulary shared by adapter protocols.
  #[must_use]
  pub const fn class(&self) -> HostFailureClass {
    match self {
      Self::InvalidRequest { .. } => HostFailureClass::InvalidRequest,
      Self::Unsupported { .. } => HostFailureClass::Unsupported,
      Self::ExecutableChanged { .. } => HostFailureClass::Permanent,
      Self::Spawn { .. } | Self::Io { .. } | Self::Crashed { .. } | Self::TimedOut { .. } => {
        HostFailureClass::Transient
      }
      Self::Cancelled { .. } => HostFailureClass::Cancelled,
      Self::ProtocolFault { .. } => HostFailureClass::ProtocolFault,
    }
  }
}

/// Fully encoded protocol frames and immutable executable identity for one operation.
pub struct ProcessRequest<'a> {
  executable: &'a VerifiedExecutable,
  request_id: &'a str,
  request_frame: Vec<u8>,
  cancellation_frame: Option<Vec<u8>>,
  max_message_bytes: usize,
}

impl<'a> ProcessRequest<'a> {
  /// Creates one process request from protocol-owned encoded frames.
  #[must_use]
  pub fn new(
    executable: &'a VerifiedExecutable,
    request_id: &'a str,
    request_frame: Vec<u8>,
    cancellation_frame: Option<Vec<u8>>,
    max_message_bytes: usize,
  ) -> Self {
    Self {
      executable,
      request_id,
      request_frame,
      cancellation_frame,
      max_message_bytes,
    }
  }
}

/// Executes one environment-cleared process and validates its first response before bounded cleanup.
pub async fn execute_process<T, Decode>(
  request: ProcessRequest<'_>,
  operation_timeout: Duration,
  cancellation_grace: Duration,
  cancellation: CancellationToken,
  decode: Decode,
) -> Result<T, HostError>
where
  Decode: FnOnce(&[u8]) -> Result<T, String>,
{
  let adapter = request.executable.adapter_id().to_owned();
  if operation_timeout.is_zero() || cancellation_grace.is_zero() || request.max_message_bytes == 0 {
    return Err(HostError::InvalidRequest {
      message: "operation timeout, cancellation grace, and message limit must be greater than zero".to_owned(),
    });
  }
  let deadline = Instant::now()
    .checked_add(operation_timeout)
    .ok_or_else(|| HostError::InvalidRequest {
      message: "operation timeout is too large".to_owned(),
    })?;
  run_before_deadline(
    &adapter,
    deadline,
    &cancellation,
    verify_current_executable(request.executable),
  )
  .await?;
  let mut command = Command::new(request.executable.path());
  command
    .env_clear()
    .current_dir(
      request
        .executable
        .path()
        .parent()
        .ok_or_else(|| HostError::ExecutableChanged {
          adapter: adapter.clone(),
        })?,
    )
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
  let process_group = ProcessGroup::new().map_err(|source| HostError::Spawn {
    adapter: adapter.clone(),
    source: std::io::Error::other(source),
  })?;
  let child = process_group.spawn(command).map_err(|source| HostError::Spawn {
    adapter: adapter.clone(),
    source: std::io::Error::other(source),
  })?;
  let mut child = ChildGuard::new(child, process_group);

  if let Err(error) = run_before_deadline(
    &adapter,
    deadline,
    &cancellation,
    verify_current_executable(request.executable),
  )
  .await
  {
    child.force_kill_and_wait().await;
    return Err(error);
  }

  let Some(mut stdin) = child.child.stdin.take() else {
    child.force_kill_and_wait().await;
    return Err(protocol(&adapter, "stdin was not piped"));
  };
  let Some(stdout) = child.child.stdout.take() else {
    child.force_kill_and_wait().await;
    return Err(protocol(&adapter, "stdout was not piped"));
  };
  let Some(stderr) = child.child.stderr.take() else {
    child.force_kill_and_wait().await;
    return Err(protocol(&adapter, "stderr was not piped"));
  };
  let mut stdout = BufReader::new(stdout);
  let mut stderr_task = Some(tokio::spawn(read_bounded(stderr, MAX_STDERR_BYTES)));

  let write_result = tokio::select! {
    biased;
    () = cancellation.cancelled() => Err(HostError::Cancelled { adapter: adapter.clone() }),
    () = sleep_until(deadline) => Err(HostError::TimedOut { adapter: adapter.clone() }),
    result = write_frame(&adapter, &mut stdin, &request.request_frame, request.max_message_bytes) => result,
  };
  if let Err(error) = write_result {
    abort_process(&mut child, &mut stderr_task, cancellation_grace).await;
    return Err(error);
  }

  enum ReadResult {
    Frame(Vec<u8>),
    Cancelled,
    TimedOut,
  }
  let read_result = tokio::select! {
    biased;
    () = cancellation.cancelled() => ReadResult::Cancelled,
    () = sleep_until(deadline) => ReadResult::TimedOut,
    result = read_frame(&adapter, &mut stdout, request.max_message_bytes) => match result {
      Ok(Some(frame)) => ReadResult::Frame(frame),
      Ok(None) => {
        let error = match timeout_at(deadline, child.wait()).await {
          Ok(Ok(status)) if !status.success() => HostError::Crashed { adapter: adapter.clone() },
          Ok(Ok(_)) => protocol(&adapter, "stdout closed before a response"),
          Ok(Err(source)) => HostError::Io { adapter: adapter.clone(), source },
          Err(_) => HostError::TimedOut { adapter: adapter.clone() },
        };
        abort_process(&mut child, &mut stderr_task, cancellation_grace).await;
        return Err(error);
      }
      Err(error) => {
        abort_process(&mut child, &mut stderr_task, cancellation_grace).await;
        return Err(error);
      }
    },
  };

  let frame = match read_result {
    ReadResult::Frame(frame) => frame,
    ReadResult::Cancelled => {
      stop_operation(
        &adapter,
        &request,
        &mut child,
        stdin,
        stdout,
        &mut stderr_task,
        cancellation_grace,
      )
      .await;
      return Err(HostError::Cancelled { adapter });
    }
    ReadResult::TimedOut => {
      stop_operation(
        &adapter,
        &request,
        &mut child,
        stdin,
        stdout,
        &mut stderr_task,
        cancellation_grace,
      )
      .await;
      return Err(HostError::TimedOut { adapter });
    }
  };
  drop(stdin);

  let decoded = match decode(&frame) {
    Ok(decoded) => decoded,
    Err(message) => {
      abort_process(&mut child, &mut stderr_task, cancellation_grace).await;
      return Err(protocol(&adapter, message));
    }
  };
  let stdout_task = tokio::spawn(read_bounded(stdout, request.max_message_bytes));
  if let Err(error) = finish_process(
    &adapter,
    &mut child,
    stdout_task,
    &mut stderr_task,
    deadline,
    cancellation_grace,
  )
  .await
  {
    child.force_kill_and_wait().await;
    return Err(error);
  }
  debug!(adapter = %adapter, request_id = request.request_id, "adapter operation completed");
  Ok(decoded)
}

async fn run_before_deadline<F>(
  adapter: &str,
  deadline: Instant,
  cancellation: &CancellationToken,
  operation: F,
) -> Result<(), HostError>
where
  F: Future<Output = Result<(), HostError>>,
{
  tokio::select! {
    biased;
    () = cancellation.cancelled() => Err(HostError::Cancelled { adapter: adapter.to_owned() }),
    () = sleep_until(deadline) => Err(HostError::TimedOut { adapter: adapter.to_owned() }),
    result = operation => result,
  }
}

/// Derives a bounded non-secret request identity for a cancellation frame.
#[must_use]
pub fn cancel_request_id(request_id: &str) -> String {
  let digest = Sha256::digest(request_id.as_bytes());
  format!("host-cancel-{digest:x}")
}

async fn verify_current_executable(executable: &VerifiedExecutable) -> Result<(), HostError> {
  let changed = || HostError::ExecutableChanged {
    adapter: executable.adapter_id().to_owned(),
  };
  validate_regular_file(executable.path(), true).map_err(|_| changed())?;
  let canonical = tokio::fs::canonicalize(executable.path())
    .await
    .map_err(|_| changed())?;
  if canonical != executable.path() {
    return Err(changed());
  }
  let mut file = tokio::fs::File::open(executable.path()).await.map_err(|_| changed())?;
  let metadata = file.metadata().await.map_err(|_| changed())?;
  if metadata.len() > MAX_EXECUTABLE_BYTES {
    return Err(changed());
  }
  let mut digest = Sha256::new();
  let mut buffer = vec![0_u8; 64 * 1024];
  loop {
    let read = file.read(&mut buffer).await.map_err(|_| changed())?;
    if read == 0 {
      break;
    }
    digest.update(&buffer[..read]);
  }
  if format!("{:x}", digest.finalize()) != executable.sha256() {
    return Err(changed());
  }
  Ok(())
}

async fn write_frame<W: tokio::io::AsyncWrite + Unpin>(
  adapter: &str,
  writer: &mut W,
  frame: &[u8],
  limit: usize,
) -> Result<(), HostError> {
  if frame.len() > limit {
    return Err(protocol(adapter, "outgoing request exceeds the message limit"));
  }
  writer.write_all(frame).await.map_err(|source| HostError::Io {
    adapter: adapter.to_owned(),
    source,
  })?;
  writer.write_all(b"\n").await.map_err(|source| HostError::Io {
    adapter: adapter.to_owned(),
    source,
  })
}

async fn read_frame<R: AsyncBufRead + Unpin>(
  adapter: &str,
  reader: &mut R,
  limit: usize,
) -> Result<Option<Vec<u8>>, HostError> {
  let mut frame = Vec::new();
  loop {
    let available = reader.fill_buf().await.map_err(|source| HostError::Io {
      adapter: adapter.to_owned(),
      source,
    })?;
    if available.is_empty() {
      return if frame.is_empty() {
        Ok(None)
      } else {
        Err(protocol(adapter, "response frame is not newline terminated"))
      };
    }
    let newline = available.iter().position(|byte| *byte == b'\n');
    let consumed = newline.map_or(available.len(), |index| index + 1);
    let content = newline.map_or(available, |index| &available[..index]);
    if frame.len().saturating_add(content.len()) > limit {
      return Err(protocol(adapter, "response exceeds the message limit"));
    }
    frame.extend_from_slice(content);
    reader.consume(consumed);
    if newline.is_some() {
      return Ok(Some(frame));
    }
  }
}

async fn stop_operation(
  adapter: &str,
  request: &ProcessRequest<'_>,
  child: &mut ChildGuard,
  mut stdin: tokio::process::ChildStdin,
  stdout: BufReader<tokio::process::ChildStdout>,
  stderr_task: &mut Option<tokio::task::JoinHandle<Result<BoundedOutput, std::io::Error>>>,
  grace: Duration,
) {
  warn!(adapter, request_id = request.request_id, "stopping adapter operation");
  let deadline = Instant::now() + grace;
  if let Some(cancel) = &request.cancellation_frame {
    let _ = timeout_at(
      deadline,
      write_frame(adapter, &mut stdin, cancel, request.max_message_bytes),
    )
    .await;
  }
  drop(stdin);
  let stdout_task = tokio::spawn(read_bounded(stdout, request.max_message_bytes));
  let graceful = timeout_at(deadline, async {
    let _ = child.wait().await;
    let _ = stdout_task.await;
    if let Some(task) = stderr_task.take() {
      let _ = task.await;
    }
  })
  .await;
  if graceful.is_err() {
    child.force_kill_and_wait().await;
  }
}

async fn finish_process(
  adapter: &str,
  child: &mut ChildGuard,
  stdout_task: tokio::task::JoinHandle<Result<BoundedOutput, std::io::Error>>,
  stderr_task: &mut Option<tokio::task::JoinHandle<Result<BoundedOutput, std::io::Error>>>,
  deadline: Instant,
  cleanup_grace: Duration,
) -> Result<(), HostError> {
  let completion = timeout_at(deadline, async {
    let status = child.wait().await;
    let stdout = stdout_task.await;
    let stderr = match stderr_task.take() {
      Some(task) => task.await,
      None => return Err(protocol(adapter, "stderr was already collected")),
    };
    Ok::<_, HostError>((status, stdout, stderr))
  })
  .await;
  let (status, stdout, stderr) = match completion {
    Ok(result) => result?,
    Err(_) => {
      abort_process(child, stderr_task, cleanup_grace).await;
      return Err(HostError::TimedOut {
        adapter: adapter.to_owned(),
      });
    }
  };
  let status = status.map_err(|source| HostError::Io {
    adapter: adapter.to_owned(),
    source,
  })?;
  let stdout = stdout
    .map_err(|_| protocol(adapter, "stdout reader task failed"))?
    .map_err(|source| HostError::Io {
      adapter: adapter.to_owned(),
      source,
    })?;
  let stderr = stderr
    .map_err(|_| protocol(adapter, "stderr reader task failed"))?
    .map_err(|source| HostError::Io {
      adapter: adapter.to_owned(),
      source,
    })?;
  if stdout.truncated || !stdout.bytes.is_empty() {
    return Err(protocol(adapter, "adapter emitted extra or oversized stdout"));
  }
  if stderr.truncated {
    return Err(protocol(adapter, "adapter stderr exceeded its bounded limit"));
  }
  if !status.success() {
    return Err(HostError::Crashed {
      adapter: adapter.to_owned(),
    });
  }
  Ok(())
}

async fn abort_process(
  child: &mut ChildGuard,
  stderr_task: &mut Option<tokio::task::JoinHandle<Result<BoundedOutput, std::io::Error>>>,
  grace: Duration,
) {
  child.force_kill_and_wait().await;
  if let Some(task) = stderr_task.take() {
    let _ = timeout(grace, task).await;
  }
}

fn protocol(adapter: &str, message: impl Into<String>) -> HostError {
  HostError::ProtocolFault {
    adapter: adapter.to_owned(),
    message: message.into(),
  }
}

struct ChildGuard {
  child: Child,
  process_group: ProcessGroup,
  active: bool,
}

impl ChildGuard {
  fn new(child: Child, process_group: ProcessGroup) -> Self {
    Self {
      child,
      process_group,
      active: true,
    }
  }

  async fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
    let status = self.child.wait().await?;
    self.active = false;
    Ok(status)
  }

  fn force_kill(&mut self) {
    let _ = self.process_group.kill_all();
    if self.active {
      let _ = self.child.start_kill();
    }
  }

  async fn force_kill_and_wait(&mut self) {
    self.force_kill();
    if self.active {
      let _ = self.wait().await;
    }
  }
}

impl Drop for ChildGuard {
  fn drop(&mut self) {
    self.force_kill();
  }
}

struct BoundedOutput {
  bytes: Vec<u8>,
  truncated: bool,
}

async fn read_bounded<R: AsyncRead + Unpin>(mut reader: R, limit: usize) -> Result<BoundedOutput, std::io::Error> {
  let mut bytes = Vec::with_capacity(limit.min(8192));
  let mut buffer = [0_u8; 8192];
  let mut truncated = false;
  loop {
    let read = reader.read(&mut buffer).await?;
    if read == 0 {
      break;
    }
    let remaining = limit.saturating_sub(bytes.len());
    bytes.extend_from_slice(&buffer[..read.min(remaining)]);
    truncated |= read > remaining;
  }
  Ok(BoundedOutput { bytes, truncated })
}

#[cfg(test)]
mod tests {
  use tokio::io::BufReader;

  use super::*;

  #[tokio::test]
  async fn rejects_oversized_incoming_frames_before_allocating_past_the_limit() {
    let (mut writer, reader) = tokio::io::duplex(128);
    writer.write_all(&[b'x'; 65]).await.unwrap();
    writer.write_all(b"\n").await.unwrap();
    drop(writer);

    let error = read_frame("fixture", &mut BufReader::new(reader), 64)
      .await
      .unwrap_err();
    assert!(matches!(error, HostError::ProtocolFault { .. }));
  }

  #[tokio::test]
  async fn rejects_unterminated_incoming_frames() {
    let (mut writer, reader) = tokio::io::duplex(128);
    writer.write_all(b"not terminated").await.unwrap();
    drop(writer);

    let error = read_frame("fixture", &mut BufReader::new(reader), 64)
      .await
      .unwrap_err();
    assert!(matches!(error, HostError::ProtocolFault { .. }));
  }
}
