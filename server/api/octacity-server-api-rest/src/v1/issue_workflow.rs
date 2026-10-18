use serde::{Deserialize, Serialize};
/// Closed business-policy document; the application validates its bounded grammar.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueWorkflowDefinitionRequest {
  /// ID/name-only statuses, workflow-level selections and explicit transitions.
  pub definition: serde_json::Value,
}

/// Creates a stable Project-owned workflow independently of its default selection.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateIssueWorkflowRequest {
  /// Caller-generated UUID retained across publication retries.
  pub id: String,
  /// Bounded business policy, never an execution graph.
  pub definition: serde_json::Value,
}

/// Reviewed exact default for new tasks; existing task pins are unaffected.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SelectDefaultIssueWorkflowRequest {
  /// Same-Project workflow UUID.
  pub workflow_id: String,
  /// Positive immutable publication version, never an implicit latest version.
  pub workflow_version: u64,
}
