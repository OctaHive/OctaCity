use std::num::NonZeroU16;

use octacity_server_domain::{
  BuildConfigurationId, BuildConfigurationName, BuildConfigurationVersion, PipelineId, PipelineName, PipelineVersion,
  ProjectId, RepositoryId, RepositoryName, RepositoryVersion, Timestamp, TriggerId, TriggerVersion,
};

use crate::{
  BuildConfigurationListVisibility, PipelineListVisibility, RepositoryListVisibility, StoreError, StoreInputError,
  StoreOperation, TriggerDefinitionListVisibility,
};

/// Maximum number of current definitions returned by one Project-scoped page.
pub const MAX_CURRENT_DEFINITION_PAGE_SIZE: u16 = 200;

macro_rules! definition_list_request {
  ($name:ident, $identity:ty, $visibility:ty, $operation:expr, $doc:literal) => {
    #[doc = $doc]
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct $name {
      project_id: ProjectId,
      after: Option<$identity>,
      limit: NonZeroU16,
      visibility: $visibility,
    }

    impl $name {
      /// Constructs and validates one Project-scoped current-definition page.
      pub fn new(
        project_id: ProjectId,
        after: Option<$identity>,
        limit: u16,
        visibility: $visibility,
      ) -> Result<Self, StoreError> {
        let limit = NonZeroU16::new(limit)
          .filter(|limit| limit.get() <= MAX_CURRENT_DEFINITION_PAGE_SIZE)
          .ok_or(StoreError::invalid(
            $operation,
            StoreInputError::InvalidCurrentDefinitionPageSize,
          ))?;
        Ok(Self {
          project_id,
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

      /// Returns the exclusive stable-identity cursor.
      #[must_use]
      pub const fn after(&self) -> Option<$identity> {
        self.after
      }

      /// Returns the validated positive page size.
      #[must_use]
      pub const fn limit(&self) -> NonZeroU16 {
        self.limit
      }

      /// Returns the authorization-derived resource visibility.
      #[must_use]
      pub const fn visibility(&self) -> &$visibility {
        &self.visibility
      }
    }
  };
}

definition_list_request!(
  ListProjectPipelines,
  PipelineId,
  PipelineListVisibility,
  StoreOperation::ListProjectPipelines,
  "Bounded current-Pipeline query scoped to one Project."
);
definition_list_request!(
  ListProjectRepositories,
  RepositoryId,
  RepositoryListVisibility,
  StoreOperation::ListProjectRepositories,
  "Bounded current-Repository query scoped to one Project."
);
definition_list_request!(
  ListProjectBuildConfigurations,
  BuildConfigurationId,
  BuildConfigurationListVisibility,
  StoreOperation::ListProjectBuildConfigurations,
  "Bounded current-Build-Configuration query scoped to one Project."
);
definition_list_request!(
  ListProjectTriggerDefinitions,
  TriggerId,
  TriggerDefinitionListVisibility,
  StoreOperation::ListProjectTriggerDefinitions,
  "Bounded current-Trigger-definition query scoped to one Project."
);

/// Safe summary of one current Pipeline version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentPipelineSummary {
  /// Stable Pipeline identity.
  pub id: PipelineId,
  /// Owning Project identity.
  pub project_id: ProjectId,
  /// Stable Project-local name.
  pub name: PipelineName,
  /// Current immutable version.
  pub version: PipelineVersion,
  /// Authoritative publication time of the current version.
  pub published_at: Timestamp,
}

/// One deterministic page of current Pipeline summaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentPipelinePage {
  /// Visible summaries ordered by stable Pipeline identity.
  pub items: Vec<CurrentPipelineSummary>,
  /// Exclusive cursor for the following page, when one exists.
  pub next_cursor: Option<PipelineId>,
}

/// Safe summary of one current Repository version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentRepositorySummary {
  /// Stable Repository identity.
  pub id: RepositoryId,
  /// Owning Project identity.
  pub project_id: ProjectId,
  /// Stable Project-local name.
  pub name: RepositoryName,
  /// Current immutable version.
  pub version: RepositoryVersion,
  /// Authoritative publication time of the current version.
  pub published_at: Timestamp,
}

/// One deterministic page of current Repository summaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentRepositoryPage {
  /// Visible summaries ordered by stable Repository identity.
  pub items: Vec<CurrentRepositorySummary>,
  /// Exclusive cursor for the following page, when one exists.
  pub next_cursor: Option<RepositoryId>,
}

/// Safe summary of one current Build Configuration version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentBuildConfigurationSummary {
  /// Stable Build Configuration identity.
  pub id: BuildConfigurationId,
  /// Owning Project identity.
  pub project_id: ProjectId,
  /// Stable Project-local name.
  pub name: BuildConfigurationName,
  /// Current immutable version.
  pub version: BuildConfigurationVersion,
  /// Whether the current version accepts new Trigger occurrences.
  pub enabled: bool,
  /// Authoritative publication time of the current version.
  pub published_at: Timestamp,
}

/// One deterministic page of current Build Configuration summaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentBuildConfigurationPage {
  /// Visible summaries ordered by stable Build Configuration identity.
  pub items: Vec<CurrentBuildConfigurationSummary>,
  /// Exclusive cursor for the following page, when one exists.
  pub next_cursor: Option<BuildConfigurationId>,
}

/// Closed credential-free Trigger kinds exposed by definition discovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CurrentTriggerDefinitionKind {
  /// Trusted-network manual Trigger definition.
  Manual,
  /// Durable scheduled Trigger definition.
  Scheduled,
  /// Server-generated internal Trigger definition.
  Internal,
}

/// Safe credential-free summary of one current Trigger definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentTriggerDefinitionSummary {
  /// Stable Trigger identity.
  pub id: TriggerId,
  /// Owning Project identity derived from the selected Build Configuration.
  pub project_id: ProjectId,
  /// Exact Build Configuration selected by this Trigger.
  pub configuration_id: BuildConfigurationId,
  /// Exact Build Configuration version selected by this Trigger.
  pub configuration_version: BuildConfigurationVersion,
  /// Current immutable Trigger version.
  pub version: TriggerVersion,
  /// Closed operator-facing Trigger origin.
  pub kind: CurrentTriggerDefinitionKind,
  /// Whether new occurrences may be accepted.
  pub enabled: bool,
  /// Authoritative publication time of the current version.
  pub published_at: Timestamp,
}

/// One deterministic page of current Trigger-definition summaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentTriggerDefinitionPage {
  /// Visible credential-free summaries ordered by stable Trigger identity.
  pub items: Vec<CurrentTriggerDefinitionSummary>,
  /// Exclusive cursor for the following page, when one exists.
  pub next_cursor: Option<TriggerId>,
}

#[cfg(any(test, feature = "test-support"))]
pub(crate) fn finish_current_definition_page<T, I: Copy>(
  items: &mut Vec<T>,
  limit: NonZeroU16,
  identity: impl Fn(&T) -> I,
) -> Option<I> {
  let limit = usize::from(limit.get());
  let has_more = items.len() > limit;
  items.truncate(limit);
  has_more.then(|| identity(items.last().expect("a non-zero full page has a last item")))
}
