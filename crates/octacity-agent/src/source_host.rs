use std::{collections::BTreeMap, future::pending, path::PathBuf, process::Stdio, time::Duration};

use octacity_source_plugin::{
  MAX_SOURCE_FRAME_BYTES, MaterializeRequest, SOURCE_PLUGIN_PROTOCOL_VERSION, SourceCommand, SourceMessage, read_frame,
  validate_request_id,
};
use thiserror::Error;
use tokio::{
  io::{AsyncRead, AsyncReadExt as _, AsyncWriteExt as _, BufReader},
  process::{Child, Command},
  time::{Instant, sleep_until, timeout},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::source::InstalledSourcePlugin;

const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_PLUGIN_STDERR_BYTES: usize = 64 * 1024;
const MAX_LIFECYCLE_MESSAGES: usize = 4096;
const MAX_LIFECYCLE_BYTES: usize = MAX_SOURCE_FRAME_BYTES;

#[derive(Debug)]
pub struct SourceMaterializationRequest {
  pub request_id: String,
  pub destination: PathBuf,
  pub revision: String,
  pub reference: Option<String>,
  pub parameters: BTreeMap<String, serde_json::Value>,
  pub credential_files: BTreeMap<String, PathBuf>,
  pub max_workspace_bytes: u64,
}

#[derive(Debug, Eq, PartialEq)]
pub struct MaterializedSource {
  pub revision: String,
  pub provenance: BTreeMap<String, String>,
  pub progress: Vec<String>,
  pub diagnostics: Vec<String>,
}

#[derive(Debug, Error)]
pub enum SourceHostError {
  #[error("invalid source-plugin request: {0}")]
  Invalid(String),
  #[error("failed to start source plugin '{plugin}': {source}")]
  Spawn { plugin: String, source: std::io::Error },
  #[error("source plugin '{plugin}' I/O failed: {source}")]
  Io { plugin: String, source: std::io::Error },
  #[error("source plugin '{plugin}' sent invalid JSON: {source}")]
  Json {
    plugin: String,
    source: Box<serde_json::Error>,
  },
  #[error("source plugin '{plugin}' violated its protocol: {message}")]
  Protocol { plugin: String, message: String },
  #[error("source plugin '{plugin}' failed: {message}")]
  Plugin { plugin: String, message: String },
  #[error("source plugin '{plugin}' was cancelled")]
  Cancelled { plugin: String },
  #[error("source plugin '{plugin}' exceeded its execution timeout")]
  TimedOut { plugin: String },
}

impl InstalledSourcePlugin {
  pub async fn materialize(
    &self,
    request: SourceMaterializationRequest,
    operation_timeout: Duration,
    cancellation_grace: Duration,
    cancellation: CancellationToken,
  ) -> Result<MaterializedSource, SourceHostError> {
    let plugin = self.manifest.name.clone();
    if operation_timeout.is_zero() || cancellation_grace.is_zero() {
      return Err(SourceHostError::Invalid(
        "operation and cancellation timeouts must be greater than zero".to_owned(),
      ));
    }
    validate_request_id(&request.request_id).map_err(SourceHostError::Invalid)?;
    let wire_request = self.wire_request(&request)?;
    wire_request.validate().map_err(SourceHostError::Invalid)?;
    info!(plugin = %plugin, request_id = %request.request_id, "starting source materialization");

    let mut command = Command::new(&self.executable);
    command
      .env_clear()
      .current_dir(
        self
          .executable
          .parent()
          .ok_or_else(|| SourceHostError::Invalid("source-plugin executable has no parent".to_owned()))?,
      )
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let child = command.spawn().map_err(|source| SourceHostError::Spawn {
      plugin: plugin.clone(),
      source,
    })?;
    let mut child = ChildGuard::new(child);
    let mut stdin = child
      .child
      .stdin
      .take()
      .ok_or_else(|| protocol(&plugin, "stdin was not piped"))?;
    let stdout = child
      .child
      .stdout
      .take()
      .ok_or_else(|| protocol(&plugin, "stdout was not piped"))?;
    let stderr = child
      .child
      .stderr
      .take()
      .ok_or_else(|| protocol(&plugin, "stderr was not piped"))?;
    let mut stdout = BufReader::new(stdout);
    let stderr_task = tokio::spawn(read_bounded(stderr, MAX_PLUGIN_STDERR_BYTES));

    let hello = timeout(HELLO_TIMEOUT, read_message(&plugin, &mut stdout))
      .await
      .map_err(|_| protocol(&plugin, "hello timed out"))??
      .ok_or_else(|| protocol(&plugin, "stdout closed before hello"))?;
    match hello {
      SourceMessage::Hello {
        protocol_version,
        plugin_name,
        plugin_version,
      } if protocol_version == SOURCE_PLUGIN_PROTOCOL_VERSION
        && plugin_name == self.manifest.name
        && plugin_version == self.manifest.version => {}
      _ => return Err(protocol(&plugin, "hello does not match the verified manifest")),
    }
    debug!(plugin = %plugin, request_id = %request.request_id, "validated source-plugin hello");

    write_command(
      &plugin,
      &mut stdin,
      &SourceCommand::Materialize {
        protocol_version: SOURCE_PLUGIN_PROTOCOL_VERSION,
        request_id: request.request_id.clone(),
        request: wire_request,
      },
    )
    .await?;

    let operation_deadline = sleep_until(Instant::now() + operation_timeout);
    tokio::pin!(operation_deadline);
    let mut cancellation_deadline = None;
    let mut stop_reason = None;
    let mut accepted = false;
    let mut progress = Vec::new();
    let mut diagnostics = Vec::new();
    let mut lifecycle_messages = 0_usize;
    let mut lifecycle_bytes = 0_usize;

    loop {
      tokio::select! {
        message = read_message(&plugin, &mut stdout) => {
          let message = match message {
            Ok(Some(message)) => message,
            Ok(None) if stop_reason.is_some() => {
              let reason = stop_reason.expect("guarded by is_some");
              child.force_kill_and_wait().await;
              return Err(stop_error(&plugin, reason));
            }
            Ok(None) => return Err(protocol(&plugin, "stdout closed before a terminal message")),
            Err(_) if stop_reason.is_some() => {
              let reason = stop_reason.expect("guarded by is_some");
              child.force_kill_and_wait().await;
              return Err(stop_error(&plugin, reason));
            }
            Err(error) => return Err(error),
          };
          match message {
            SourceMessage::Accepted { request_id } if request_id == request.request_id && !accepted => {
              accepted = true;
              debug!(plugin = %plugin, request_id = %request.request_id, "source plugin accepted request");
            },
            SourceMessage::Progress { request_id, message } if request_id == request.request_id && accepted => {
              record_lifecycle_message(&plugin, &message, &mut lifecycle_messages, &mut lifecycle_bytes)?;
              progress.push(message);
            }
            SourceMessage::Diagnostic { request_id, message } if request_id == request.request_id && accepted => {
              record_lifecycle_message(&plugin, &message, &mut lifecycle_messages, &mut lifecycle_bytes)?;
              diagnostics.push(message);
            }
            SourceMessage::Finished { request_id, revision, provenance }
              if request_id == request.request_id && accepted => {
                if let Some(reason) = stop_reason {
                  child.force_kill_and_wait().await;
                  return Err(stop_error(&plugin, reason));
                }
                if revision != request.revision {
                  return Err(protocol(&plugin, "terminal revision differs from the requested revision"));
                }
                finish_process(&plugin, &mut child, stderr_task, cancellation_grace).await?;
                info!(plugin = %plugin, request_id = %request.request_id, revision = %revision, "source materialization finished");
                return Ok(MaterializedSource { revision, provenance, progress, diagnostics });
              }
            SourceMessage::Cancelled { request_id } if request_id == request.request_id && accepted => {
              if let Some(reason) = stop_reason {
                child.force_kill_and_wait().await;
                return Err(stop_error(&plugin, reason));
              }
              finish_process(&plugin, &mut child, stderr_task, cancellation_grace).await?;
              return Err(stop_error(&plugin, StopReason::Cancelled));
            }
            SourceMessage::Error { request_id, message }
              if request_id.as_deref().is_none_or(|id| id == request.request_id) => {
                if let Some(reason) = stop_reason {
                  child.force_kill_and_wait().await;
                  return Err(stop_error(&plugin, reason));
                }
                finish_process(&plugin, &mut child, stderr_task, cancellation_grace).await?;
                return Err(SourceHostError::Plugin { plugin, message });
              }
            _ => return Err(protocol(&plugin, "unexpected, duplicate, or incorrectly correlated message")),
          }
        }
        () = cancellation.cancelled(), if stop_reason.is_none() => {
          warn!(plugin = %plugin, request_id = %request.request_id, "cancelling source materialization");
          stop_reason = Some(StopReason::Cancelled);
          let _ = send_cancel(&plugin, &mut stdin, &request.request_id).await;
          cancellation_deadline = Some(Instant::now() + cancellation_grace);
        }
        () = &mut operation_deadline, if stop_reason.is_none() => {
          warn!(plugin = %plugin, request_id = %request.request_id, "source materialization timed out");
          stop_reason = Some(StopReason::TimedOut);
          let _ = send_cancel(&plugin, &mut stdin, &request.request_id).await;
          cancellation_deadline = Some(Instant::now() + cancellation_grace);
        }
        () = wait_for_deadline(cancellation_deadline), if stop_reason.is_some() => {
          if let Some(reason) = stop_reason {
            child.force_kill_and_wait().await;
            return Err(stop_error(&plugin, reason));
          }
        }
      }
    }
  }

  fn wire_request(&self, request: &SourceMaterializationRequest) -> Result<MaterializeRequest, SourceHostError> {
    let destination = utf8_path("destination", &request.destination)?;
    let credential_files = request
      .credential_files
      .iter()
      .map(|(name, path)| Ok((name.clone(), utf8_path("credential file", path)?)))
      .collect::<Result<_, SourceHostError>>()?;
    Ok(MaterializeRequest {
      destination,
      revision: request.revision.clone(),
      reference: request.reference.clone(),
      parameters: request.parameters.clone(),
      settings: self.manifest.settings.clone(),
      credential_files,
      max_workspace_bytes: request.max_workspace_bytes,
    })
  }
}

#[derive(Clone, Copy)]
enum StopReason {
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

async fn write_command<W: tokio::io::AsyncWrite + Unpin>(
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
  stdin.write_all(&frame).await.map_err(|source| SourceHostError::Io {
    plugin: plugin.to_owned(),
    source,
  })?;
  stdin.flush().await.map_err(|source| SourceHostError::Io {
    plugin: plugin.to_owned(),
    source,
  })
}

async fn read_message<R: AsyncRead + Unpin>(
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

async fn finish_process(
  plugin: &str,
  child: &mut ChildGuard,
  stderr_task: tokio::task::JoinHandle<Result<BoundedOutput, std::io::Error>>,
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
  let stderr = stderr_task
    .await
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

struct ChildGuard {
  child: Child,
  active: bool,
}

impl ChildGuard {
  fn new(child: Child) -> Self {
    Self { child, active: true }
  }

  async fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
    let status = self.child.wait().await?;
    self.active = false;
    Ok(status)
  }

  fn force_kill(&mut self) {
    if self.active {
      kill_process_group(&mut self.child);
    }
  }

  async fn force_kill_and_wait(&mut self) {
    self.force_kill();
    let _ = self.wait().await;
  }
}

impl Drop for ChildGuard {
  fn drop(&mut self) {
    self.force_kill();
  }
}

fn kill_process_group(child: &mut Child) {
  #[cfg(unix)]
  if let Some(id) = child.id().and_then(|id| i32::try_from(id).ok()) {
    // SAFETY: the plugin is started as leader of a new process group.
    unsafe {
      libc::kill(-id, libc::SIGKILL);
    }
  }
  let _ = child.start_kill();
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

async fn wait_for_deadline(deadline: Option<Instant>) {
  match deadline {
    Some(deadline) => sleep_until(deadline).await,
    None => pending().await,
  }
}

fn utf8_path(name: &str, path: &std::path::Path) -> Result<String, SourceHostError> {
  path
    .to_str()
    .map(str::to_owned)
    .ok_or_else(|| SourceHostError::Invalid(format!("{name} must be valid UTF-8")))
}

fn stop_error(plugin: &str, reason: StopReason) -> SourceHostError {
  match reason {
    StopReason::Cancelled => SourceHostError::Cancelled {
      plugin: plugin.to_owned(),
    },
    StopReason::TimedOut => SourceHostError::TimedOut {
      plugin: plugin.to_owned(),
    },
  }
}

fn protocol(plugin: &str, message: impl Into<String>) -> SourceHostError {
  SourceHostError::Protocol {
    plugin: plugin.to_owned(),
    message: message.into(),
  }
}

fn record_lifecycle_message(
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

fn sanitize(bytes: &[u8]) -> String {
  let mut value: String = String::from_utf8_lossy(bytes)
    .chars()
    .filter(|character| *character == '\n' || *character == '\t' || !character.is_control())
    .take(MAX_SOURCE_FRAME_BYTES)
    .collect();
  value.truncate(value.trim_end().len());
  value
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn reads_one_bounded_protocol_message() {
    let input = br#"{"type":"accepted","request_id":"request-1"}
"#;
    let mut reader = BufReader::new(&input[..]);
    assert_eq!(
      read_message("fixture", &mut reader).await.unwrap(),
      Some(SourceMessage::Accepted {
        request_id: "request-1".to_owned()
      })
    );
    assert_eq!(read_message("fixture", &mut reader).await.unwrap(), None);
  }

  #[tokio::test]
  async fn rejects_malformed_protocol_messages() {
    let input = b"not-json\n";
    let mut reader = BufReader::new(&input[..]);
    assert!(matches!(
      read_message("fixture", &mut reader).await,
      Err(SourceHostError::Json { .. })
    ));
  }

  #[tokio::test]
  async fn distinguishes_eof_and_unterminated_protocol_messages() {
    let mut empty = BufReader::new(&b""[..]);
    assert_eq!(read_message("fixture", &mut empty).await.unwrap(), None);

    let mut unterminated = BufReader::new(&br#"{"type":"accepted","request_id":"request-1"}"#[..]);
    assert!(matches!(
      read_message("fixture", &mut unterminated).await,
      Err(SourceHostError::Protocol { .. })
    ));
  }

  #[tokio::test]
  async fn serializes_a_complete_command_frame() {
    let (mut writer, mut reader) = tokio::io::duplex(1024);
    write_command(
      "fixture",
      &mut writer,
      &SourceCommand::Cancel {
        request_id: "request-1".to_owned(),
      },
    )
    .await
    .unwrap();
    drop(writer);
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).await.unwrap();
    assert_eq!(
      serde_json::from_slice::<SourceCommand>(&bytes).unwrap(),
      SourceCommand::Cancel {
        request_id: "request-1".to_owned()
      }
    );
  }

  #[tokio::test]
  async fn rejects_an_oversized_outgoing_command() {
    let mut output = Vec::new();
    let error = write_command(
      "fixture",
      &mut output,
      &SourceCommand::Cancel {
        request_id: "x".repeat(MAX_SOURCE_FRAME_BYTES),
      },
    )
    .await
    .unwrap_err();

    assert!(error.to_string().contains("outgoing command exceeds"));
    assert!(output.is_empty());
  }

  #[tokio::test]
  async fn bounds_diagnostics_while_draining_the_reader() {
    let output = read_bounded(&b"abcdef"[..], 3).await.unwrap();
    assert_eq!(output.bytes, b"abc");
    assert!(output.truncated);
    assert_eq!(sanitize(b"error\0 text\n"), "error text");
  }

  #[test]
  fn preserves_stop_reason_in_errors() {
    assert!(matches!(
      stop_error("fixture", StopReason::Cancelled),
      SourceHostError::Cancelled { .. }
    ));
    assert!(matches!(
      stop_error("fixture", StopReason::TimedOut),
      SourceHostError::TimedOut { .. }
    ));
  }

  #[test]
  fn bounds_accumulated_lifecycle_messages() {
    let mut count = 0;
    let mut bytes = 0;
    record_lifecycle_message("fixture", "progress", &mut count, &mut bytes).unwrap();
    assert_eq!((count, bytes), (1, 8));

    let mut count = MAX_LIFECYCLE_MESSAGES;
    let mut bytes = 0;
    assert!(record_lifecycle_message("fixture", "one more", &mut count, &mut bytes).is_err());

    let mut count = 0;
    let mut bytes = MAX_LIFECYCLE_BYTES;
    assert!(record_lifecycle_message("fixture", "x", &mut count, &mut bytes).is_err());
  }
}
