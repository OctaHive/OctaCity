use async_trait::async_trait;
use octacity_server_domain::ProjectId;

use crate::{
  CreateProject, DeleteProject, DeleteProjectOutcome, ListProjects, MoveProject, ProjectDetails,
  ProjectMutationOutcome, ProjectPage, RenameProject, StoreError,
};

/// Backend-neutral atomic operations for the hierarchical Project aggregate.
///
/// Mutation methods are transaction boundaries: hierarchy state, replay
/// outcome, audit fact, and outbox record commit together.
#[async_trait]
pub trait ProjectStore: Send + Sync {
  /// Creates one root or nested Project.
  async fn create_project(&self, request: CreateProject) -> Result<ProjectMutationOutcome, StoreError>;

  /// Renames one Project if its optimistic version is current.
  async fn rename_project(&self, request: RenameProject) -> Result<ProjectMutationOutcome, StoreError>;

  /// Moves one Project while preserving acyclic ancestry.
  async fn move_project(&self, request: MoveProject) -> Result<ProjectMutationOutcome, StoreError>;

  /// Reads one Project and its root-to-parent ancestry.
  async fn project(&self, project_id: ProjectId) -> Result<ProjectDetails, StoreError>;

  /// Lists one bounded deterministic page of direct children.
  async fn list_projects(&self, request: ListProjects) -> Result<ProjectPage, StoreError>;

  /// Deletes a Project only when no child or domain resource references it.
  async fn delete_project(&self, request: DeleteProject) -> Result<DeleteProjectOutcome, StoreError>;
}
