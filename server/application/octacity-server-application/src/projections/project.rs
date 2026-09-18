use octacity_server_domain::{ProjectId, ProjectName, ProjectVersion, Timestamp};
use octacity_server_store::{Project, ProjectDetails};
use serde::Serialize;

/// Safe application summary of one Project resource.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProjectSummaryProjection {
  /// Stable Project identity.
  pub id: ProjectId,
  /// Direct parent, or `None` for a root Project.
  pub parent_id: Option<ProjectId>,
  /// Mutable sibling-unique display name.
  pub name: ProjectName,
  /// Current optimistic resource version.
  pub version: ProjectVersion,
  /// Authoritative creation time.
  pub created_at: Timestamp,
  /// Time of the latest accepted mutation.
  pub updated_at: Timestamp,
}

impl From<Project> for ProjectSummaryProjection {
  fn from(project: Project) -> Self {
    Self {
      id: project.id,
      parent_id: project.parent_id,
      name: project.name,
      version: project.version,
      created_at: project.created_at,
      updated_at: project.updated_at,
    }
  }
}

/// One Project together with its stable root-to-parent ancestry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProjectProjection {
  /// Requested Project.
  pub project: ProjectSummaryProjection,
  /// Ancestors ordered from the root to the direct parent.
  pub ancestors: Vec<ProjectSummaryProjection>,
}

impl From<ProjectDetails> for ProjectProjection {
  fn from(details: ProjectDetails) -> Self {
    Self {
      project: details.project.into(),
      ancestors: details.ancestors.into_iter().map(Into::into).collect(),
    }
  }
}
