use std::collections::BTreeSet;

use octacity_server_domain::{
  AgentId, ArtifactId, AuditFactId, BuildConfigurationId, BuildId, CacheSessionId, JobId, PipelineId, PoolId,
  ProjectId, RepositoryId, TriggerId,
};
use octacity_server_factory::{FactoryConfigurationId, FactoryRunId};
use thiserror::Error;

/// Maximum identities accepted by one restricted management read scope.
pub const MAX_READ_VISIBILITY_IDENTITIES: usize = 256;

/// Stable classification shared by query-specific management read scopes.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ReadVisibilityKind {
  /// Every resource matching the query is visible.
  All,
  /// No matching resource is visible.
  None,
  /// Only an explicitly bounded identity set is visible.
  Restricted,
}

/// Borrowed exhaustive view of one query-specific management read scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadVisibilityView<'a, I> {
  /// Every resource matching the query is visible.
  All,
  /// No matching resource is visible.
  None,
  /// Only the listed identities are visible.
  Restricted(&'a BTreeSet<I>),
}

/// Validation failure for a restricted management read scope.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ReadVisibilityError {
  /// Restricted visibility must contain at least one identity.
  #[error("restricted read visibility must contain at least one identity")]
  Empty,
  /// Restricted visibility exceeded its fixed identity bound.
  #[error("restricted read visibility contains too many identities")]
  TooLarge,
  /// Restricted visibility repeated an identity.
  #[error("restricted read visibility contains a duplicate identity")]
  Duplicate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ReadVisibility<I> {
  All,
  None,
  Restricted(BTreeSet<I>),
}

impl<I: Ord> ReadVisibility<I> {
  fn restricted(identities: impl IntoIterator<Item = I>) -> Result<Self, ReadVisibilityError> {
    let mut restricted = BTreeSet::new();
    for identity in identities {
      if !restricted.insert(identity) {
        return Err(ReadVisibilityError::Duplicate);
      }
      if restricted.len() > MAX_READ_VISIBILITY_IDENTITIES {
        return Err(ReadVisibilityError::TooLarge);
      }
    }
    if restricted.is_empty() {
      return Err(ReadVisibilityError::Empty);
    }
    Ok(Self::Restricted(restricted))
  }

  const fn kind(&self) -> ReadVisibilityKind {
    match self {
      Self::All => ReadVisibilityKind::All,
      Self::None => ReadVisibilityKind::None,
      Self::Restricted(_) => ReadVisibilityKind::Restricted,
    }
  }

  const fn view(&self) -> ReadVisibilityView<'_, I> {
    match self {
      Self::All => ReadVisibilityView::All,
      Self::None => ReadVisibilityView::None,
      Self::Restricted(identities) => ReadVisibilityView::Restricted(identities),
    }
  }
}

macro_rules! query_visibility {
  ($name:ident, $identity:ty, $doc:literal) => {
    #[doc = $doc]
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct $name(ReadVisibility<$identity>);

    impl $name {
      /// Allows every resource matching the query.
      #[must_use]
      pub const fn all() -> Self {
        Self(ReadVisibility::All)
      }

      /// Allows no resource matching the query.
      #[must_use]
      pub const fn none() -> Self {
        Self(ReadVisibility::None)
      }

      /// Allows only the supplied non-empty bounded identity set.
      pub fn restricted(identities: impl IntoIterator<Item = $identity>) -> Result<Self, ReadVisibilityError> {
        ReadVisibility::restricted(identities).map(Self)
      }

      /// Returns the stable visibility classification.
      #[must_use]
      pub const fn kind(&self) -> ReadVisibilityKind {
        self.0.kind()
      }

      /// Borrows the exhaustive visibility value.
      #[must_use]
      pub const fn view(&self) -> ReadVisibilityView<'_, $identity> {
        self.0.view()
      }

      /// Returns whether one identity is visible to this query.
      #[must_use]
      pub fn allows(&self, identity: &$identity) -> bool {
        match self.0.view() {
          ReadVisibilityView::All => true,
          ReadVisibilityView::None => false,
          ReadVisibilityView::Restricted(identities) => identities.contains(identity),
        }
      }
    }
  };
}

query_visibility!(
  ProjectListVisibility,
  ProjectId,
  "Visibility input for a deterministic Project collection page."
);
query_visibility!(
  PipelineListVisibility,
  PipelineId,
  "Visibility input for a deterministic current Pipeline page."
);
query_visibility!(
  RepositoryListVisibility,
  RepositoryId,
  "Visibility input for a deterministic current Repository page."
);
query_visibility!(
  BuildConfigurationListVisibility,
  BuildConfigurationId,
  "Visibility input for a deterministic current Build Configuration page."
);
query_visibility!(
  TriggerDefinitionListVisibility,
  TriggerId,
  "Visibility input for a deterministic current Trigger definition page."
);
query_visibility!(
  BuildListVisibility,
  BuildId,
  "Visibility input for a deterministic Project Build page."
);
query_visibility!(
  FactoryConfigurationListVisibility,
  FactoryConfigurationId,
  "Visibility input for a deterministic current Factory Configuration page."
);
query_visibility!(
  FactoryRunListVisibility,
  FactoryRunId,
  "Visibility input for deterministic Factory Run discovery."
);
query_visibility!(
  AgentPoolListVisibility,
  PoolId,
  "Visibility input for a deterministic current Agent Pool page."
);
query_visibility!(
  AgentListVisibility,
  AgentId,
  "Visibility input for a deterministic enrolled Agent page."
);
query_visibility!(
  InternalTriggerListVisibility,
  TriggerId,
  "Visibility input for a deterministic current internal Trigger page."
);
query_visibility!(
  JobEventReadVisibility,
  JobId,
  "Visibility input for a paged Job event stream read."
);
query_visibility!(
  BuildLogSearchVisibility,
  ProjectId,
  "Visibility input for a paged Build log search."
);
query_visibility!(
  AuditFactListVisibility,
  AuditFactId,
  "Visibility input for a deterministic immutable audit-fact page."
);
query_visibility!(
  ArtifactListVisibility,
  ArtifactId,
  "Visibility input for a bounded Build artifact page."
);
query_visibility!(
  CacheSessionListVisibility,
  CacheSessionId,
  "Visibility input for a bounded Build cache-session page."
);

#[cfg(test)]
mod tests {
  use std::str::FromStr as _;

  use super::*;

  fn project_id(value: u128) -> ProjectId {
    ProjectId::from_str(&uuid::Uuid::from_u128(value).to_string()).unwrap()
  }

  #[test]
  fn restricted_visibility_is_explicit_non_empty_unique_and_bounded() {
    assert_eq!(ProjectListVisibility::restricted([]), Err(ReadVisibilityError::Empty));
    assert_eq!(
      ProjectListVisibility::restricted([project_id(1), project_id(1)]),
      Err(ReadVisibilityError::Duplicate)
    );
    assert_eq!(
      ProjectListVisibility::restricted(
        (0..=MAX_READ_VISIBILITY_IDENTITIES).map(|value| project_id(value as u128 + 1))
      ),
      Err(ReadVisibilityError::TooLarge)
    );
  }

  #[test]
  fn query_specific_visibility_preserves_all_none_and_restricted() {
    let project = project_id(1);
    let restricted = ProjectListVisibility::restricted([project]).unwrap();
    assert_eq!(ProjectListVisibility::all().kind(), ReadVisibilityKind::All);
    assert_eq!(ProjectListVisibility::none().kind(), ReadVisibilityKind::None);
    assert_eq!(restricted.kind(), ReadVisibilityKind::Restricted);
    assert!(matches!(
      restricted.view(),
      ReadVisibilityView::Restricted(identities) if identities.contains(&project)
    ));
  }
}
