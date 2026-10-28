//! Bounded native tool observations over the existing trace Artifact.
use std::collections::BTreeMap;

use octacity_server_artifacts::ArtifactIdentity;
use octacity_server_domain::Timestamp;
use octacity_server_factory::FactoryDigest;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ApplicationError;

/// Hard observation ceiling; overflow fails rather than dropping tool evidence.
pub const MAX_FACTORY_MCP_CAPTURE_CALLS: usize = 64;

/// Untrusted conclusion reference to one exact retained native observation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryMcpEvidenceReference {
  /// Immutable source Artifact; another trace with identical call names cannot substitute.
  pub trace: octacity_server_factory::FactoryArtifactReference,
  /// Completion sequence, or the start sequence of an interrupted call.
  pub sequence: u64,
  /// Native item identity inside that Artifact.
  pub call_id: String,
  /// Sanitized response identity, absent when no response was consumed.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub result_digest: Option<FactoryDigest>,
}

/// Native call lifecycle; a stopped invocation does not establish a remote result.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryMcpCallStatus {
  /// The native client consumed a successful response.
  Completed,
  /// A refused, failed or unavailable call, with optional consumed error content.
  Failed,
  /// Native completion did not contain a valid consumed MCP response.
  Malformed,
  /// The call started but no completion was captured.
  Interrupted,
}

/// Verified local observation. Its raw content remains untrusted provider data.
pub struct FactoryMcpCallObservation {
  trace: ArtifactIdentity,
  id: String,
  connection: String,
  tool: String,
  arguments: Value,
  started_sequence: u64,
  completed_sequence: Option<u64>,
  observed_at: Timestamp,
  status: FactoryMcpCallStatus,
  result: Option<Value>,
  error: Option<Value>,
}
impl FactoryMcpCallObservation {
  /// Exact ordinary Artifact/record/content locator for this observation.
  pub fn reference(&self) -> FactoryMcpEvidenceReference {
    FactoryMcpEvidenceReference {
      trace: super::evidence::artifact_reference(&self.trace).expect("a retained call has a nonempty verified trace"),
      sequence: self.completed_sequence.unwrap_or(self.started_sequence),
      call_id: self.id.clone(),
      result_digest: self.result_digest(),
    }
  }
  /// Native item identity, scoped to this exact trace.
  pub fn id(&self) -> &str {
    &self.id
  }
  /// Native connection alias; execution authority is checked separately.
  pub fn connection(&self) -> &str {
    &self.connection
  }
  /// Actual native tool name.
  pub fn tool(&self) -> &str {
    &self.tool
  }
  /// Sanitized arguments actually sent by the native client.
  pub const fn arguments(&self) -> &Value {
    &self.arguments
  }
  /// Exact ordinary trace Artifact, including original Build/Attempt/Job.
  pub const fn trace(&self) -> &ArtifactIdentity {
    &self.trace
  }
  /// Start record sequence in this immutable trace.
  pub const fn started_sequence(&self) -> u64 {
    self.started_sequence
  }
  /// Completion record sequence, absent for an interrupted call.
  pub const fn completed_sequence(&self) -> Option<u64> {
    self.completed_sequence
  }
  /// Local observation time inside the verified invocation window.
  pub const fn observed_at(&self) -> Timestamp {
    self.observed_at
  }
  /// Actual captured lifecycle, independent of any model claim.
  pub const fn status(&self) -> FactoryMcpCallStatus {
    self.status
  }
  /// Sanitized consumed MCP response; absence is not an empty response.
  pub const fn result(&self) -> Option<&Value> {
    self.result.as_ref()
  }
  /// Sanitized native error when no successful result was available.
  pub const fn error(&self) -> Option<&Value> {
    self.error.as_ref()
  }
  /// Content identity of the retained response, not of unavailable original bytes.
  pub fn result_digest(&self) -> Option<FactoryDigest> {
    self
      .result
      .as_ref()
      .map(|value| FactoryDigest::content_sha256(&serde_json::to_vec(value).expect("JSON value")))
  }
}

pub(super) fn observations(
  trace: &ArtifactIdentity,
  bytes: &[u8],
) -> Result<Vec<FactoryMcpCallObservation>, ApplicationError> {
  let invalid = ApplicationError::invalid;
  let mut calls: Vec<FactoryMcpCallObservation> = Vec::new();
  let mut by_id = BTreeMap::new();
  for line in bytes.split(|byte| *byte == b'\n').filter(|line| !line.is_empty()) {
    let record: Value = serde_json::from_slice(line).map_err(|_| invalid())?;
    let event = &record["event"];
    let item = &event["item"];
    if item["type"] != "mcp_tool_call" {
      continue;
    }
    let id = item["id"].as_str().ok_or_else(invalid)?;
    let connection = item["server"].as_str().ok_or_else(invalid)?;
    let tool = item["tool"].as_str().ok_or_else(invalid)?;
    let arguments = item.get("arguments").ok_or_else(invalid)?;
    let sequence = record["sequence"].as_u64().ok_or_else(invalid)?;
    let observed = record["observed_at_unix_millis"].as_i64().ok_or_else(invalid)?;
    let observed_at = Timestamp::from_unix_millis(observed).map_err(|_| invalid())?;
    match event["type"].as_str() {
      Some("item.started") if !by_id.contains_key(id) && item["status"] == "in_progress" => {
        if calls.len() == MAX_FACTORY_MCP_CAPTURE_CALLS {
          return Err(invalid());
        }
        by_id.insert(id.to_owned(), calls.len());
        calls.push(FactoryMcpCallObservation {
          trace: trace.clone(),
          id: id.into(),
          connection: connection.into(),
          tool: tool.into(),
          arguments: arguments.clone(),
          started_sequence: sequence,
          completed_sequence: None,
          observed_at,
          status: FactoryMcpCallStatus::Interrupted,
          result: None,
          error: None,
        });
      }
      Some("item.completed") => {
        let call = by_id
          .get(id)
          .and_then(|index| calls.get_mut(*index))
          .ok_or_else(invalid)?;
        if call.completed_sequence.is_some()
          || call.connection != connection
          || call.tool != tool
          || call.arguments != *arguments
        {
          return Err(invalid());
        }
        call.status = match item["status"].as_str() {
          Some("completed") => FactoryMcpCallStatus::Completed,
          Some("failed") => FactoryMcpCallStatus::Failed,
          _ => return Err(invalid()),
        };
        call.completed_sequence = Some(sequence);
        call.observed_at = observed_at;
        call.result = (!item["result"].is_null()).then(|| item["result"].clone());
        call.error = (!item["error"].is_null()).then(|| item["error"].clone());
        if call.status == FactoryMcpCallStatus::Completed {
          call.status = match call.result.as_ref() {
            Some(result) if result.get("isError").or_else(|| result.get("is_error")) == Some(&Value::Bool(true)) => {
              FactoryMcpCallStatus::Failed
            }
            Some(result)
              if call.error.is_none()
                && result["content"]
                  .as_array()
                  .is_some_and(|items| items.iter().all(valid_content)) =>
            {
              FactoryMcpCallStatus::Completed
            }
            _ => FactoryMcpCallStatus::Malformed,
          };
        }
      }
      _ => return Err(invalid()),
    }
  }
  Ok(calls)
}

fn valid_content(value: &Value) -> bool {
  match value["type"].as_str() {
    Some("text") => value["text"].is_string(),
    Some("image" | "audio") => value["data"].is_string() && value["mimeType"].is_string(),
    Some("resource") => value["resource"].is_object(),
    Some("resource_link") => value["uri"].is_string(),
    _ => false,
  }
}
