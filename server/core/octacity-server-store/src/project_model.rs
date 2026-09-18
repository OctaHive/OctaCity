use std::num::NonZeroU16;

use octacity_server_domain::{ProjectId, ProjectName, ProjectVersion, Timestamp};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{IdempotencyKey, MutationDisposition, StoreError, StoreInputError, StoreOperation};

/// Maximum number of Projects returned by one direct-children query.
pub const MAX_PROJECT_PAGE_SIZE: u16 = 200;

/// Verifies that a proposed parent ancestry does not contain the moved Project.
///
/// Adapters remain responsible for locking and loading the authoritative
/// ancestry, while this shared Rust rule owns the cycle decision.
pub fn validate_project_ancestry(
  project_id: ProjectId,
  ancestry: impl IntoIterator<Item = ProjectId>,
) -> Result<(), ProjectHierarchyError> {
  if ancestry.into_iter().any(|ancestor_id| ancestor_id == project_id) {
    Err(ProjectHierarchyError::Cycle)
  } else {
    Ok(())
  }
}

/// Invalid mutation of the Project hierarchy.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ProjectHierarchyError {
  /// The proposed parent is the Project itself or one of its descendants.
  #[error("the proposed project parent creates a hierarchy cycle")]
  Cycle,
}

/// Durable Project resource whose identity is independent of its mutable path.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Project {
  /// Stable opaque identity.
  pub id: ProjectId,
  /// Optional direct parent; `None` identifies a root Project.
  pub parent_id: Option<ProjectId>,
  /// Sibling-unique display name.
  pub name: ProjectName,
  /// Optimistic resource version.
  pub version: ProjectVersion,
  /// Authoritative creation time.
  pub created_at: Timestamp,
  /// Time of the latest accepted mutation.
  pub updated_at: Timestamp,
}

/// One Project and its root-to-parent ancestry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectDetails {
  /// Requested Project.
  pub project: Project,
  /// Ancestors ordered from the root to the direct parent.
  pub ancestors: Vec<Project>,
}

/// Complete atomic request to create one Project.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CreateProject {
  /// Stable identity selected once by the application.
  pub id: ProjectId,
  /// Optional direct parent.
  pub parent_id: Option<ProjectId>,
  /// Sibling-unique display name.
  pub name: ProjectName,
  /// Stable replay identity for this command.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative creation time.
  pub created_at: Timestamp,
}

/// Complete atomic request to rename one Project.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RenameProject {
  /// Project to rename.
  pub id: ProjectId,
  /// Version that must still be current.
  pub expected_version: ProjectVersion,
  /// New sibling-unique display name.
  pub name: ProjectName,
  /// Stable replay identity for this command.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative mutation time.
  pub renamed_at: Timestamp,
}

/// Complete atomic request to move one Project.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MoveProject {
  /// Project to move.
  pub id: ProjectId,
  /// Version that must still be current.
  pub expected_version: ProjectVersion,
  /// New direct parent, or `None` to move to the root.
  pub parent_id: Option<ProjectId>,
  /// Stable replay identity for this command.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative mutation time.
  pub moved_at: Timestamp,
}

/// Complete atomic request to delete one unreferenced Project.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DeleteProject {
  /// Project to delete.
  pub id: ProjectId,
  /// Version that must still be current.
  pub expected_version: ProjectVersion,
  /// Stable replay identity for this command.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative mutation time.
  pub deleted_at: Timestamp,
}

/// Result of a create, rename, or move command.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectMutationOutcome {
  /// Whether the command was newly applied or exactly replayed.
  pub disposition: MutationDisposition,
  /// Project state committed by the original command.
  pub project: Project,
}

/// Result of deleting one Project.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeleteProjectOutcome {
  /// Whether the command was newly applied or exactly replayed.
  pub disposition: MutationDisposition,
  /// Stable identity of the deleted Project.
  pub project_id: ProjectId,
}

/// Bounded direct-children query ordered by stable Project identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ListProjects {
  /// Parent whose direct children are requested, or `None` for roots.
  parent_id: Option<ProjectId>,
  /// Exclusive stable-identity cursor from the previous page.
  after: Option<ProjectId>,
  /// Positive page size no greater than [`MAX_PROJECT_PAGE_SIZE`].
  limit: NonZeroU16,
}

impl ListProjects {
  /// Constructs and validates one bounded Project list query.
  pub fn new(parent_id: Option<ProjectId>, after: Option<ProjectId>, limit: u16) -> Result<Self, StoreError> {
    let limit = NonZeroU16::new(limit)
      .filter(|limit| limit.get() <= MAX_PROJECT_PAGE_SIZE)
      .ok_or(StoreError::invalid(
        StoreOperation::ListProjects,
        StoreInputError::InvalidProjectPageSize,
      ))?;
    Ok(Self {
      parent_id,
      after,
      limit,
    })
  }

  /// Returns the parent whose direct children are requested.
  #[must_use]
  pub const fn parent_id(self) -> Option<ProjectId> {
    self.parent_id
  }

  /// Returns the exclusive stable-identity cursor.
  #[must_use]
  pub const fn after(self) -> Option<ProjectId> {
    self.after
  }

  /// Returns the validated positive page size.
  #[must_use]
  pub const fn limit(self) -> NonZeroU16 {
    self.limit
  }
}

/// One deterministic bounded page of direct child Projects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectPage {
  /// Projects ordered by stable identity.
  pub projects: Vec<Project>,
  /// Cursor to pass as `after` for the next page, when one exists.
  pub next_cursor: Option<ProjectId>,
}
