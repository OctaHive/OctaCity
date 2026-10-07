use std::num::NonZeroU16;

use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::{
  ExternalWorkIdentity, FactoryConfigurationId, FactoryConfigurationVersion, FactoryKey, FactoryRunId, FactoryRunState,
  FactoryRunVersion, WorkEnvelopeId,
};

use crate::{
  FactoryConfigurationListVisibility, FactoryRunListVisibility, StoreError, StoreInputError, StoreOperation,
};

/// Maximum current Factory Configurations returned by one Project page.
pub const MAX_FACTORY_CONFIGURATION_PAGE_SIZE: u16 = 200;
/// Maximum current Factory Runs returned by one discovery page.
pub const MAX_FACTORY_RUN_PAGE_SIZE: u16 = 200;

/// Composite position in deterministic newest-first Factory Configuration order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct FactoryConfigurationPagePosition {
  /// Authoritative identity creation time.
  pub created_at: Timestamp,
  /// Stable identity used to break equal-time ties.
  pub configuration_id: FactoryConfigurationId,
}

/// Bounded Project-scoped current Factory Configuration discovery request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListProjectFactoryConfigurations {
  project_id: ProjectId,
  after: Option<FactoryConfigurationPagePosition>,
  limit: NonZeroU16,
  visibility: FactoryConfigurationListVisibility,
}

impl ListProjectFactoryConfigurations {
  /// Constructs one validated current Factory Configuration page request.
  pub fn new(
    project_id: ProjectId,
    after: Option<FactoryConfigurationPagePosition>,
    limit: u16,
    visibility: FactoryConfigurationListVisibility,
  ) -> Result<Self, StoreError> {
    let limit = NonZeroU16::new(limit)
      .filter(|limit| limit.get() <= MAX_FACTORY_CONFIGURATION_PAGE_SIZE)
      .ok_or(StoreError::invalid(
        StoreOperation::ListProjectFactoryConfigurations,
        StoreInputError::InvalidFactoryConfigurationPageSize,
      ))?;
    Ok(Self {
      project_id,
      after,
      limit,
      visibility,
    })
  }

  /// Returns the owning Project.
  #[must_use]
  pub const fn project_id(&self) -> ProjectId {
    self.project_id
  }

  /// Returns the exclusive continuation position.
  #[must_use]
  pub const fn after(&self) -> Option<FactoryConfigurationPagePosition> {
    self.after
  }

  /// Returns the validated positive page size.
  #[must_use]
  pub const fn limit(&self) -> NonZeroU16 {
    self.limit
  }

  /// Returns authorization-derived visibility.
  #[must_use]
  pub const fn visibility(&self) -> &FactoryConfigurationListVisibility {
    &self.visibility
  }
}

/// Safe current immutable Factory Configuration facts used by discovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentFactoryConfigurationSummary {
  /// Stable configuration identity.
  pub id: FactoryConfigurationId,
  /// Owning Project identity.
  pub project_id: ProjectId,
  /// Current immutable version.
  pub version: FactoryConfigurationVersion,
  /// Whether new Work may select the current version.
  pub enabled: bool,
  /// Authoritative identity creation time.
  pub created_at: Timestamp,
  /// Authoritative current-version publication time.
  pub published_at: Timestamp,
}

impl CurrentFactoryConfigurationSummary {
  /// Returns this summary's deterministic newest-first position.
  #[must_use]
  pub const fn page_position(&self) -> FactoryConfigurationPagePosition {
    FactoryConfigurationPagePosition {
      created_at: self.created_at,
      configuration_id: self.id,
    }
  }
}

/// One deterministic page of visible current Factory Configurations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentFactoryConfigurationPage {
  /// Matching visible count before cursor pagination.
  pub total: u64,
  /// Current summaries in deterministic newest-first order.
  pub items: Vec<CurrentFactoryConfigurationSummary>,
  /// Exclusive continuation position when another item exists.
  pub next_cursor: Option<FactoryConfigurationPagePosition>,
}

/// Exact optional predicates applied after visibility and before ordering.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FactoryRunFilter {
  /// Exact Factory Configuration identity, when selected.
  pub configuration_id: Option<FactoryConfigurationId>,
  /// Exact Work Source adapter kind, when selected.
  pub source: Option<FactoryKey>,
  /// Exact current lifecycle state, when selected.
  pub state: Option<FactoryRunState>,
  /// Inclusive lower admission-time bound.
  pub admitted_from: Option<Timestamp>,
  /// Exclusive upper admission-time bound.
  pub admitted_before: Option<Timestamp>,
}

/// Composite position in deterministic newest-first Factory Run order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct FactoryRunPagePosition {
  /// Authoritative Work admission time.
  pub admitted_at: Timestamp,
  /// Stable identity used to break equal-time ties.
  pub run_id: FactoryRunId,
}

/// Bounded Project-scoped or global Factory Run discovery request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListFactoryRuns {
  project_id: Option<ProjectId>,
  filter: FactoryRunFilter,
  after: Option<FactoryRunPagePosition>,
  limit: NonZeroU16,
  visibility: FactoryRunListVisibility,
}

impl ListFactoryRuns {
  /// Constructs one validated Factory Run discovery request.
  pub fn new(
    project_id: Option<ProjectId>,
    filter: FactoryRunFilter,
    after: Option<FactoryRunPagePosition>,
    limit: u16,
    visibility: FactoryRunListVisibility,
  ) -> Result<Self, StoreError> {
    let limit = NonZeroU16::new(limit)
      .filter(|limit| limit.get() <= MAX_FACTORY_RUN_PAGE_SIZE)
      .ok_or(StoreError::invalid(
        StoreOperation::ListFactoryRuns,
        StoreInputError::InvalidFactoryRunPageSize,
      ))?;
    if filter
      .admitted_from
      .zip(filter.admitted_before)
      .is_some_and(|(from, before)| from >= before)
    {
      return Err(StoreError::invalid(
        StoreOperation::ListFactoryRuns,
        StoreInputError::InvalidFactoryRunFilter,
      ));
    }
    Ok(Self {
      project_id,
      filter,
      after,
      limit,
      visibility,
    })
  }

  /// Returns an optional Project scope; absence denotes the global collection.
  #[must_use]
  pub const fn project_id(&self) -> Option<ProjectId> {
    self.project_id
  }

  /// Returns exact predicates applied to visible rows.
  #[must_use]
  pub const fn filter(&self) -> &FactoryRunFilter {
    &self.filter
  }

  /// Returns the exclusive continuation position.
  #[must_use]
  pub const fn after(&self) -> Option<FactoryRunPagePosition> {
    self.after
  }

  /// Returns the validated positive page size.
  #[must_use]
  pub const fn limit(&self) -> NonZeroU16 {
    self.limit
  }

  /// Returns authorization-derived visibility.
  #[must_use]
  pub const fn visibility(&self) -> &FactoryRunListVisibility {
    &self.visibility
  }
}

/// Safe current Factory Run and admitted Work facts used by discovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryRunSummary {
  /// Stable Factory Run identity.
  pub id: FactoryRunId,
  /// Owning Project identity.
  pub project_id: ProjectId,
  /// Immutable admitted Work identity.
  pub work_id: WorkEnvelopeId,
  /// Exact Factory Configuration identity.
  pub configuration_id: FactoryConfigurationId,
  /// Exact immutable Factory Configuration version.
  pub configuration_version: FactoryConfigurationVersion,
  /// Provider-neutral Work Source kind.
  pub source: FactoryKey,
  /// Source-scoped external Work identity.
  pub external_identity: ExternalWorkIdentity,
  /// Current durable lifecycle state.
  pub state: FactoryRunState,
  /// Current optimistic aggregate version.
  pub version: FactoryRunVersion,
  /// Authoritative admission time.
  pub admitted_at: Timestamp,
  /// Authoritative last transition time.
  pub updated_at: Timestamp,
}

impl FactoryRunSummary {
  /// Returns this summary's deterministic newest-first position.
  #[must_use]
  pub const fn page_position(&self) -> FactoryRunPagePosition {
    FactoryRunPagePosition {
      admitted_at: self.admitted_at,
      run_id: self.id,
    }
  }
}

/// One deterministic page of visible matching Factory Runs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryRunPage {
  /// Matching visible count before cursor pagination.
  pub total: u64,
  /// Current summaries in deterministic newest-first order.
  pub items: Vec<FactoryRunSummary>,
  /// Exclusive continuation position when another item exists.
  pub next_cursor: Option<FactoryRunPagePosition>,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn factory_discovery_requests_enforce_bounds_and_time_order() {
    let project_id = ProjectId::generate();
    assert!(
      ListProjectFactoryConfigurations::new(project_id, None, 0, FactoryConfigurationListVisibility::all(),).is_err()
    );
    assert!(
      ListFactoryRuns::new(
        None,
        FactoryRunFilter::default(),
        None,
        0,
        FactoryRunListVisibility::all()
      )
      .is_err()
    );
    let error = ListFactoryRuns::new(
      None,
      FactoryRunFilter {
        admitted_from: Some(Timestamp::from_unix_millis(2).unwrap()),
        admitted_before: Some(Timestamp::from_unix_millis(1).unwrap()),
        ..FactoryRunFilter::default()
      },
      None,
      1,
      FactoryRunListVisibility::all(),
    )
    .unwrap_err();
    assert!(matches!(
      error,
      StoreError::InvalidInput {
        source: StoreInputError::InvalidFactoryRunFilter,
        ..
      }
    ));
  }
}
