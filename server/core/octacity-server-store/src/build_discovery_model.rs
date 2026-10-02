use std::num::NonZeroU16;

use octacity_server_domain::{
  AttemptId, AttemptNumber, BuildConfigurationId, BuildConfigurationVersion, BuildId, ProjectId, Timestamp,
};
use octacity_server_orchestrator::{AttemptState, BuildState};
use octacity_server_trigger::TriggerCause;

use crate::{BuildListVisibility, StoreError, StoreInputError, StoreOperation};

/// Maximum number of Build summaries returned by one Project-scoped page.
pub const MAX_PROJECT_BUILD_PAGE_SIZE: u16 = 200;

/// Exact optional predicates applied before Build ordering and pagination.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectBuildFilter {
  /// Exact Build Configuration identity, when selected.
  pub configuration_id: Option<BuildConfigurationId>,
  /// Exact current Build state, when selected.
  pub state: Option<BuildState>,
}

/// Composite position in deterministic newest-first Build order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ProjectBuildPagePosition {
  /// Authoritative Build creation time.
  pub created_at: Timestamp,
  /// Stable identity used to break equal-time ties.
  pub build_id: BuildId,
}

/// Bounded Project Build discovery request with authorization-derived visibility.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListProjectBuilds {
  project_id: ProjectId,
  filter: ProjectBuildFilter,
  after: Option<ProjectBuildPagePosition>,
  limit: NonZeroU16,
  visibility: BuildListVisibility,
}

impl ListProjectBuilds {
  /// Constructs and validates one Project-scoped Build page.
  pub fn new(
    project_id: ProjectId,
    filter: ProjectBuildFilter,
    after: Option<ProjectBuildPagePosition>,
    limit: u16,
    visibility: BuildListVisibility,
  ) -> Result<Self, StoreError> {
    let limit = NonZeroU16::new(limit)
      .filter(|limit| limit.get() <= MAX_PROJECT_BUILD_PAGE_SIZE)
      .ok_or(StoreError::invalid(
        StoreOperation::ListProjectBuilds,
        StoreInputError::InvalidProjectBuildPageSize,
      ))?;
    Ok(Self {
      project_id,
      filter,
      after,
      limit,
      visibility,
    })
  }

  /// Returns the Project that owns every result.
  #[must_use]
  pub const fn project_id(&self) -> ProjectId {
    self.project_id
  }

  /// Returns the exact predicates applied before ordering and pagination.
  #[must_use]
  pub const fn filter(&self) -> ProjectBuildFilter {
    self.filter
  }

  /// Returns the exclusive newest-first continuation position.
  #[must_use]
  pub const fn after(&self) -> Option<ProjectBuildPagePosition> {
    self.after
  }

  /// Returns the validated positive page size.
  #[must_use]
  pub const fn limit(&self) -> NonZeroU16 {
    self.limit
  }

  /// Returns the authorization-derived Build visibility.
  #[must_use]
  pub const fn visibility(&self) -> &BuildListVisibility {
    &self.visibility
  }
}

/// Safe current Build and Attempt facts used by Project discovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectBuildSummary {
  /// Stable Build identity.
  pub id: BuildId,
  /// Owning Project identity.
  pub project_id: ProjectId,
  /// Selected Build Configuration identity.
  pub configuration_id: BuildConfigurationId,
  /// Exact immutable Build Configuration version.
  pub configuration_version: BuildConfigurationVersion,
  /// Provider-neutral cause that initiated the Build.
  pub cause: TriggerCause,
  /// Current aggregate Build state.
  pub state: BuildState,
  /// Authoritative Build creation time.
  pub created_at: Timestamp,
  /// Current Attempt identity.
  pub current_attempt_id: AttemptId,
  /// Current positive Attempt number.
  pub current_attempt_number: AttemptNumber,
  /// Current Attempt state.
  pub current_attempt_state: AttemptState,
  /// Terminal transition time when the Build is terminal.
  pub terminal_at: Option<Timestamp>,
}

impl ProjectBuildSummary {
  /// Returns this summary's deterministic newest-first position.
  #[must_use]
  pub const fn page_position(&self) -> ProjectBuildPagePosition {
    ProjectBuildPagePosition {
      created_at: self.created_at,
      build_id: self.id,
    }
  }
}

/// One deterministic newest-first page of visible matching Builds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectBuildPage {
  /// Build summaries ordered by creation time and stable identity descending.
  pub items: Vec<ProjectBuildSummary>,
  /// Exclusive position for the following page, when another item exists.
  pub next_cursor: Option<ProjectBuildPagePosition>,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn project_build_request_enforces_the_shared_page_bound() {
    let project_id = ProjectId::generate();

    assert_eq!(
      ListProjectBuilds::new(
        project_id,
        ProjectBuildFilter::default(),
        None,
        0,
        BuildListVisibility::all(),
      ),
      Err(StoreError::invalid(
        StoreOperation::ListProjectBuilds,
        StoreInputError::InvalidProjectBuildPageSize,
      ))
    );
    assert!(
      ListProjectBuilds::new(
        project_id,
        ProjectBuildFilter::default(),
        None,
        MAX_PROJECT_BUILD_PAGE_SIZE,
        BuildListVisibility::all(),
      )
      .is_ok()
    );
  }
}
