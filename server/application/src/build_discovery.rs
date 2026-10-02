use std::{num::NonZeroU16, str};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use octacity_server_domain::{
  AttemptId, AttemptNumber, BuildConfigurationId, BuildConfigurationVersion, BuildId, ProjectId, Timestamp,
};
use octacity_server_orchestrator::{AttemptState, BuildState};
use serde::Serialize;
use thiserror::Error;

use crate::{
  ManagementAction, ManagementAuthorizationMapping, ManagementAuthorizationTarget, ManagementResourceKind,
  ManagementResourceResult, Query, TriggerCauseProjection, management_security::owned_collection_resource,
};

const BUILD_PAGE_CURSOR_VERSION: &str = "1";

/// Maximum number of Build summaries accepted by one Project-scoped page.
pub const MAX_BUILD_LIST_PAGE_SIZE: u16 = octacity_server_store::MAX_PROJECT_BUILD_PAGE_SIZE;
/// Maximum encoded UTF-8 bytes accepted for one opaque Build page cursor.
pub const MAX_BUILD_PAGE_CURSOR_BYTES: usize = 128;

/// Invalid Build-page bounds at the application boundary.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum BuildPageInputError {
  /// The requested page size was zero or exceeded the application bound.
  #[error("Build page size is invalid")]
  InvalidLimit,
}

/// Invalid, non-canonical, or unsupported Build continuation cursor.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("Build page cursor is invalid")]
pub struct BuildPageCursorError;

/// Typed optional filters applied before Build ordering and pagination.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BuildListFilter {
  /// Exact Build Configuration identity, when selected.
  pub configuration_id: Option<BuildConfigurationId>,
  /// Exact current Build state, when selected.
  pub state: Option<BuildState>,
}

/// Composite newest-first position carried by an opaque versioned Build cursor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildPageCursor {
  created_at: Timestamp,
  build_id: BuildId,
}

impl BuildPageCursor {
  /// Constructs a cursor from one visible page item's deterministic sort position.
  #[must_use]
  pub const fn new(created_at: Timestamp, build_id: BuildId) -> Self {
    Self { created_at, build_id }
  }

  /// Decodes one bounded canonical versioned cursor.
  pub fn decode(encoded: &str) -> Result<Self, BuildPageCursorError> {
    if encoded.is_empty() || encoded.len() > MAX_BUILD_PAGE_CURSOR_BYTES {
      return Err(BuildPageCursorError);
    }
    let decoded = URL_SAFE_NO_PAD.decode(encoded).map_err(|_| BuildPageCursorError)?;
    if decoded.len() > MAX_BUILD_PAGE_CURSOR_BYTES || URL_SAFE_NO_PAD.encode(&decoded) != encoded {
      return Err(BuildPageCursorError);
    }
    let decoded = str::from_utf8(&decoded).map_err(|_| BuildPageCursorError)?;
    let mut components = decoded.split(':');
    let version = components.next().ok_or(BuildPageCursorError)?;
    let created_at = components.next().ok_or(BuildPageCursorError)?;
    let build_id = components.next().ok_or(BuildPageCursorError)?;
    if version != BUILD_PAGE_CURSOR_VERSION || components.next().is_some() {
      return Err(BuildPageCursorError);
    }
    let cursor = Self {
      created_at: Timestamp::from_unix_millis(created_at.parse().map_err(|_| BuildPageCursorError)?)
        .map_err(|_| BuildPageCursorError)?,
      build_id: build_id.parse().map_err(|_| BuildPageCursorError)?,
    };
    if cursor.encode() != encoded {
      return Err(BuildPageCursorError);
    }
    Ok(cursor)
  }

  /// Encodes this position as a canonical opaque URL-safe cursor.
  #[must_use]
  pub fn encode(self) -> String {
    URL_SAFE_NO_PAD.encode(format!(
      "{BUILD_PAGE_CURSOR_VERSION}:{}:{}",
      self.created_at.unix_millis(),
      self.build_id
    ))
  }

  /// Returns the creation time component used by newest-first ordering.
  #[must_use]
  pub const fn created_at(self) -> Timestamp {
    self.created_at
  }

  /// Returns the stable Build identity used as the deterministic tie-breaker.
  #[must_use]
  pub const fn build_id(self) -> BuildId {
    self.build_id
  }
}

/// Lists visible Builds owned by one Project in deterministic newest-first order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListProjectBuildsQuery {
  project_id: ProjectId,
  filter: BuildListFilter,
  after: Option<BuildPageCursor>,
  limit: NonZeroU16,
}

impl ListProjectBuildsQuery {
  /// Validates and constructs one Project Build discovery query.
  pub fn try_new(
    project_id: ProjectId,
    filter: BuildListFilter,
    after: Option<BuildPageCursor>,
    limit: u16,
  ) -> Result<Self, BuildPageInputError> {
    let limit = NonZeroU16::new(limit)
      .filter(|limit| limit.get() <= MAX_BUILD_LIST_PAGE_SIZE)
      .ok_or(BuildPageInputError::InvalidLimit)?;
    Ok(Self {
      project_id,
      filter,
      after,
      limit,
    })
  }

  /// Returns the Project that owns every matching Build.
  #[must_use]
  pub const fn project_id(&self) -> ProjectId {
    self.project_id
  }

  /// Returns the typed exact-match filters.
  #[must_use]
  pub const fn filter(&self) -> BuildListFilter {
    self.filter
  }

  /// Returns the exclusive newest-first continuation position.
  #[must_use]
  pub const fn after(&self) -> Option<BuildPageCursor> {
    self.after
  }

  /// Returns the positive bounded page size.
  #[must_use]
  pub const fn limit(&self) -> u16 {
    self.limit.get()
  }
}

impl Query for ListProjectBuildsQuery {
  type Outcome = BuildPageProjection;
}

impl ManagementAuthorizationTarget for ListProjectBuildsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::owned_collection(
    ManagementAction::View,
    ManagementResourceKind::Build,
    ManagementResourceKind::Project,
  );

  fn management_resource(&self) -> ManagementResourceResult {
    owned_collection_resource(
      ManagementResourceKind::Build,
      ManagementResourceKind::Project,
      self.project_id,
    )
  }
}

/// Bounded Build data needed by Project discovery without embedding diagnostics.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BuildSummaryProjection {
  /// Stable Build identity.
  pub id: BuildId,
  /// Owning Project identity.
  pub project_id: ProjectId,
  /// Selected Build Configuration identity.
  pub configuration_id: BuildConfigurationId,
  /// Exact immutable Build Configuration version.
  pub configuration_version: BuildConfigurationVersion,
  /// Provider-neutral cause that initiated the Build.
  pub cause: TriggerCauseProjection,
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

/// One deterministic newest-first page of Build summaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildPageProjection {
  /// Visible matching summaries ordered by creation time and Build identity descending.
  pub items: Vec<BuildSummaryProjection>,
  /// Opaque position that continues strictly after the final returned Build.
  pub next_cursor: Option<BuildPageCursor>,
}

impl From<octacity_server_store::ProjectBuildSummary> for BuildSummaryProjection {
  fn from(summary: octacity_server_store::ProjectBuildSummary) -> Self {
    let cause = (&summary.cause).into();
    Self {
      id: summary.id,
      project_id: summary.project_id,
      configuration_id: summary.configuration_id,
      configuration_version: summary.configuration_version,
      cause,
      state: summary.state,
      created_at: summary.created_at,
      current_attempt_id: summary.current_attempt_id,
      current_attempt_number: summary.current_attempt_number,
      current_attempt_state: summary.current_attempt_state,
      terminal_at: summary.terminal_at,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn build_cursor_round_trips_both_newest_first_sort_components() {
    let cursor = BuildPageCursor::new(
      Timestamp::from_unix_millis(1_797_000_000_123).unwrap(),
      BuildId::generate(),
    );

    assert_eq!(BuildPageCursor::decode(&cursor.encode()), Ok(cursor));
  }

  #[test]
  fn build_cursor_rejects_unsupported_malformed_noncanonical_and_oversized_values() {
    let build_id = BuildId::generate();
    let unsupported = URL_SAFE_NO_PAD.encode(format!("2:1:{build_id}"));
    let malformed = URL_SAFE_NO_PAD.encode("1:not-a-time:not-a-build");
    let leading_zero = URL_SAFE_NO_PAD.encode(format!("1:01:{build_id}"));
    let explicit_sign = URL_SAFE_NO_PAD.encode(format!("1:+1:{build_id}"));
    let padded = format!(
      "{}=",
      BuildPageCursor::new(Timestamp::from_unix_millis(1).unwrap(), build_id).encode()
    );
    let oversized = "a".repeat(MAX_BUILD_PAGE_CURSOR_BYTES + 1);

    for value in [
      "",
      "%%%",
      &unsupported,
      &malformed,
      &leading_zero,
      &explicit_sign,
      &padded,
      &oversized,
    ] {
      assert_eq!(BuildPageCursor::decode(value), Err(BuildPageCursorError));
    }
  }

  #[test]
  fn build_page_input_enforces_positive_shared_bound() {
    let project_id = ProjectId::generate();
    let filter = BuildListFilter::default();

    assert_eq!(
      ListProjectBuildsQuery::try_new(project_id, filter, None, 0),
      Err(BuildPageInputError::InvalidLimit)
    );
    assert!(ListProjectBuildsQuery::try_new(project_id, filter, None, MAX_BUILD_LIST_PAGE_SIZE).is_ok());
    assert_eq!(
      ListProjectBuildsQuery::try_new(project_id, filter, None, MAX_BUILD_LIST_PAGE_SIZE + 1),
      Err(BuildPageInputError::InvalidLimit)
    );
  }

  #[test]
  fn build_query_targets_the_project_owned_build_collection() {
    let project_id = ProjectId::generate();
    let query = ListProjectBuildsQuery::try_new(project_id, BuildListFilter::default(), None, 1).unwrap();
    let resource = query.management_resource().unwrap();

    assert_eq!(ListProjectBuildsQuery::AUTHORIZATION.action(), ManagementAction::View);
    assert_eq!(
      ListProjectBuildsQuery::AUTHORIZATION.resource_kind(),
      ManagementResourceKind::Build
    );
    assert_eq!(resource.kind(), ManagementResourceKind::Build);
    assert!(matches!(
      resource.owner(),
      Some((ManagementResourceKind::Project, owner)) if owner.as_str() == project_id.to_string()
    ));
  }
}
