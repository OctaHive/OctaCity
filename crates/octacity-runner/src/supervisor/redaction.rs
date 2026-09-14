//! Removes agent-owned credentials from untrusted runner output.
//!
//! Octa redacts secrets it resolves itself, but a workload can still print the
//! workload-identity file used to authenticate to Vault. The supervisor is the
//! last boundary before runner events and results enter the durable agent
//! spool, so redaction belongs here rather than in Vault, lifecycle, or storage
//! code. The filter covers JSON strings and the base64 byte payload used by
//! Octa console events without exposing the configured values through `Debug`.

use std::collections::{BTreeMap, VecDeque};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use zeroize::{Zeroize as _, Zeroizing};

use crate::{
  protocol::{MAX_RUNNER_OUTPUT_FRAME_BYTES, RunnerEvent},
  supervisor::RunnerSupervisionError,
};

const REDACTION_MARKER: &[u8] = b"[redacted]";
const MAX_REDACTION_STREAMS: usize = 64;
const MAX_REDACTION_PENDING_BYTES: usize = MAX_RUNNER_OUTPUT_FRAME_BYTES;

/// In-memory values that must not cross the runner-supervisor boundary.
///
/// Values are zeroed on drop. This filter prevents accidental verbatim
/// disclosure; it cannot prevent an intentionally malicious workload from
/// transforming a credential before printing it.
#[derive(Default)]
pub struct RunnerRedactions {
  values: Vec<Zeroizing<Vec<u8>>>,
  prefix_tables: Vec<Vec<usize>>,
  streams: BTreeMap<StreamKey, RedactionStream>,
  pending: VecDeque<PendingEvent>,
  pending_bytes: usize,
}

impl RunnerRedactions {
  /// Copies non-empty values into a short-lived redaction set.
  pub fn new<'a>(values: impl IntoIterator<Item = &'a [u8]>) -> Self {
    let mut copied = Vec::new();
    for value in values.into_iter().filter(|value| !value.is_empty()) {
      copied.push(Zeroizing::new(value.to_vec()));
      let trimmed = value.trim_ascii();
      if !trimmed.is_empty() && trimmed != value {
        // Vault authenticates with the trimmed JWT. Retaining both forms also
        // covers `cat`/line output, where the event payload omits the newline
        // present in an ordinary token file.
        copied.push(Zeroizing::new(trimmed.to_vec()));
      }
    }
    let mut values = copied;
    values.sort_by(|left, right| left.as_slice().cmp(right.as_slice()));
    values.dedup_by(|left, right| left.as_slice() == right.as_slice());
    // Replacing a contained prefix first could hide the longer credential
    // from subsequent passes while leaving its suffix visible.
    values.sort_by_key(|value| std::cmp::Reverse(value.len()));
    let prefix_tables = values.iter().map(|value| prefix_table(value)).collect();
    Self {
      values,
      prefix_tables,
      streams: BTreeMap::new(),
      pending: VecDeque::new(),
      pending_bytes: 0,
    }
  }

  /// Redacts one event and returns every event whose output is now resolved.
  ///
  /// Binary output is scanned as a stream rather than as independent chunks.
  /// A possible secret prefix remains attached to its original pending event
  /// until another chunk from that stream or its terminal boundary resolves
  /// it. Interleaved streams therefore cannot corrupt ordinary output. Both
  /// the stream count and shared pending queue are bounded. At capacity, only
  /// ambiguous secret-prefix tails are masked so normal backpressure can keep
  /// the ordered stream moving without changing or inventing sequence numbers.
  pub(super) fn events(&mut self, event: RunnerEvent) -> Result<Vec<RunnerEvent>, RunnerSupervisionError> {
    let mut emitted = self.release_resolved()?;
    let output = output_payload(&event);
    if output
      .as_ref()
      .is_some_and(|(key, _, _)| !self.streams.contains_key(key) && self.streams.len() == MAX_REDACTION_STREAMS)
    {
      emitted.extend(self.relieve_pressure()?);
    }
    let event_bytes = event_wire_bytes(&event)?;
    if self.total_bytes().saturating_add(event_bytes) > MAX_REDACTION_PENDING_BYTES {
      emitted.extend(self.relieve_pressure()?);
    }
    self.push_event(event, event_bytes)?;
    let index = self.pending.len() - 1;
    if let Some((key, format, bytes)) = output {
      match format {
        OutputFormat::Bytes | OutputFormat::RawBytes => {
          let (bytes, resolved) = {
            let stream = self
              .streams
              .entry(key.clone())
              .or_insert_with(|| RedactionStream::new(index, self.values.len()));
            stream.anchor = index;
            let bytes = stream.push(bytes, &self.values, &self.prefix_tables);
            (bytes, stream.pending.is_empty())
          };
          if resolved {
            self.streams.remove(&key);
          }
          self.set_output_data(index, bytes, format)?;
        }
        OutputFormat::Line => {
          // A line is a semantic stream boundary. Only convert it to a byte
          // event when a preceding byte chunk left a candidate secret prefix.
          if let Some(mut stream) = self.streams.remove(&key) {
            let mut bytes = stream.push(bytes, &self.values, &self.prefix_tables);
            bytes.extend(stream.finish());
            bytes.push(b'\n');
            self.set_output_data(index, bytes, OutputFormat::Bytes)?;
          }
        }
      }
    }
    self.redact_event(index)?;
    self.flush_for_boundary(index)?;
    if self.total_bytes() > MAX_REDACTION_PENDING_BYTES {
      emitted.extend(self.relieve_pressure()?);
    }
    if self.total_bytes() > MAX_REDACTION_PENDING_BYTES {
      return Err(redaction_limit("pending byte count"));
    }
    emitted.extend(self.release_resolved()?);
    Ok(emitted)
  }

  /// Flushes bounded stream tails when an otherwise valid runner reaches its
  /// terminal protocol message without an explicit run-finished event.
  pub(super) fn finish_events(&mut self) -> Result<Vec<RunnerEvent>, RunnerSupervisionError> {
    let keys = self.streams.keys().cloned().collect::<Vec<_>>();
    self.flush_keys(keys)?;
    self.release_resolved()
  }

  fn flush_for_boundary(&mut self, index: usize) -> Result<(), RunnerSupervisionError> {
    let event = &self.pending[index].event;
    let Some(run_id) = event.data.get("run_id").and_then(serde_json::Value::as_u64) else {
      return Ok(());
    };
    let kind = event.data.get("type").and_then(serde_json::Value::as_str);
    let step_id = match kind {
      Some("step_finished") => event
        .data
        .get("step")
        .and_then(serde_json::Value::as_object)
        .and_then(|step| step.get("id"))
        .and_then(serde_json::Value::as_u64),
      Some("run_finished") => None,
      _ => return Ok(()),
    };
    let keys = self
      .streams
      .keys()
      .filter(|key| key.run_id == run_id && step_id.is_none_or(|step_id| key.step_id == Some(step_id)))
      .cloned()
      .collect::<Vec<_>>();
    self.flush_keys(keys)
  }

  fn flush_keys(&mut self, keys: Vec<StreamKey>) -> Result<(), RunnerSupervisionError> {
    for key in keys {
      if let Some(mut stream) = self.streams.remove(&key) {
        let bytes = stream.finish();
        if !bytes.is_empty() {
          self.append_output_data(stream.anchor, bytes)?;
          self.redact_event(stream.anchor)?;
        }
      }
    }
    Ok(())
  }

  fn relieve_pressure(&mut self) -> Result<Vec<RunnerEvent>, RunnerSupervisionError> {
    let keys = self.streams.keys().cloned().collect::<Vec<_>>();
    for key in keys {
      if let Some(mut stream) = self.streams.remove(&key) {
        let bytes = stream.finish_masked(&self.values, &self.prefix_tables);
        if !bytes.is_empty() {
          self.append_output_data(stream.anchor, bytes)?;
          self.redact_event(stream.anchor)?;
        }
      }
    }
    self.release_resolved()
  }

  fn push_event(&mut self, event: RunnerEvent, bytes: usize) -> Result<(), RunnerSupervisionError> {
    self.pending_bytes = self
      .pending_bytes
      .checked_add(bytes)
      .ok_or_else(|| redaction_limit("pending byte count"))?;
    self.pending.push_back(PendingEvent { event, bytes });
    Ok(())
  }

  fn set_output_data(
    &mut self,
    index: usize,
    bytes: Vec<u8>,
    format: OutputFormat,
  ) -> Result<(), RunnerSupervisionError> {
    set_output_data(&mut self.pending[index].event, bytes, format);
    self.refresh_event_size(index)
  }

  fn append_output_data(&mut self, index: usize, suffix: Vec<u8>) -> Result<(), RunnerSupervisionError> {
    let (_, format, mut bytes) = output_payload(&self.pending[index].event)
      .ok_or_else(|| RunnerSupervisionError::Protocol("redaction anchor is not an output event".to_owned()))?;
    bytes.extend(suffix);
    self.set_output_data(index, bytes, format)
  }

  fn redact_event(&mut self, index: usize) -> Result<(), RunnerSupervisionError> {
    redact_map(&mut self.pending[index].event.data, &self.values);
    self.refresh_event_size(index)
  }

  fn refresh_event_size(&mut self, index: usize) -> Result<(), RunnerSupervisionError> {
    let bytes = event_wire_bytes(&self.pending[index].event)?;
    self.pending_bytes = self.pending_bytes - self.pending[index].bytes + bytes;
    self.pending[index].bytes = bytes;
    Ok(())
  }

  fn total_bytes(&self) -> usize {
    let stream_bytes = self
      .streams
      .values()
      .fold(0_usize, |total, stream| total.saturating_add(stream.allocated_bytes()));
    self.pending_bytes.saturating_add(stream_bytes)
  }

  fn release_resolved(&mut self) -> Result<Vec<RunnerEvent>, RunnerSupervisionError> {
    let count = self
      .streams
      .values()
      .map(|stream| stream.anchor)
      .min()
      .unwrap_or(self.pending.len());
    let mut emitted = Vec::with_capacity(count);
    for _ in 0..count {
      let pending = self
        .pending
        .pop_front()
        .ok_or_else(|| RunnerSupervisionError::Protocol("redaction queue became inconsistent".to_owned()))?;
      self.pending_bytes -= pending.bytes;
      emitted.push(pending.event);
    }
    for stream in self.streams.values_mut() {
      stream.anchor -= count;
    }
    Ok(emitted)
  }

  pub(super) fn results(&self, results: &mut [serde_json::Value]) {
    for result in results {
      redact_value(result, &self.values);
    }
  }

  pub(super) fn message(&self, message: String) -> String {
    redact_string(message, &self.values)
  }
}

struct PendingEvent {
  event: RunnerEvent,
  bytes: usize,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct StreamKey {
  run_id: u64,
  step_id: Option<u64>,
  command_id: String,
  stream: String,
}

struct RedactionStream {
  anchor: usize,
  pending: VecDeque<u8>,
  state_history: VecDeque<usize>,
  base_states: Vec<usize>,
  states: Vec<usize>,
}

impl RedactionStream {
  fn new(anchor: usize, pattern_count: usize) -> Self {
    Self {
      anchor,
      pending: VecDeque::new(),
      state_history: VecDeque::new(),
      base_states: vec![0; pattern_count],
      states: vec![0; pattern_count],
    }
  }

  fn push(&mut self, bytes: Vec<u8>, values: &[Zeroizing<Vec<u8>>], prefix_tables: &[Vec<usize>]) -> Vec<u8> {
    let bytes = Zeroizing::new(bytes);
    let mut output = Vec::with_capacity(bytes.len());
    for &byte in bytes.iter() {
      self.push_byte(byte, values, prefix_tables);
      if let Some((index, matched)) = self
        .states
        .iter()
        .enumerate()
        .find(|(index, state)| **state == values[*index].len())
      {
        let matched = *matched;
        self.remove_match(values[index].len());
        self.append_safe_marker(matched, values, prefix_tables);
      }
      self.release_safe_prefix(&mut output);
    }
    output
  }

  fn finish(&mut self) -> Vec<u8> {
    self.states.fill(0);
    self.base_states.fill(0);
    self.state_history.clear();
    self.pending.drain(..).collect()
  }

  fn finish_masked(&mut self, values: &[Zeroizing<Vec<u8>>], prefix_tables: &[Vec<usize>]) -> Vec<u8> {
    let removed = self.pending.len();
    self.remove_match(removed);
    self.append_safe_marker(removed, values, prefix_tables);
    self.finish()
  }

  fn push_byte(&mut self, byte: u8, values: &[Zeroizing<Vec<u8>>], prefix_tables: &[Vec<usize>]) {
    self.pending.push_back(byte);
    advance_states(&mut self.states, byte, values, prefix_tables);
    self.state_history.extend(self.states.iter().copied());
  }

  fn remove_match(&mut self, matched_bytes: usize) {
    for _ in 0..matched_bytes {
      if let Some(mut byte) = self.pending.pop_back() {
        byte.zeroize();
      }
      for _ in 0..self.states.len() {
        self.state_history.pop_back();
      }
    }
    if self.pending.is_empty() {
      self.states.clone_from(&self.base_states);
    } else {
      let start = self.state_history.len() - self.states.len();
      for (index, state) in self.states.iter_mut().enumerate() {
        *state = self.state_history[start + index];
      }
    }
  }

  fn append_safe_marker(&mut self, removed_bytes: usize, values: &[Zeroizing<Vec<u8>>], prefix_tables: &[Vec<usize>]) {
    let mut preferred = Vec::new();
    append_marker(&mut preferred, removed_bytes);
    let fallback = *b"#";
    let candidate = if self.marker_is_safe(&preferred, values, prefix_tables) {
      preferred.as_slice()
    } else if self.marker_is_safe(&fallback, values, prefix_tables) {
      fallback.as_slice()
    } else {
      // An empty replacement is always safe. The current automaton state is
      // retained, so a secret formed by joining the surrounding bytes is
      // still recognized when the following chunk arrives.
      &[]
    };
    for &byte in candidate {
      self.push_byte(byte, values, prefix_tables);
    }
  }

  fn marker_is_safe(&self, marker: &[u8], values: &[Zeroizing<Vec<u8>>], prefix_tables: &[Vec<usize>]) -> bool {
    let mut states = self.states.clone();
    for &byte in marker {
      advance_states(&mut states, byte, values, prefix_tables);
      if states
        .iter()
        .enumerate()
        .any(|(index, state)| *state == values[index].len())
      {
        return false;
      }
    }
    true
  }

  fn release_safe_prefix(&mut self, output: &mut Vec<u8>) {
    let retained = self.states.iter().copied().max().unwrap_or(0);
    while self.pending.len() > retained {
      output.push(self.pending.pop_front().expect("pending length was checked"));
      for state in &mut self.base_states {
        *state = self
          .state_history
          .pop_front()
          .expect("every pending byte retains one state per pattern");
      }
    }
  }

  fn zero_pending(&mut self) {
    for byte in &mut self.pending {
      byte.zeroize();
    }
    self.pending.clear();
  }

  fn allocated_bytes(&self) -> usize {
    let state_bytes = std::mem::size_of::<usize>();
    self
      .pending
      .capacity()
      .saturating_add(self.state_history.capacity().saturating_mul(state_bytes))
      .saturating_add(self.base_states.capacity().saturating_mul(state_bytes))
      .saturating_add(self.states.capacity().saturating_mul(state_bytes))
  }
}

impl Drop for RedactionStream {
  fn drop(&mut self) {
    self.zero_pending();
  }
}

#[derive(Clone, Copy)]
enum OutputFormat {
  Bytes,
  RawBytes,
  Line,
}

fn output_payload(event: &RunnerEvent) -> Option<(StreamKey, OutputFormat, Vec<u8>)> {
  if event.data.get("type")?.as_str()? != "output" {
    return None;
  }
  let payload = event.data.get("payload")?.as_object()?;
  let format = match payload.get("format")?.as_str()? {
    "bytes" => OutputFormat::Bytes,
    "raw_bytes" => OutputFormat::RawBytes,
    "line" => OutputFormat::Line,
    _ => return None,
  };
  let data = payload.get("data")?.as_str()?;
  let bytes = match format {
    OutputFormat::Bytes | OutputFormat::RawBytes => STANDARD.decode(data).ok()?,
    OutputFormat::Line => data.as_bytes().to_vec(),
  };
  Some((
    StreamKey {
      run_id: event.data.get("run_id")?.as_u64()?,
      step_id: event.data.get("step_id").and_then(serde_json::Value::as_u64),
      command_id: event.data.get("command_id")?.as_str()?.to_owned(),
      stream: event.data.get("stream")?.as_str()?.to_owned(),
    },
    format,
    bytes,
  ))
}

fn set_output_data(event: &mut RunnerEvent, bytes: Vec<u8>, format: OutputFormat) {
  let payload = event
    .data
    .get_mut("payload")
    .and_then(serde_json::Value::as_object_mut)
    .expect("validated output event retains its payload");
  payload.insert(
    "format".to_owned(),
    serde_json::Value::String(
      match format {
        OutputFormat::Bytes => "bytes",
        OutputFormat::RawBytes => "raw_bytes",
        OutputFormat::Line => "line",
      }
      .to_owned(),
    ),
  );
  let data = match format {
    OutputFormat::Bytes | OutputFormat::RawBytes => STANDARD.encode(bytes),
    OutputFormat::Line => String::from_utf8(bytes).expect("line payload was valid UTF-8"),
  };
  payload.insert("data".to_owned(), serde_json::Value::String(data));
}

fn prefix_table(pattern: &[u8]) -> Vec<usize> {
  let mut table = vec![0; pattern.len()];
  let mut matched = 0;
  for index in 1..pattern.len() {
    while matched > 0 && pattern[index] != pattern[matched] {
      matched = table[matched - 1];
    }
    if pattern[index] == pattern[matched] {
      matched += 1;
    }
    table[index] = matched;
  }
  table
}

fn advance_prefix(mut state: usize, byte: u8, pattern: &[u8], table: &[usize]) -> usize {
  while state > 0 && byte != pattern[state] {
    state = table[state - 1];
  }
  if byte == pattern[state] {
    state += 1;
  }
  state
}

fn advance_states(states: &mut [usize], byte: u8, values: &[Zeroizing<Vec<u8>>], prefix_tables: &[Vec<usize>]) {
  for ((state, value), table) in states.iter_mut().zip(values).zip(prefix_tables) {
    *state = advance_prefix(*state, byte, value, table);
  }
}

impl std::fmt::Debug for RunnerRedactions {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("RunnerRedactions")
      .field("value_count", &self.values.len())
      .finish()
  }
}

fn redact_map(map: &mut serde_json::Map<String, serde_json::Value>, values: &[Zeroizing<Vec<u8>>]) {
  // Byte console payloads are encoded, so replacing only JSON strings would
  // miss the most direct `cat /run/octa-identity` output path.
  if matches!(
    map.get("format").and_then(serde_json::Value::as_str),
    Some("bytes" | "raw_bytes")
  ) && let Some(serde_json::Value::String(encoded)) = map.get_mut("data")
    && let Ok(mut bytes) = STANDARD.decode(&*encoded)
  {
    redact_bytes(&mut bytes, values);
    *encoded = STANDARD.encode(bytes);
  }
  for value in map.values_mut() {
    redact_value(value, values);
  }
}

fn redact_value(value: &mut serde_json::Value, values: &[Zeroizing<Vec<u8>>]) {
  match value {
    serde_json::Value::String(string) => {
      *string = redact_string(std::mem::take(string), values);
    }
    serde_json::Value::Array(items) => {
      for item in items {
        redact_value(item, values);
      }
    }
    serde_json::Value::Object(map) => redact_map(map, values),
    serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {}
  }
}

fn redact_string(value: String, values: &[Zeroizing<Vec<u8>>]) -> String {
  let mut bytes = value.into_bytes();
  redact_bytes(&mut bytes, values);
  String::from_utf8(bytes).expect("replacing bytes with ASCII preserves valid UTF-8")
}

fn redact_bytes(bytes: &mut Vec<u8>, values: &[Zeroizing<Vec<u8>>]) {
  for needle in values {
    let finder = memchr::memmem::Finder::new(needle.as_slice());
    let Some(first) = finder.find(bytes) else {
      continue;
    };
    let mut redacted = Vec::with_capacity(bytes.len());
    redacted.extend_from_slice(&bytes[..first]);
    append_marker(&mut redacted, needle.len());
    let mut copied = first + needle.len();
    while let Some(relative) = finder.find(&bytes[copied..]) {
      let position = copied + relative;
      redacted.extend_from_slice(&bytes[copied..position]);
      append_marker(&mut redacted, needle.len());
      copied = position + needle.len();
    }
    redacted.extend_from_slice(&bytes[copied..]);
    bytes.zeroize();
    *bytes = redacted;
  }
}

fn append_marker(target: &mut Vec<u8>, removed_bytes: usize) {
  if removed_bytes < REDACTION_MARKER.len() {
    target.extend(std::iter::repeat_n(b'*', removed_bytes));
  } else {
    target.extend_from_slice(REDACTION_MARKER);
  }
}

fn event_wire_bytes(event: &RunnerEvent) -> Result<usize, RunnerSupervisionError> {
  serde_json::to_vec(event)
    .map(|bytes| bytes.len())
    .map_err(|error| RunnerSupervisionError::Protocol(format!("cannot size event during redaction: {error}")))
}

fn redaction_limit(kind: &str) -> RunnerSupervisionError {
  RunnerSupervisionError::Protocol(format!("redaction {kind} exceeds the bounded supervisor limit"))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn redacts_strings_byte_payloads_and_debug_output() {
    let secret = b"signed-workload-jwt";
    let mut redactions = RunnerRedactions::new([secret.as_slice()]);
    let event = RunnerEvent {
      schema_version: 4,
      sequence: 1,
      timestamp: "2026-09-14T00:00:00Z".to_owned(),
      category: "execution".to_owned(),
      data: serde_json::json!({
        "message": "token=signed-workload-jwt",
        "payload": {
          "format": "bytes",
          "data": STANDARD.encode(b"before signed-workload-jwt after")
        }
      })
      .as_object()
      .unwrap()
      .clone(),
    };

    let event = redactions.events(event).unwrap().pop().unwrap();

    let serialized = serde_json::to_string(&event).unwrap();
    assert!(!serialized.contains("signed-workload-jwt"));
    let encoded = event.data["payload"]["data"].as_str().unwrap();
    assert_eq!(STANDARD.decode(encoded).unwrap(), b"before [redacted] after");
    assert_eq!(format!("{redactions:?}"), "RunnerRedactions { value_count: 1 }");
  }

  #[test]
  fn redacts_a_secret_split_across_binary_events() {
    let mut redactions = RunnerRedactions::new([b"signed-workload-jwt".as_slice()]);
    let first = output_event(1, b"before signed-workload-");
    let second = output_event(2, b"jwt after");
    let finished = RunnerEvent {
      schema_version: 4,
      sequence: 3,
      timestamp: "2026-09-14T00:00:00Z".to_owned(),
      category: "execution".to_owned(),
      data: serde_json::json!({
        "type": "step_finished",
        "run_id": 7,
        "scope": { "id": 1, "parent_id": null, "name": "build" },
        "step": { "id": 9, "parent_task_id": 1, "label": "shell" },
        "status": "success"
      })
      .as_object()
      .unwrap()
      .clone(),
    };

    let mut events = redactions.events(first).unwrap();
    events.extend(redactions.events(second).unwrap());
    events.extend(redactions.events(finished).unwrap());

    let serialized = serde_json::to_vec(&events).unwrap();
    assert!(
      !serialized
        .windows(b"signed-workload-jwt".len())
        .any(|value| value == b"signed-workload-jwt")
    );
    let output = events
      .iter()
      .filter_map(|event| output_payload(event).map(|(_, _, bytes)| bytes))
      .flatten()
      .collect::<Vec<_>>();
    assert_eq!(output, b"before [redacted] after");
    assert_eq!(events.iter().map(|event| event.sequence).collect::<Vec<_>>(), [1, 2, 3]);
  }

  #[test]
  fn streaming_matcher_handles_self_overlapping_prefixes() {
    let mut redactions = RunnerRedactions::new([b"aaaaab".as_slice()]);
    let mut events = redactions.events(output_event(1, b"aaaa")).unwrap();
    events.extend(redactions.events(output_event(2, b"aab after")).unwrap());
    events.extend(redactions.finish_events().unwrap());

    let output = events
      .iter()
      .filter_map(|event| output_payload(event).map(|(_, _, bytes)| bytes))
      .flatten()
      .collect::<Vec<_>>();
    assert_eq!(output, b"a****** after");
  }

  #[test]
  fn completed_matches_do_not_accumulate_in_the_stream_buffer() {
    let input = vec![b'x'; 64 * 1024];
    let mut redactions = RunnerRedactions::new([b"x".as_slice()]);

    let events = redactions.events(output_event(1, &input)).unwrap();

    let output = events
      .iter()
      .filter_map(|event| output_payload(event).map(|(_, _, bytes)| bytes))
      .flatten()
      .collect::<Vec<_>>();
    assert_eq!(output.len(), input.len());
    assert!(output.iter().all(|byte| *byte == b'*'));
    assert!(redactions.streams.is_empty());
    assert!(redactions.pending.is_empty());
  }

  #[test]
  fn replacement_never_reproduces_the_protected_value() {
    for secret in [b"*".as_slice(), REDACTION_MARKER] {
      let mut redactions = RunnerRedactions::new([secret]);
      let mut events = redactions.events(output_event(1, secret)).unwrap();
      events.extend(redactions.finish_events().unwrap());
      let output = events
        .iter()
        .filter_map(|event| output_payload(event).map(|(_, _, bytes)| bytes))
        .flatten()
        .collect::<Vec<_>>();

      assert!(!output.windows(secret.len()).any(|candidate| candidate == secret));
    }
  }

  #[test]
  fn replacement_cannot_complete_a_secret_across_the_next_chunk() {
    let mut redactions = RunnerRedactions::new([b"xy".as_slice(), b"abz".as_slice()]);
    let mut events = redactions.events(output_event(1, b"xabz")).unwrap();
    events.extend(redactions.events(output_event(2, b"y after")).unwrap());
    events.extend(redactions.finish_events().unwrap());
    let output = events
      .iter()
      .filter_map(|event| output_payload(event).map(|(_, _, bytes)| bytes))
      .flatten()
      .collect::<Vec<_>>();

    assert!(!output.windows(2).any(|candidate| candidate == b"xy"));
    assert!(!output.windows(3).any(|candidate| candidate == b"abz"));
    assert!(output.ends_with(b"y after"));
  }

  #[test]
  fn pressure_accounting_includes_allocated_matcher_history() {
    let mut redactions = RunnerRedactions::new([b"abc".as_slice(), b"xyz".as_slice()]);
    let mut stream = RedactionStream::new(0, redactions.values.len());
    assert!(
      stream
        .push(b"a".to_vec(), &redactions.values, &redactions.prefix_tables)
        .is_empty()
    );
    let pending_capacity = stream.pending.capacity();
    let allocated = stream.allocated_bytes();
    redactions.streams.insert(
      StreamKey {
        run_id: 1,
        step_id: Some(1),
        command_id: "command".to_owned(),
        stream: "stdout".to_owned(),
      },
      stream,
    );

    assert!(allocated > pending_capacity);
    assert_eq!(redactions.total_bytes(), allocated);
  }

  #[test]
  fn terminal_flush_is_bounded_and_keeps_sequences_monotonic() {
    let mut redactions = RunnerRedactions::new([b"signed-workload-jwt".as_slice()]);
    let emitted = redactions.events(output_event(10, b"ordinary s")).unwrap();
    let flushed = redactions.finish_events().unwrap();

    assert!(emitted.is_empty());
    assert_eq!(flushed[0].sequence, 10);
    assert_eq!(output_payload(&flushed[0]).unwrap().2, b"ordinary s");
  }

  #[test]
  fn stream_capacity_releases_ordered_events_and_short_redactions_never_expand() {
    let redactions = RunnerRedactions::new([b"x".as_slice()]);
    assert_eq!(redactions.message("x".to_owned()), "*");

    let mut redactions = RunnerRedactions::new([b"xy".as_slice()]);
    for index in 0..MAX_REDACTION_STREAMS {
      assert!(
        redactions
          .events(output_event_for(index as u64 + 1, &format!("command-{index}"), b"x"))
          .is_ok()
      );
    }
    let emitted = redactions.events(output_event_for(100, "overflow", b"x")).unwrap();
    assert_eq!(emitted.len(), MAX_REDACTION_STREAMS);
    assert_eq!(emitted[0].sequence, 1);
    assert_eq!(emitted.last().unwrap().sequence, MAX_REDACTION_STREAMS as u64);
    assert_eq!(redactions.streams.len(), 1);
  }

  #[test]
  fn interleaving_does_not_mask_an_incomplete_prefix() {
    let mut redactions = RunnerRedactions::new([b"xy".as_slice()]);
    assert!(
      redactions
        .events(output_event_for(1, "stdout", b"x"))
        .unwrap()
        .is_empty()
    );
    assert!(
      redactions
        .events(output_event_for(2, "stderr", b"ordinary"))
        .unwrap()
        .is_empty()
    );
    let mut emitted = redactions.events(output_event_for(3, "stdout", b"z")).unwrap();
    emitted.extend(redactions.finish_events().unwrap());

    assert_eq!(
      emitted.iter().map(|event| event.sequence).collect::<Vec<_>>(),
      [1, 2, 3]
    );
    let stdout = emitted
      .iter()
      .filter(|event| event.data["command_id"] == "stdout")
      .filter_map(|event| output_payload(event).map(|(_, _, bytes)| bytes))
      .flatten()
      .collect::<Vec<_>>();
    let stderr = emitted
      .iter()
      .filter(|event| event.data["command_id"] == "stderr")
      .filter_map(|event| output_payload(event).map(|(_, _, bytes)| bytes))
      .flatten()
      .collect::<Vec<_>>();
    assert_eq!(stdout, b"xz");
    assert_eq!(stderr, b"ordinary");
  }

  #[test]
  fn redacts_terminal_results_and_runner_errors() {
    let redactions = RunnerRedactions::new([b"identity-token".as_slice()]);
    let mut results = vec![serde_json::json!({
      "output": "identity-token",
      "nested": ["prefix identity-token suffix"]
    })];

    redactions.results(&mut results);

    assert_eq!(results[0]["output"], "[redacted]");
    assert_eq!(results[0]["nested"][0], "prefix [redacted] suffix");
    assert_eq!(
      redactions.message("runner exposed identity-token".to_owned()),
      "runner exposed [redacted]"
    );
  }

  #[test]
  fn redacts_the_trimmed_form_of_a_line_terminated_identity() {
    let redactions = RunnerRedactions::new([b"signed-jwt\r\n".as_slice()]);
    assert_eq!(redactions.message("signed-jwt".to_owned()), "[redacted]");
    assert_eq!(redactions.message("signed-jwt\r\n".to_owned()), "[redacted]");
  }

  fn output_event(sequence: u64, bytes: &[u8]) -> RunnerEvent {
    output_event_for(sequence, "command-1", bytes)
  }

  fn output_event_for(sequence: u64, command_id: &str, bytes: &[u8]) -> RunnerEvent {
    RunnerEvent {
      schema_version: 4,
      sequence,
      timestamp: "2026-09-14T00:00:00Z".to_owned(),
      category: "execution".to_owned(),
      data: serde_json::json!({
        "type": "output",
        "run_id": 7,
        "scope": { "id": 1, "parent_id": null, "name": "build" },
        "step_id": 9,
        "command_id": command_id,
        "stream": "stdout",
        "payload": { "format": "bytes", "data": STANDARD.encode(bytes) }
      })
      .as_object()
      .unwrap()
      .clone(),
    }
  }
}
