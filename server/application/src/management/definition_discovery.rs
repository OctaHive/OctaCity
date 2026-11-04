use std::num::NonZeroU16;

use octacity_server_domain::{
  BuildConfigurationId, BuildConfigurationName, BuildConfigurationVersion, PipelineId, PipelineName, PipelineVersion,
  ProjectId, RepositoryId, RepositoryName, RepositoryVersion, Timestamp, TriggerId, TriggerVersion,
};
use serde::Serialize;
use thiserror::Error;

use crate::{
  ManagementAction, ManagementAuthorizationMapping, ManagementAuthorizationTarget, ManagementResourceKind,
  ManagementResourceResult, Query, management_security::owned_collection_resource,
};

/// Maximum number of current definitions accepted by one Project-scoped page.
pub const MAX_CURRENT_DEFINITION_PAGE_SIZE: u16 = octacity_server_store::MAX_CURRENT_DEFINITION_PAGE_SIZE;

/// Invalid current-definition pagination input at the application boundary.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum CurrentDefinitionPageError {
  /// The requested page size was zero or exceeded the application bound.
  #[error("current-definition page size is invalid")]
  InvalidLimit,
}

/// Typed Project ownership, cursor, and bound shared by current-definition queries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentDefinitionPageInput<I> {
  project_id: ProjectId,
  after: Option<I>,
  limit: NonZeroU16,
}

impl<I> CurrentDefinitionPageInput<I> {
  /// Validates and constructs one Project-scoped current-definition page input.
  pub fn try_new(project_id: ProjectId, after: Option<I>, limit: u16) -> Result<Self, CurrentDefinitionPageError> {
    let limit = NonZeroU16::new(limit)
      .filter(|limit| limit.get() <= MAX_CURRENT_DEFINITION_PAGE_SIZE)
      .ok_or(CurrentDefinitionPageError::InvalidLimit)?;
    Ok(Self {
      project_id,
      after,
      limit,
    })
  }

  /// Returns the Project that owns every definition in the requested page.
  #[must_use]
  pub const fn project_id(&self) -> ProjectId {
    self.project_id
  }

  /// Borrows the exclusive stable-identity cursor.
  #[must_use]
  pub const fn after(&self) -> Option<&I> {
    self.after.as_ref()
  }

  /// Returns the positive bounded page size.
  #[must_use]
  pub const fn limit(&self) -> u16 {
    self.limit.get()
  }
}

/// Lists current Pipeline versions owned by one Project in stable identity order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListProjectPipelinesQuery {
  page: CurrentDefinitionPageInput<PipelineId>,
}

impl ListProjectPipelinesQuery {
  /// Validates and constructs one Pipeline discovery query.
  pub fn try_new(
    project_id: ProjectId,
    after: Option<PipelineId>,
    limit: u16,
  ) -> Result<Self, CurrentDefinitionPageError> {
    CurrentDefinitionPageInput::try_new(project_id, after, limit).map(|page| Self { page })
  }

  /// Borrows the typed Project-scoped page input.
  #[must_use]
  pub const fn page(&self) -> &CurrentDefinitionPageInput<PipelineId> {
    &self.page
  }
}

/// Lists current Repository versions owned by one Project in stable identity order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListProjectRepositoriesQuery {
  page: CurrentDefinitionPageInput<RepositoryId>,
}

impl ListProjectRepositoriesQuery {
  /// Validates and constructs one Repository discovery query.
  pub fn try_new(
    project_id: ProjectId,
    after: Option<RepositoryId>,
    limit: u16,
  ) -> Result<Self, CurrentDefinitionPageError> {
    CurrentDefinitionPageInput::try_new(project_id, after, limit).map(|page| Self { page })
  }

  /// Borrows the typed Project-scoped page input.
  #[must_use]
  pub const fn page(&self) -> &CurrentDefinitionPageInput<RepositoryId> {
    &self.page
  }
}

/// Lists current Build Configuration versions owned by one Project in stable identity order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListProjectBuildConfigurationsQuery {
  page: CurrentDefinitionPageInput<BuildConfigurationId>,
}

impl ListProjectBuildConfigurationsQuery {
  /// Validates and constructs one Build Configuration discovery query.
  pub fn try_new(
    project_id: ProjectId,
    after: Option<BuildConfigurationId>,
    limit: u16,
  ) -> Result<Self, CurrentDefinitionPageError> {
    CurrentDefinitionPageInput::try_new(project_id, after, limit).map(|page| Self { page })
  }

  /// Borrows the typed Project-scoped page input.
  #[must_use]
  pub const fn page(&self) -> &CurrentDefinitionPageInput<BuildConfigurationId> {
    &self.page
  }
}

/// Lists current manual, scheduled, and internal Trigger definitions owned by one Project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListProjectTriggerDefinitionsQuery {
  page: CurrentDefinitionPageInput<TriggerId>,
}

impl ListProjectTriggerDefinitionsQuery {
  /// Validates and constructs one Trigger definition discovery query.
  pub fn try_new(
    project_id: ProjectId,
    after: Option<TriggerId>,
    limit: u16,
  ) -> Result<Self, CurrentDefinitionPageError> {
    CurrentDefinitionPageInput::try_new(project_id, after, limit).map(|page| Self { page })
  }

  /// Borrows the typed Project-scoped page input.
  #[must_use]
  pub const fn page(&self) -> &CurrentDefinitionPageInput<TriggerId> {
    &self.page
  }
}

macro_rules! definition_query_contract {
  ($query:ty, $outcome:ty, $kind:expr) => {
    impl Query for $query {
      type Outcome = $outcome;
    }

    impl ManagementAuthorizationTarget for $query {
      const AUTHORIZATION: ManagementAuthorizationMapping = ManagementAuthorizationMapping::owned_collection(
        ManagementAction::View,
        $kind,
        ManagementResourceKind::Project,
      );

      fn management_resource(&self) -> ManagementResourceResult {
        owned_collection_resource($kind, ManagementResourceKind::Project, self.page().project_id())
      }
    }
  };
}

definition_query_contract!(
  ListProjectPipelinesQuery,
  PipelinePageProjection,
  ManagementResourceKind::Pipeline
);
definition_query_contract!(
  ListProjectRepositoriesQuery,
  RepositoryPageProjection,
  ManagementResourceKind::Repository
);
definition_query_contract!(
  ListProjectBuildConfigurationsQuery,
  BuildConfigurationPageProjection,
  ManagementResourceKind::BuildConfiguration
);
definition_query_contract!(
  ListProjectTriggerDefinitionsQuery,
  TriggerDefinitionPageProjection,
  ManagementResourceKind::Trigger
);

/// Safe summary of one current Pipeline version.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PipelineSummaryProjection {
  /// Stable Pipeline identity.
  pub id: PipelineId,
  /// Owning Project identity.
  pub project_id: ProjectId,
  /// Operator-facing Project-local name.
  pub name: PipelineName,
  /// Current immutable version.
  pub version: PipelineVersion,
  /// Authoritative publication time of the current version.
  pub published_at: Timestamp,
}

/// One deterministic page of current Pipeline summaries.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PipelinePageProjection {
  /// Current summaries ordered by stable Pipeline identity.
  pub items: Vec<PipelineSummaryProjection>,
  /// Exclusive stable-identity cursor for the following page.
  pub next_cursor: Option<PipelineId>,
}

/// Safe summary of one current Repository version.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RepositorySummaryProjection {
  /// Stable Repository identity.
  pub id: RepositoryId,
  /// Owning Project identity.
  pub project_id: ProjectId,
  /// Operator-facing Project-local name.
  pub name: RepositoryName,
  /// Current immutable version.
  pub version: RepositoryVersion,
  /// Authoritative publication time of the current version.
  pub published_at: Timestamp,
}

/// One deterministic page of current Repository summaries.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RepositoryPageProjection {
  /// Current summaries ordered by stable Repository identity.
  pub items: Vec<RepositorySummaryProjection>,
  /// Exclusive stable-identity cursor for the following page.
  pub next_cursor: Option<RepositoryId>,
}

/// Safe summary of one current Build Configuration version.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BuildConfigurationSummaryProjection {
  /// Stable Build Configuration identity.
  pub id: BuildConfigurationId,
  /// Owning Project identity.
  pub project_id: ProjectId,
  /// Operator-facing Project-local name.
  pub name: BuildConfigurationName,
  /// Current immutable version.
  pub version: BuildConfigurationVersion,
  /// Whether the current version accepts new Trigger occurrences.
  pub enabled: bool,
  /// Authoritative publication time of the current version.
  pub published_at: Timestamp,
}

/// One deterministic page of current Build Configuration summaries.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BuildConfigurationPageProjection {
  /// Current summaries ordered by stable Build Configuration identity.
  pub items: Vec<BuildConfigurationSummaryProjection>,
  /// Exclusive stable-identity cursor for the following page.
  pub next_cursor: Option<BuildConfigurationId>,
}

/// Closed Trigger kinds exposed by Project definition discovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerDefinitionKindProjection {
  /// Trusted-network manual Trigger definition.
  Manual,
  /// Durable scheduled Trigger definition.
  Scheduled,
  /// Server-generated internal Trigger definition.
  Internal,
}

/// Safe credential-free summary of one current Trigger definition.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TriggerDefinitionSummaryProjection {
  /// Stable Trigger identity.
  pub id: TriggerId,
  /// Owning Project identity derived from the selected Build Configuration.
  pub project_id: ProjectId,
  /// Exact Build Configuration selected by this Trigger.
  pub configuration_id: BuildConfigurationId,
  /// Exact immutable Build Configuration version selected by this Trigger.
  pub configuration_version: BuildConfigurationVersion,
  /// Current immutable Trigger version.
  pub version: TriggerVersion,
  /// Closed operator-facing Trigger origin.
  pub kind: TriggerDefinitionKindProjection,
  /// Whether new occurrences may be accepted.
  pub enabled: bool,
  /// Authoritative publication time of the current version.
  pub published_at: Timestamp,
}

/// One deterministic page of current Trigger definition summaries.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TriggerDefinitionPageProjection {
  /// Credential-free summaries ordered by stable Trigger identity.
  pub items: Vec<TriggerDefinitionSummaryProjection>,
  /// Exclusive stable-identity cursor for the following page.
  pub next_cursor: Option<TriggerId>,
}

impl From<octacity_server_store::CurrentPipelineSummary> for PipelineSummaryProjection {
  fn from(summary: octacity_server_store::CurrentPipelineSummary) -> Self {
    Self {
      id: summary.id,
      project_id: summary.project_id,
      name: summary.name,
      version: summary.version,
      published_at: summary.published_at,
    }
  }
}

impl From<octacity_server_store::CurrentRepositorySummary> for RepositorySummaryProjection {
  fn from(summary: octacity_server_store::CurrentRepositorySummary) -> Self {
    Self {
      id: summary.id,
      project_id: summary.project_id,
      name: summary.name,
      version: summary.version,
      published_at: summary.published_at,
    }
  }
}

impl From<octacity_server_store::CurrentBuildConfigurationSummary> for BuildConfigurationSummaryProjection {
  fn from(summary: octacity_server_store::CurrentBuildConfigurationSummary) -> Self {
    Self {
      id: summary.id,
      project_id: summary.project_id,
      name: summary.name,
      version: summary.version,
      enabled: summary.enabled,
      published_at: summary.published_at,
    }
  }
}

impl From<octacity_server_store::CurrentTriggerDefinitionKind> for TriggerDefinitionKindProjection {
  fn from(kind: octacity_server_store::CurrentTriggerDefinitionKind) -> Self {
    match kind {
      octacity_server_store::CurrentTriggerDefinitionKind::Manual => Self::Manual,
      octacity_server_store::CurrentTriggerDefinitionKind::Scheduled => Self::Scheduled,
      octacity_server_store::CurrentTriggerDefinitionKind::Internal => Self::Internal,
    }
  }
}

impl From<octacity_server_store::CurrentTriggerDefinitionSummary> for TriggerDefinitionSummaryProjection {
  fn from(summary: octacity_server_store::CurrentTriggerDefinitionSummary) -> Self {
    Self {
      id: summary.id,
      project_id: summary.project_id,
      configuration_id: summary.configuration_id,
      configuration_version: summary.configuration_version,
      version: summary.version,
      kind: summary.kind.into(),
      enabled: summary.enabled,
      published_at: summary.published_at,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn current_definition_page_input_enforces_positive_shared_bound() {
    let project_id = ProjectId::generate();

    assert_eq!(
      CurrentDefinitionPageInput::<PipelineId>::try_new(project_id, None, 0),
      Err(CurrentDefinitionPageError::InvalidLimit)
    );
    assert!(
      CurrentDefinitionPageInput::<PipelineId>::try_new(project_id, None, MAX_CURRENT_DEFINITION_PAGE_SIZE).is_ok()
    );
    assert_eq!(
      CurrentDefinitionPageInput::<PipelineId>::try_new(project_id, None, MAX_CURRENT_DEFINITION_PAGE_SIZE + 1,),
      Err(CurrentDefinitionPageError::InvalidLimit)
    );
  }

  #[test]
  fn definition_query_constructors_preserve_typed_project_cursor_and_bound() {
    let project_id = ProjectId::generate();
    let pipeline_id = PipelineId::generate();
    let query = ListProjectPipelinesQuery::try_new(project_id, Some(pipeline_id), 17).unwrap();

    assert_eq!(query.page().project_id(), project_id);
    assert_eq!(query.page().after(), Some(&pipeline_id));
    assert_eq!(query.page().limit(), 17);
  }

  #[test]
  fn definition_queries_target_their_project_owned_collections() {
    let project_id = ProjectId::generate();
    let cases = [
      target(
        &ListProjectPipelinesQuery::try_new(project_id, None, 1).unwrap(),
        ManagementResourceKind::Pipeline,
        project_id,
      ),
      target(
        &ListProjectRepositoriesQuery::try_new(project_id, None, 1).unwrap(),
        ManagementResourceKind::Repository,
        project_id,
      ),
      target(
        &ListProjectBuildConfigurationsQuery::try_new(project_id, None, 1).unwrap(),
        ManagementResourceKind::BuildConfiguration,
        project_id,
      ),
      target(
        &ListProjectTriggerDefinitionsQuery::try_new(project_id, None, 1).unwrap(),
        ManagementResourceKind::Trigger,
        project_id,
      ),
    ];

    assert!(cases.into_iter().all(|valid| valid));
  }

  fn target<Q>(query: &Q, kind: ManagementResourceKind, project_id: ProjectId) -> bool
  where
    Q: ManagementAuthorizationTarget,
  {
    let resource = query.management_resource().unwrap();
    Q::AUTHORIZATION.action() == ManagementAction::View
      && Q::AUTHORIZATION.resource_kind() == kind
      && resource.kind() == kind
      && matches!(
        resource.owner(),
        Some((ManagementResourceKind::Project, owner)) if owner.as_str() == project_id.to_string()
      )
  }
}
