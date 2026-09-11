//! Owns bounded source-plugin framing and descendant process cleanup.

use std::{future::pending, time::Duration};

use octacity_source_plugin::{MAX_SOURCE_FRAME_BYTES, SourceCommand, SourceMessage, read_frame};
use processkit::ProcessGroup;
use tokio::{
  io::{AsyncRead, AsyncReadExt as _, AsyncWriteExt as _, BufReader},
  process::Child,
  time::{Instant, sleep_until, timeout, timeout_at},
};
use tracing::debug;

use super::{MAX_LIFECYCLE_BYTES, MAX_LIFECYCLE_MESSAGES, SourceHostError};

/// Reason retained while the host drains a plugin's terminal response.
#[derive(Clone, Copy)]
pub(super) enum StopReason {
  Cancelled,
  TimedOut,
}

async fn send_cancel(
  plugin: &str,
  stdin: &mut tokio::process::ChildStdin,
  request_id: &str,
) -> Result<(), SourceHostError> {
  write_command(
    plugin,
    stdin,
    &SourceCommand::Cancel {
      request_id: request_id.to_owned(),
    },
  )
  .await
}

/// Attempts cooperative cancellation without extending the forced-stop deadline.
pub(super) async fn send_cancel_before(
  plugin: &str,
  stdin: &mut tokio::process::ChildStdin,
  request_id: &str,
  deadline: Instant,
) {
  if !matches!(
    timeout_at(deadline, send_cancel(plugin, stdin, request_id)).await,
    Ok(Ok(()))
  ) {
    debug!(
      plugin,
      request_id, "could not deliver source cancellation before forced-stop deadline"
    );
  }
}

/// Serializes one bounded JSONL command onto the plugin control stream.
pub(super) async fn write_command<W: tokio::io::AsyncWrite + Unpin>(
  plugin: &str,
  stdin: &mut W,
  command: &SourceCommand,
) -> Result<(), SourceHostError> {
  let mut frame = serde_json::to_vec(command).map_err(|source| SourceHostError::Json {
    plugin: plugin.to_owned(),
    source: Box::new(source),
  })?;
  frame.push(b'\n');
  if frame.len() > MAX_SOURCE_FRAME_BYTES {
    return Err(protocol(plugin, "outgoing command exceeds the bounded frame limit"));
  }
  // Child stdin writes directly to an OS pipe. Flushing it separately is
  // unnecessary and can race with a plugin that consumes the frame and exits.
  stdin.write_all(&frame).await.map_err(|source| SourceHostError::Io {
    plugin: plugin.to_owned(),
    source,
  })
}

/// Reads and decodes one bounded JSONL message from a plugin.
pub(super) async fn read_message<R: AsyncRead + Unpin>(
  plugin: &str,
  reader: &mut BufReader<R>,
) -> Result<Option<SourceMessage>, SourceHostError> {
  let mut frame = String::new();
  let read = read_frame(reader, &mut frame)
    .await
    .map_err(|error| SourceHostError::Protocol {
      plugin: plugin.to_owned(),
      message: error.to_string(),
    })?;
  if read == 0 {
    return Ok(None);
  }
  let message = serde_json::from_str(&frame).map_err(|source| SourceHostError::Json {
    plugin: plugin.to_owned(),
    source: Box::new(source),
  })?;
  Ok(Some(message))
}

/// Reaps a terminal plugin and proves that inherited diagnostic pipes closed.
pub(super) async fn finish_process(
  plugin: &str,
  child: &mut ChildGuard,
  stderr_task: &mut Option<tokio::task::JoinHandle<Result<BoundedOutput, std::io::Error>>>,
  grace: Duration,
) -> Result<(), SourceHostError> {
  let status = match timeout(grace, child.wait()).await {
    Ok(status) => status.map_err(|source| SourceHostError::Io {
      plugin: plugin.to_owned(),
      source,
    })?,
    Err(_) => {
      child.force_kill_and_wait().await;
      return Err(protocol(plugin, "did not exit after its terminal message"));
    }
  };
  // A plugin is not allowed to leave background descendants behind. Kill the
  // complete group even after its protocol process exited so inherited pipe
  // descriptors cannot keep stderr open forever.
  child.force_kill();
  let stderr = timeout(
    grace,
    stderr_task
      .take()
      .ok_or_else(|| protocol(plugin, "stderr was already collected"))?,
  )
  .await
  .map_err(|_| protocol(plugin, "stderr did not close after the plugin exited"))?
  .map_err(|error| SourceHostError::Io {
    plugin: plugin.to_owned(),
    source: std::io::Error::other(error),
  })?
  .map_err(|source| SourceHostError::Io {
    plugin: plugin.to_owned(),
    source,
  })?;
  if stderr.truncated {
    return Err(protocol(plugin, "stderr exceeded its bounded diagnostic limit"));
  }
  if !status.success() {
    return Err(protocol(
      plugin,
      format!("exited with {status}: {}", sanitize(&stderr.bytes)),
    ));
  }
  Ok(())
}

/// Ensures the plugin process tree is terminated even on early returns.
pub(super) struct ChildGuard {
  pub(super) child: Child,
  process_group: ProcessGroup,
  active: bool,
}

impl ChildGuard {
  pub(super) fn new(child: Child, process_group: ProcessGroup) -> Self {
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
      // Preserve a direct-child fallback when an OS reports that complete
      // tree termination could not be confirmed.
      let _ = self.child.start_kill();
    }
  }

  pub(super) async fn force_kill_and_wait(&mut self) {
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

/// Bounded diagnostic data plus an indication that bytes were discarded.
pub(super) struct BoundedOutput {
  pub(super) bytes: Vec<u8>,
  pub(super) truncated: bool,
}

/// Drains a stream fully while retaining no more than `limit` bytes.
pub(super) async fn read_bounded<R: AsyncRead + Unpin>(
  mut reader: R,
  limit: usize,
) -> Result<BoundedOutput, std::io::Error> {
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

/// Waits for an optional stop deadline without polling.
pub(super) async fn wait_for_deadline(deadline: Option<Instant>) {
  match deadline {
    Some(deadline) => sleep_until(deadline).await,
    None => pending().await,
  }
}

/// Converts only paths representable by the language-neutral wire protocol.
pub(super) fn utf8_path(name: &str, path: &std::path::Path) -> Result<String, SourceHostError> {
  path
    .to_str()
    .map(str::to_owned)
    .ok_or_else(|| SourceHostError::Invalid(format!("{name} must be valid UTF-8")))
}

/// Maps the retained stop reason to its stable host error.
pub(super) fn stop_error(plugin: &str, reason: StopReason) -> SourceHostError {
  match reason {
    StopReason::Cancelled => SourceHostError::Cancelled {
      plugin: plugin.to_owned(),
    },
    StopReason::TimedOut => SourceHostError::TimedOut {
      plugin: plugin.to_owned(),
    },
  }
}

pub(super) fn protocol(plugin: &str, message: impl Into<String>) -> SourceHostError {
  SourceHostError::Protocol {
    plugin: plugin.to_owned(),
    message: message.into(),
  }
}

/// Accounts for untrusted progress and diagnostics across the complete request.
pub(super) fn record_lifecycle_message(
  plugin: &str,
  message: &str,
  count: &mut usize,
  bytes: &mut usize,
) -> Result<(), SourceHostError> {
  *count = count.saturating_add(1);
  *bytes = bytes.saturating_add(message.len());
  if *count > MAX_LIFECYCLE_MESSAGES || *bytes > MAX_LIFECYCLE_BYTES {
    return Err(protocol(plugin, "lifecycle messages exceed their bounded limit"));
  }
  Ok(())
}

/// Removes control characters and bounds diagnostics before logging them.
pub(super) fn sanitize(bytes: &[u8]) -> String {
  let mut value: String = String::from_utf8_lossy(bytes)
    .chars()
    .filter(|character| *character == '\n' || *character == '\t' || !character.is_control())
    .take(MAX_SOURCE_FRAME_BYTES)
    .collect();
  value.truncate(value.trim_end().len());
  value
}
