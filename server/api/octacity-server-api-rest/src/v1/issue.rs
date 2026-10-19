//! Closed task command documents; identity, actor and execution authority are server-owned.
use serde::{Deserialize, Serialize};
/// Reviewed direct organizational move; the Issue revision comes from If-Match.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionIssueRequest {
  /// Exact pinned business workflow UUID.
  pub workflow_id: String,
  /// Positive immutable workflow publication.
  pub workflow_version: u64,
  /// Exact observed starting status UUID.
  pub expected_status_id: String,
  /// One declared direct edge UUID.
  pub transition_id: String,
  /// Optional organizational explanation, never execution evidence.
  pub reason: Option<String>,
}

/// Explicit reviewed adoption; current Issue revision is supplied by If-Match.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdoptIssueWorkflowRequest {
  /// Exact original policy UUID.
  pub expected_workflow_id: String,
  /// Exact original policy publication.
  pub expected_workflow_version: u64,
  /// Exact observed current status UUID.
  pub expected_status_id: String,
  /// Same-Project reviewed target policy UUID.
  pub target_workflow_id: String,
  /// Exact target publication, never latest.
  pub target_workflow_version: u64,
  /// Bounded old-to-new status correspondence including current status.
  pub status_correspondence: std::collections::BTreeMap<String, String>,
  /// Current reviewed ownership/mapping precondition; cannot install authority.
  pub expected_field_policy: octacity_server_application::IssueFieldPolicy,
}
