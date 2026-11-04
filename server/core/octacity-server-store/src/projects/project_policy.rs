use octacity_server_domain::{ProjectId, ProjectPolicyVersion};
use serde_json::Value;

/// One immutable Project-policy document loaded without coupling persistence to
/// the application policy model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectPolicyDocument {
  /// Project owning this policy version.
  pub project_id: ProjectId,
  /// Direct parent recorded for root-to-leaf validation.
  pub parent_id: Option<ProjectId>,
  /// Exact immutable policy version.
  pub version: ProjectPolicyVersion,
  /// Strict application-owned policy document.
  pub policy: Value,
}
