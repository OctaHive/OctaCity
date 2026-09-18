use serde::{Deserialize, Serialize};

/// Request body for creating one root or nested Project.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateProjectRequest {
  /// Optional direct parent identity; absence creates a root Project.
  pub parent_id: Option<String>,
  /// Sibling-unique operator-facing name.
  pub name: String,
}

/// Request body for renaming one Project.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RenameProjectRequest {
  /// New sibling-unique operator-facing name.
  pub name: String,
}

/// Request body for moving one Project in the hierarchy.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MoveProjectRequest {
  /// New direct parent identity; absence moves the Project to the root.
  pub parent_id: Option<String>,
}

/// REST representation of one current Project resource.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectResource {
  /// Stable opaque Project identity.
  pub id: String,
  /// Optional direct parent identity.
  pub parent_id: Option<String>,
  /// Current sibling-unique display name.
  pub name: String,
  /// Positive optimistic resource version.
  pub version: u64,
  /// Authoritative creation time as Unix milliseconds.
  pub created_at_unix_ms: i64,
  /// Latest accepted mutation time as Unix milliseconds.
  pub updated_at_unix_ms: i64,
}

/// One Project together with stable root-to-parent ancestry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectDetails {
  /// Requested current Project.
  pub project: ProjectResource,
  /// Ancestors ordered from the root to the direct parent.
  pub ancestors: Vec<ProjectResource>,
}

/// Result of deleting one Project.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteProjectResponse {
  /// Whether this request applied or replayed the mutation.
  pub disposition: super::MutationDisposition,
  /// Stable identity of the deleted Project.
  pub project_id: String,
}
