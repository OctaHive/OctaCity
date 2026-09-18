use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_domain::{ProjectId, ProjectName, ProjectVersion, Timestamp};
use octacity_server_store::{
  CreateProject, DeleteProject, IdempotencyKey, ListProjects, MoveProject, ProjectStore, RenameProject,
};
use serde::{Deserialize, Serialize};

use crate::{
  ApplicationError, Command, CommandHandler, CommandTransaction, MutationDisposition, ProjectProjection,
  ProjectSummaryProjection, Query, QueryHandler,
};

/// Creates one root or nested Project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateProjectCommand {
  /// Stable Project identity selected once by the application caller.
  pub id: ProjectId,
  /// Optional direct parent.
  pub parent_id: Option<ProjectId>,
  /// Sibling-unique Project name.
  pub name: ProjectName,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative creation time.
  pub created_at: Timestamp,
}

/// Renames one Project using an optimistic version precondition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenameProjectCommand {
  /// Project to rename.
  pub id: ProjectId,
  /// Version that must still be current.
  pub expected_version: ProjectVersion,
  /// New sibling-unique name.
  pub name: ProjectName,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative mutation time.
  pub renamed_at: Timestamp,
}

/// Moves one Project using an optimistic version precondition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MoveProjectCommand {
  /// Project to move.
  pub id: ProjectId,
  /// Version that must still be current.
  pub expected_version: ProjectVersion,
  /// New direct parent, or `None` for a root Project.
  pub parent_id: Option<ProjectId>,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative mutation time.
  pub moved_at: Timestamp,
}

/// Deletes one unreferenced Project using an optimistic version precondition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteProjectCommand {
  /// Project to delete.
  pub id: ProjectId,
  /// Version that must still be current.
  pub expected_version: ProjectVersion,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative mutation time.
  pub deleted_at: Timestamp,
}

/// Result shared by Project create, rename, and move commands.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectCommandOutcome {
  /// Whether the mutation was newly applied or exactly replayed.
  pub disposition: MutationDisposition,
  /// Safe state committed by the original command.
  pub project: ProjectSummaryProjection,
}

/// Result of deleting one Project.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeleteProjectCommandOutcome {
  /// Whether the mutation was newly applied or exactly replayed.
  pub disposition: MutationDisposition,
  /// Stable identity of the deleted Project.
  pub project_id: ProjectId,
}

macro_rules! command_outcome {
  ($command:ty, $outcome:ty) => {
    impl Command for $command {
      type Outcome = $outcome;
    }
  };
}

command_outcome!(CreateProjectCommand, ProjectCommandOutcome);
command_outcome!(RenameProjectCommand, ProjectCommandOutcome);
command_outcome!(MoveProjectCommand, ProjectCommandOutcome);
command_outcome!(DeleteProjectCommand, DeleteProjectCommandOutcome);

/// Reads one Project together with its root-to-parent ancestry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetProjectQuery {
  /// Project to read.
  pub project_id: ProjectId,
}

impl Query for GetProjectQuery {
  type Outcome = ProjectProjection;
}

/// Reads one bounded deterministic page of direct child Projects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ListProjectsQuery {
  /// Parent whose children are requested, or `None` for roots.
  pub parent_id: Option<ProjectId>,
  /// Exclusive stable-identity cursor.
  pub after: Option<ProjectId>,
  /// Positive page size accepted by the application boundary.
  pub limit: u16,
}

impl Query for ListProjectsQuery {
  type Outcome = ProjectPageProjection;
}

/// One deterministic bounded page of safe Project summaries.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProjectPageProjection {
  /// Projects ordered by stable identity.
  pub projects: Vec<ProjectSummaryProjection>,
  /// Cursor to pass to the next page, when one exists.
  pub next_cursor: Option<ProjectId>,
}

/// Typed Project command and query handlers backed by one narrow port.
pub struct ProjectHandlers<S> {
  store: Arc<S>,
}

impl<S> ProjectHandlers<S> {
  /// Creates handlers from a backend-neutral Project port.
  pub fn new(store: Arc<S>) -> Self {
    Self { store }
  }
}

#[async_trait]
impl<S> CommandTransaction<CreateProjectCommand> for S
where
  S: ProjectStore + 'static,
{
  type Error = ApplicationError;

  async fn commit_command(&self, command: CreateProjectCommand) -> Result<ProjectCommandOutcome, Self::Error> {
    let outcome = self
      .create_project(CreateProject {
        id: command.id,
        parent_id: command.parent_id,
        name: command.name,
        idempotency_key: command.idempotency_key,
        created_at: command.created_at,
      })
      .await?;
    Ok(ProjectCommandOutcome {
      disposition: outcome.disposition.into(),
      project: outcome.project.into(),
    })
  }
}

#[async_trait]
impl<S> CommandHandler<CreateProjectCommand> for ProjectHandlers<S>
where
  S: ProjectStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_command(&self, command: CreateProjectCommand) -> Result<ProjectCommandOutcome, Self::Error> {
    self.store.commit_command(command).await
  }
}

#[async_trait]
impl<S> CommandTransaction<RenameProjectCommand> for S
where
  S: ProjectStore + 'static,
{
  type Error = ApplicationError;

  async fn commit_command(&self, command: RenameProjectCommand) -> Result<ProjectCommandOutcome, Self::Error> {
    let outcome = self
      .rename_project(RenameProject {
        id: command.id,
        expected_version: command.expected_version,
        name: command.name,
        idempotency_key: command.idempotency_key,
        renamed_at: command.renamed_at,
      })
      .await?;
    Ok(ProjectCommandOutcome {
      disposition: outcome.disposition.into(),
      project: outcome.project.into(),
    })
  }
}

#[async_trait]
impl<S> CommandHandler<RenameProjectCommand> for ProjectHandlers<S>
where
  S: ProjectStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_command(&self, command: RenameProjectCommand) -> Result<ProjectCommandOutcome, Self::Error> {
    self.store.commit_command(command).await
  }
}

#[async_trait]
impl<S> CommandTransaction<MoveProjectCommand> for S
where
  S: ProjectStore + 'static,
{
  type Error = ApplicationError;

  async fn commit_command(&self, command: MoveProjectCommand) -> Result<ProjectCommandOutcome, Self::Error> {
    let outcome = self
      .move_project(MoveProject {
        id: command.id,
        expected_version: command.expected_version,
        parent_id: command.parent_id,
        idempotency_key: command.idempotency_key,
        moved_at: command.moved_at,
      })
      .await?;
    Ok(ProjectCommandOutcome {
      disposition: outcome.disposition.into(),
      project: outcome.project.into(),
    })
  }
}

#[async_trait]
impl<S> CommandHandler<MoveProjectCommand> for ProjectHandlers<S>
where
  S: ProjectStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_command(&self, command: MoveProjectCommand) -> Result<ProjectCommandOutcome, Self::Error> {
    self.store.commit_command(command).await
  }
}

#[async_trait]
impl<S> CommandTransaction<DeleteProjectCommand> for S
where
  S: ProjectStore + 'static,
{
  type Error = ApplicationError;

  async fn commit_command(&self, command: DeleteProjectCommand) -> Result<DeleteProjectCommandOutcome, Self::Error> {
    let outcome = self
      .delete_project(DeleteProject {
        id: command.id,
        expected_version: command.expected_version,
        idempotency_key: command.idempotency_key,
        deleted_at: command.deleted_at,
      })
      .await?;
    Ok(DeleteProjectCommandOutcome {
      disposition: outcome.disposition.into(),
      project_id: outcome.project_id,
    })
  }
}

#[async_trait]
impl<S> CommandHandler<DeleteProjectCommand> for ProjectHandlers<S>
where
  S: ProjectStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_command(&self, command: DeleteProjectCommand) -> Result<DeleteProjectCommandOutcome, Self::Error> {
    self.store.commit_command(command).await
  }
}

#[async_trait]
impl<S> QueryHandler<GetProjectQuery> for ProjectHandlers<S>
where
  S: ProjectStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_query(&self, query: GetProjectQuery) -> Result<ProjectProjection, Self::Error> {
    self
      .store
      .project(query.project_id)
      .await
      .map(Into::into)
      .map_err(Into::into)
  }
}

#[async_trait]
impl<S> QueryHandler<ListProjectsQuery> for ProjectHandlers<S>
where
  S: ProjectStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_query(&self, query: ListProjectsQuery) -> Result<ProjectPageProjection, Self::Error> {
    let page = self
      .store
      .list_projects(ListProjects::new(query.parent_id, query.after, query.limit)?)
      .await?;
    Ok(ProjectPageProjection {
      projects: page.projects.into_iter().map(Into::into).collect(),
      next_cursor: page.next_cursor,
    })
  }
}
