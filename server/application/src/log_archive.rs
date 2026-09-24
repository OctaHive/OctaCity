use std::fmt;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use octacity_artifact_store::LogChunkStoreError;
use octacity_protocol::{AttemptEventEnvelope, AttemptEventKind};
use octacity_server_domain::JobId;
use octacity_server_store::{BuildLogStream, LogChunkManifest, MAX_LOG_CHUNK_BYTES};
use zeroize::Zeroizing;

use crate::AgentExecutionError;

const MAX_PROTECTED_VALUES: usize = 256;
const MAX_PROTECTED_VALUE_BYTES: usize = 64 * 1024;
const MAX_PROTECTED_TOTAL_BYTES: usize = 1024 * 1024;

/// In-memory server-side protection applied before logs reach any persistence.
///
/// Values are never formatted and are zeroed when the redactor is dropped.
/// Output frames are redacted as logical stdout and stderr streams, so an
/// unrelated event cannot expose a value split across adjacent stream bytes.
/// The Agent applies the corresponding stateful protection before spooling;
/// this server-side pass is the final defense for one validated append batch.
#[derive(Clone, Default)]
pub struct LogRedactor {
  protected: Vec<Zeroizing<Vec<u8>>>,
}

impl LogRedactor {
  /// Builds a bounded set of exact byte values that must be masked.
  pub fn new(values: impl IntoIterator<Item = Vec<u8>>) -> Result<Self, AgentExecutionError> {
    let mut protected = Vec::new();
    let mut total = 0_usize;
    for value in values {
      if value.is_empty() || value.len() > MAX_PROTECTED_VALUE_BYTES || protected.len() >= MAX_PROTECTED_VALUES {
        return Err(AgentExecutionError::InvalidRequest);
      }
      total = total
        .checked_add(value.len())
        .ok_or(AgentExecutionError::InvalidRequest)?;
      if total > MAX_PROTECTED_TOTAL_BYTES {
        return Err(AgentExecutionError::InvalidRequest);
      }
      if !protected
        .iter()
        .any(|candidate: &Zeroizing<Vec<u8>>| candidate.as_slice() == value)
      {
        protected.push(Zeroizing::new(value));
      }
    }
    protected.sort_by_key(|value| std::cmp::Reverse(value.len()));
    Ok(Self { protected })
  }

  fn redact(&self, bytes: &mut [u8]) {
    for protected in &self.protected {
      mask_occurrences(bytes, protected);
    }
    mask_url_queries(bytes);
  }
}

impl fmt::Debug for LogRedactor {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("LogRedactor")
      .field("protected_value_count", &self.protected.len())
      .finish()
  }
}

pub(crate) struct PreparedLogArchive {
  pub(crate) events: Vec<AttemptEventEnvelope>,
  pub(crate) chunks: Vec<(LogChunkManifest, Vec<u8>)>,
}

pub(crate) fn prepare_log_archive(
  mut events: Vec<AttemptEventEnvelope>,
  durable_through: u64,
  job_id: JobId,
  redactor: &LogRedactor,
) -> Result<PreparedLogArchive, AgentExecutionError> {
  let mut frames = Vec::new();
  for (index, envelope) in events.iter().enumerate() {
    if let Some(frame) = output_frame(index, envelope)? {
      frames.push(frame);
    }
  }

  for stream in [BuildLogStream::Stdout, BuildLogStream::Stderr] {
    let stream_frames = frames.iter().filter(|frame| frame.stream == stream).collect::<Vec<_>>();
    redact_stream_frames(&mut events, &stream_frames, redactor)?;
  }

  let mut chunks = Vec::new();
  let mut pending: Vec<&OutputFrame> = Vec::new();
  let mut pending_bytes = 0_usize;
  for frame in frames.iter().filter(|frame| frame.sequence > durable_through) {
    if frame.bytes.is_empty() {
      continue;
    }
    let continues = pending.last().is_some_and(|previous| {
      previous.sequence.checked_add(1) == Some(frame.sequence) && previous.stream == frame.stream
    });
    if !continues || pending_bytes.saturating_add(frame.bytes.len()) > MAX_LOG_CHUNK_BYTES {
      finish_chunk(&events, job_id, &mut pending, &mut pending_bytes, &mut chunks)?;
    }
    if frame.bytes.len() > MAX_LOG_CHUNK_BYTES {
      return Err(AgentExecutionError::InvalidRequest);
    }
    pending_bytes += frame.bytes.len();
    pending.push(frame);
  }
  finish_chunk(&events, job_id, &mut pending, &mut pending_bytes, &mut chunks)?;

  Ok(PreparedLogArchive { events, chunks })
}

pub(crate) fn durable_event_kind(event: &AttemptEventEnvelope) -> &'static str {
  match output_frame(0, event) {
    Ok(Some(frame)) if !frame.bytes.is_empty() => frame.stream.as_str(),
    _ => match event.kind {
      AttemptEventKind::Runner { .. } => "runner",
      AttemptEventKind::Agent { .. } => "agent",
    },
  }
}

#[derive(Clone)]
struct OutputFrame {
  event_index: usize,
  sequence: u64,
  stream: BuildLogStream,
  format: OutputFormat,
  bytes: Vec<u8>,
}

#[derive(Clone, Copy)]
enum OutputFormat {
  Bytes,
  RawBytes,
  Line,
}

fn output_frame(index: usize, envelope: &AttemptEventEnvelope) -> Result<Option<OutputFrame>, AgentExecutionError> {
  let AttemptEventKind::Runner { event } = &envelope.kind else {
    return Ok(None);
  };
  if event.data.get("type").and_then(serde_json::Value::as_str) != Some("output") {
    return Ok(None);
  }
  let stream = match event.data.get("stream").and_then(serde_json::Value::as_str) {
    Some("stdout") => BuildLogStream::Stdout,
    Some("stderr") => BuildLogStream::Stderr,
    _ => return Err(AgentExecutionError::InvalidRequest),
  };
  let payload = event
    .data
    .get("payload")
    .and_then(serde_json::Value::as_object)
    .ok_or(AgentExecutionError::InvalidRequest)?;
  let format = match payload.get("format").and_then(serde_json::Value::as_str) {
    Some("bytes") => OutputFormat::Bytes,
    Some("raw_bytes") => OutputFormat::RawBytes,
    Some("line") => OutputFormat::Line,
    _ => return Err(AgentExecutionError::InvalidRequest),
  };
  let data = payload
    .get("data")
    .and_then(serde_json::Value::as_str)
    .ok_or(AgentExecutionError::InvalidRequest)?;
  let bytes = match format {
    OutputFormat::Bytes | OutputFormat::RawBytes => {
      STANDARD.decode(data).map_err(|_| AgentExecutionError::InvalidRequest)?
    }
    OutputFormat::Line => data.as_bytes().to_vec(),
  };
  Ok(Some(OutputFrame {
    event_index: index,
    sequence: envelope.stream_sequence,
    stream,
    format,
    bytes,
  }))
}

fn redact_stream_frames(
  events: &mut [AttemptEventEnvelope],
  frames: &[&OutputFrame],
  redactor: &LogRedactor,
) -> Result<(), AgentExecutionError> {
  let mut combined: Vec<u8> = frames.iter().flat_map(|frame| frame.bytes.iter().copied()).collect();
  redactor.redact(&mut combined);
  let mut offset = 0;
  for frame in frames {
    let end = offset + frame.bytes.len();
    set_output_bytes(&mut events[frame.event_index], frame.format, &combined[offset..end])?;
    offset = end;
  }
  Ok(())
}

fn set_output_bytes(
  envelope: &mut AttemptEventEnvelope,
  format: OutputFormat,
  bytes: &[u8],
) -> Result<(), AgentExecutionError> {
  let AttemptEventKind::Runner { event } = &mut envelope.kind else {
    return Err(AgentExecutionError::InvalidRequest);
  };
  let payload = event
    .data
    .get_mut("payload")
    .and_then(serde_json::Value::as_object_mut)
    .ok_or(AgentExecutionError::InvalidRequest)?;
  let data = match format {
    OutputFormat::Bytes | OutputFormat::RawBytes => STANDARD.encode(bytes),
    OutputFormat::Line => std::str::from_utf8(bytes)
      .map_err(|_| AgentExecutionError::InvalidRequest)?
      .to_owned(),
  };
  payload.insert("data".to_owned(), serde_json::Value::String(data));
  Ok(())
}

fn finish_chunk(
  events: &[AttemptEventEnvelope],
  job_id: JobId,
  pending: &mut Vec<&OutputFrame>,
  pending_bytes: &mut usize,
  chunks: &mut Vec<(LogChunkManifest, Vec<u8>)>,
) -> Result<(), AgentExecutionError> {
  let Some(first) = pending.first() else {
    return Ok(());
  };
  let last = pending.last().expect("a first frame implies a last frame");
  let mut bytes = Vec::with_capacity(*pending_bytes);
  for frame in pending.iter() {
    let redacted =
      output_frame(frame.event_index, &events[frame.event_index])?.ok_or(AgentExecutionError::InvalidRequest)?;
    bytes.extend_from_slice(&redacted.bytes);
  }
  let manifest = LogChunkManifest::prepare(job_id, first.stream, first.sequence, last.sequence, &bytes)
    .map_err(|_| AgentExecutionError::InvalidRequest)?;
  chunks.push((manifest, bytes));
  pending.clear();
  *pending_bytes = 0;
  Ok(())
}

fn mask_occurrences(bytes: &mut [u8], pattern: &[u8]) {
  let mut offset = 0;
  while let Some(found) = bytes[offset..]
    .windows(pattern.len())
    .position(|candidate| candidate == pattern)
  {
    let start = offset + found;
    bytes[start..start + pattern.len()].fill(b'*');
    offset = start + pattern.len();
  }
}

fn mask_url_queries(bytes: &mut [u8]) {
  for scheme in [b"https://".as_slice(), b"http://".as_slice()] {
    let mut offset = 0;
    while let Some(found) = bytes[offset..]
      .windows(scheme.len())
      .position(|candidate| candidate == scheme)
    {
      let start = offset + found + scheme.len();
      let end = bytes[start..]
        .iter()
        .position(|byte| byte.is_ascii_whitespace())
        .map_or(bytes.len(), |length| start + length);
      if let Some(query) = bytes[start..end].iter().position(|byte| *byte == b'?') {
        bytes[start + query + 1..end].fill(b'*');
      }
      offset = end;
    }
  }
}

pub(crate) fn map_log_store_error(_: LogChunkStoreError) -> AgentExecutionError {
  AgentExecutionError::Unavailable
}

#[cfg(test)]
mod tests {
  use super::*;
  use octacity_protocol::RunnerEventPayload;
  use uuid::Uuid;

  #[test]
  fn redacts_cross_frame_values_and_presigned_queries_before_chunking() {
    let redactor = LogRedactor::new([b"token-secret".to_vec()]).unwrap();
    let events = vec![
      event(1, b"before token-"),
      event(2, b"secret https://store/object?signature=private after"),
    ];
    let prepared = prepare_log_archive(events, 0, JobId::from_uuid(Uuid::from_u128(1)).unwrap(), &redactor).unwrap();
    assert_eq!(prepared.chunks.len(), 1);
    let bytes = &prepared.chunks[0].1;
    assert!(!bytes.windows(12).any(|candidate| candidate == b"token-secret"));
    assert!(!String::from_utf8_lossy(bytes).contains("signature=private"));
    prepared.chunks[0].0.verify(bytes).unwrap();
  }

  #[test]
  fn redacts_a_stream_across_interleaved_output() {
    let redactor = LogRedactor::new([b"token-secret".to_vec()]).unwrap();
    let events = vec![
      event_for(1, "stdout", b"before token-"),
      event_for(2, "stderr", b"unrelated diagnostic"),
      event_for(3, "stdout", b"secret after"),
    ];

    let prepared = prepare_log_archive(events, 0, JobId::from_uuid(Uuid::from_u128(1)).unwrap(), &redactor).unwrap();
    let stdout = prepared
      .events
      .iter()
      .enumerate()
      .filter_map(|(index, event)| output_frame(index, event).unwrap())
      .filter(|frame| frame.stream == BuildLogStream::Stdout)
      .flat_map(|frame| frame.bytes)
      .collect::<Vec<_>>();

    assert!(!stdout.windows(12).any(|candidate| candidate == b"token-secret"));
    assert_eq!(stdout, b"before ************ after");
  }

  fn event(sequence: u64, bytes: &[u8]) -> AttemptEventEnvelope {
    event_for(sequence, "stdout", bytes)
  }

  fn event_for(sequence: u64, stream: &str, bytes: &[u8]) -> AttemptEventEnvelope {
    AttemptEventEnvelope {
      job_id: Uuid::from_u128(1).to_string(),
      attempt: 1,
      lease_id: Uuid::from_u128(2).to_string(),
      fencing_token: "01".repeat(32),
      stream_sequence: sequence,
      occurred_at_unix_ms: sequence as i64,
      kind: AttemptEventKind::Runner {
        event: RunnerEventPayload {
          schema_version: 4,
          sequence,
          timestamp: "2026-09-24T00:00:00Z".to_owned(),
          category: "execution".to_owned(),
          data: serde_json::json!({
            "type": "output",
            "stream": stream,
            "payload": {"format": "bytes", "data": STANDARD.encode(bytes)}
          })
          .as_object()
          .unwrap()
          .clone(),
        },
      },
    }
  }
}
