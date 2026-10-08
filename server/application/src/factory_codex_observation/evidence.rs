//! Integrity and schema checks over generic runner evidence.

use std::collections::BTreeMap;

use octacity_protocol::RunnerEventPayload;
use octacity_server_artifacts::{ArtifactIdentity, ArtifactType};
use octacity_server_factory::{FactoryArtifactReference, FactoryDigest, FactoryTaskEnvelope};
use serde::Deserialize;

use super::{FactoryCodexOutputDocument, records::CodexProvenanceRecord, records::CodexResultRecord};
use crate::factory_codex_contract::{
  CODEX_EXECUTABLE_IDENTITY, CODEX_PLUGIN_PROVENANCE_IDENTITY, CODEX_RESULT_FORMAT, JSON_MEDIA_TYPE, TRACE_MEDIA_TYPE,
};

const MAX_TRACE_RECORDS: usize = 100_000;
const MAX_TRACE_DEPTH: usize = 16;
const MAX_TRACE_NODES: usize = 8_192;
const MAX_TRACE_STRING_BYTES: usize = 64 * 1024;
const MAX_HARNESS_IDENTIFIERS: usize = 16;
const MAX_USAGE_FIELDS: usize = 16;

pub(super) fn output_map(outputs: &[ArtifactIdentity]) -> Option<BTreeMap<&str, &ArtifactIdentity>> {
  let mut mapped = BTreeMap::new();
  for output in outputs {
    if mapped.insert(output.logical_name.as_str(), output).is_some() {
      return None;
    }
  }
  Some(mapped)
}

pub(super) fn document_map<'a>(
  documents: &'a [FactoryCodexOutputDocument],
  outputs: &BTreeMap<&str, &ArtifactIdentity>,
) -> Option<BTreeMap<&'a str, &'a FactoryCodexOutputDocument>> {
  let mut mapped = BTreeMap::new();
  for document in documents {
    let expected = outputs.get(document.identity.logical_name.as_str())?;
    if *expected != &document.identity
      || document.identity.size_bytes != u64::try_from(document.bytes.len()).ok()?
      || FactoryDigest::content_sha256(&document.bytes).as_bytes() != document.identity.digest.as_bytes()
      || mapped
        .insert(document.identity.logical_name.as_str(), document)
        .is_some()
    {
      return None;
    }
  }
  Some(mapped)
}

pub(super) fn record_output_types(
  result: &FactoryCodexOutputDocument,
  trace: &FactoryCodexOutputDocument,
  provenance: &FactoryCodexOutputDocument,
) -> bool {
  matches!(
    &result.identity.artifact_type,
    ArtifactType::Report(format) if format.as_str() == CODEX_RESULT_FORMAT
  ) && result.identity.media_type.as_str() == JSON_MEDIA_TYPE
    && matches!(trace.identity.artifact_type, ArtifactType::Artifact)
    && trace.identity.media_type.as_str() == TRACE_MEDIA_TYPE
    && matches!(provenance.identity.artifact_type, ArtifactType::Artifact)
    && provenance.identity.media_type.as_str() == JSON_MEDIA_TYPE
}

pub(super) fn artifact_reference(identity: &ArtifactIdentity) -> Option<FactoryArtifactReference> {
  FactoryArtifactReference::new(
    identity.artifact_id,
    FactoryDigest::from_bytes(identity.digest.as_bytes()),
    identity.size_bytes,
  )
  .ok()
}

pub(super) fn valid_provenance(
  envelope: &FactoryTaskEnvelope,
  result: &CodexResultRecord,
  provenance: &CodexProvenanceRecord,
) -> bool {
  provenance.format_version == 1
    && result.format_version == 1
    && valid_identifiers(&result.harness_identifiers)
    && valid_usage(&result.usage)
    && provenance.trace_format_version == 1
    && provenance.result_format_version == 1
    && provenance.plugin.name == CODEX_PLUGIN_PROVENANCE_IDENTITY
    && provenance.plugin.version == envelope.plugin().version().as_str()
    && provenance.codex.name == CODEX_EXECUTABLE_IDENTITY
    && provenance.codex.version == envelope.executable().version().as_str()
    && provenance.settings.model.as_deref() == Some(envelope.model().identity().as_str())
    && provenance
      .settings
      .reasoning_effort
      .as_deref()
      .is_none_or(|value| matches!(value, "low" | "medium" | "high" | "xhigh"))
    && provenance.prompt_digest.algorithm == "blake3"
    && provenance.prompt_digest.value == envelope.digests().prompt_provenance.to_string()
    && provenance.source_revision.as_deref() == Some(expected_revision(envelope))
    && provenance.timing.finished_unix_millis >= provenance.timing.started_unix_millis
    && provenance.timing.finished_unix_millis - provenance.timing.started_unix_millis
      == provenance.timing.duration_millis
    && provenance.outcome == result.outcome
    && provenance.usage == result.usage
}

fn expected_revision(envelope: &FactoryTaskEnvelope) -> &str {
  envelope
    .subject()
    .candidate()
    .map_or_else(
      || envelope.subject().exact().base_revision(),
      |candidate| candidate.candidate_revision(),
    )
    .as_str()
}

fn valid_identifiers(values: &BTreeMap<String, String>) -> bool {
  values.len() <= MAX_HARNESS_IDENTIFIERS
    && values
      .iter()
      .all(|(key, value)| valid_field_name(key) && !value.is_empty() && value.len() <= 256 && control_free(value))
}

fn valid_usage(values: &BTreeMap<String, u64>) -> bool {
  !values.is_empty()
    && values.len() <= MAX_USAGE_FIELDS
    && values.contains_key("input_tokens")
    && values.contains_key("output_tokens")
    && values.keys().all(|key| valid_field_name(key))
}

fn valid_field_name(value: &str) -> bool {
  !value.is_empty()
    && value.len() <= 64
    && value
      .bytes()
      .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn control_free(value: &str) -> bool {
  value.chars().all(|character| !character.is_control())
}

pub(super) fn valid_trace(bytes: &[u8]) -> bool {
  if bytes.is_empty() || !bytes.ends_with(b"\n") {
    return false;
  }
  let mut expected_sequence = 0_u64;
  for (index, line) in bytes
    .split(|byte| *byte == b'\n')
    .filter(|line| !line.is_empty())
    .enumerate()
  {
    if index >= MAX_TRACE_RECORDS {
      return false;
    }
    let Ok(record) = serde_json::from_slice::<CodexTraceRecord>(line) else {
      return false;
    };
    if record.format_version != 1 || record.sequence != expected_sequence || !valid_trace_event(&record.event) {
      return false;
    }
    expected_sequence = expected_sequence.saturating_add(1);
  }
  expected_sequence > 0
}

fn valid_trace_event(event: &serde_json::Value) -> bool {
  let Some(object) = event.as_object() else {
    return false;
  };
  if !object
    .get("type")
    .and_then(serde_json::Value::as_str)
    .is_some_and(valid_trace_discriminator)
  {
    return false;
  }
  let mut remaining = MAX_TRACE_NODES;
  valid_trace_value(event, 0, &mut remaining)
}

fn valid_trace_discriminator(value: &str) -> bool {
  !value.is_empty()
    && value.len() <= 64
    && value
      .bytes()
      .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'.' | b'-'))
}

fn valid_trace_value(value: &serde_json::Value, depth: usize, remaining: &mut usize) -> bool {
  if depth > MAX_TRACE_DEPTH || *remaining == 0 {
    return false;
  }
  *remaining -= 1;
  match value {
    serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => true,
    serde_json::Value::String(value) => {
      value.len() <= MAX_TRACE_STRING_BYTES && control_free(value) && !contains_sensitive_trace_material(value)
    }
    serde_json::Value::Array(values) => values
      .iter()
      .all(|value| valid_trace_value(value, depth + 1, remaining)),
    serde_json::Value::Object(values) => values
      .iter()
      .all(|(key, value)| valid_trace_key(key) && valid_trace_value(value, depth + 1, remaining)),
  }
}

fn valid_trace_key(value: &str) -> bool {
  value.len() <= 64
    && control_free(value)
    && !matches!(
      value.to_ascii_lowercase().as_str(),
      "authorization" | "credential" | "password" | "private_key" | "secret" | "signature" | "token"
    )
}

fn contains_sensitive_trace_material(value: &str) -> bool {
  let value = value.to_ascii_lowercase();
  [
    "-----begin ",
    "authorization:",
    "aws_secret_access_key",
    "bearer ",
    "github_pat_",
    "ghp_",
    "password=",
    "password\":",
    "private_key=",
    "secret=",
    "sk-proj-",
    "token=",
    "x-amz-signature",
  ]
  .iter()
  .any(|marker| value.contains(marker))
}

pub(super) fn valid_runner_events(events: &[RunnerEventPayload]) -> bool {
  let mut expected_sequence = 0_u64;
  let mut started_run = None;
  let mut finished = false;
  for event in events {
    if event.schema_version != 4
      || event.sequence != expected_sequence
      || !matches!(event.category.as_str(), "execution" | "diagnostic" | "document")
    {
      return false;
    }
    expected_sequence = expected_sequence.saturating_add(1);
    if event.category != "execution" {
      if started_run.is_none() || finished {
        return false;
      }
      continue;
    }
    match event.data.get("type").and_then(serde_json::Value::as_str) {
      Some("run_started") if started_run.is_none() && !finished => {
        started_run = event.data.get("run_id").and_then(serde_json::Value::as_u64);
        if started_run.is_none() {
          return false;
        }
      }
      Some("run_finished") if started_run.is_some() && !finished => {
        if event.data.get("run_id").and_then(serde_json::Value::as_u64) != started_run
          || event.data.get("status").and_then(serde_json::Value::as_str) != Some("success")
        {
          return false;
        }
        finished = true;
      }
      Some(_) if started_run.is_some() && !finished => {}
      _ => return false,
    }
  }
  finished
}

pub(super) fn valid_provider_unavailable_events(events: &[RunnerEventPayload]) -> bool {
  let mut expected_sequence = 0_u64;
  let mut run_id = None;
  let mut unavailable = false;
  let mut failed = false;
  for event in events {
    if event.schema_version != 4
      || event.sequence != expected_sequence
      || !matches!(event.category.as_str(), "execution" | "diagnostic" | "document")
    {
      return false;
    }
    expected_sequence = expected_sequence.saturating_add(1);
    match (
      event.category.as_str(),
      event.data.get("type").and_then(serde_json::Value::as_str),
    ) {
      ("execution", Some("run_started")) if run_id.is_none() => {
        run_id = event.data.get("run_id").and_then(serde_json::Value::as_u64);
        if run_id.is_none() {
          return false;
        }
      }
      ("diagnostic", Some("provider_unavailable")) if run_id.is_some() && !failed => unavailable = true,
      ("execution", Some("run_finished")) if run_id.is_some() && !failed => {
        failed = event.data.get("run_id").and_then(serde_json::Value::as_u64) == run_id
          && event.data.get("status").and_then(serde_json::Value::as_str) == Some("failed");
        if !failed {
          return false;
        }
      }
      (_, Some(_)) if run_id.is_some() && !failed => {}
      _ => return false,
    }
  }
  unavailable && failed
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CodexTraceRecord {
  format_version: u16,
  sequence: u64,
  event: serde_json::Value,
}
