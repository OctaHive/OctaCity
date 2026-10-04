use std::{collections::BTreeSet, num::NonZeroU16};

use octacity_server_domain::{AgentId, BuildId, PoolId, ProjectId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{StoreError, StoreInputError, StoreOperation};

/// Maximum UTF-8 bytes accepted in one normalized global-search query.
pub const MAX_RESOURCE_SEARCH_QUERY_BYTES: usize = 128;
/// Maximum UTF-8 bytes returned in one safe result label.
pub const MAX_RESOURCE_SEARCH_LABEL_BYTES: usize = 256;
/// Maximum UTF-8 bytes returned in one optional non-secret context string.
pub const MAX_RESOURCE_SEARCH_CONTEXT_BYTES: usize = 512;
/// Maximum number of results returned by one global-search page.
pub const MAX_RESOURCE_SEARCH_PAGE_SIZE: u16 = 100;

/// Searchable top-level management resource kinds.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceSearchKind {
  /// Hierarchical Project.
  Project,
  /// Immutable Build request and current state.
  Build,
  /// Registered Agent.
  Agent,
  /// Static Agent Pool.
  AgentPool,
}

impl ResourceSearchKind {
  /// Complete stable kind order used by unfiltered searches and rank ties.
  pub const ALL: [Self; 4] = [Self::Project, Self::Build, Self::Agent, Self::AgentPool];
}

/// Typed stable identity of one searchable resource.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum ResourceSearchResource {
  /// Project identity.
  Project(ProjectId),
  /// Build identity.
  Build(BuildId),
  /// Agent identity.
  Agent(AgentId),
  /// Agent Pool identity.
  AgentPool(PoolId),
}

impl ResourceSearchResource {
  /// Returns the resource kind used for filters and deterministic ties.
  #[must_use]
  pub const fn kind(self) -> ResourceSearchKind {
    match self {
      Self::Project(_) => ResourceSearchKind::Project,
      Self::Build(_) => ResourceSearchKind::Build,
      Self::Agent(_) => ResourceSearchKind::Agent,
      Self::AgentPool(_) => ResourceSearchKind::AgentPool,
    }
  }

  /// Returns the canonical stable identity used by exact-identifier matching.
  #[must_use]
  pub fn identity(self) -> String {
    match self {
      Self::Project(identity) => identity.to_string(),
      Self::Build(identity) => identity.to_string(),
      Self::Agent(identity) => identity.to_string(),
      Self::AgentPool(identity) => identity.to_string(),
    }
  }
}

/// Stable match class ordered from strongest to weakest.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceSearchRank {
  /// Query equals the canonical stable identity.
  ExactIdentifier,
  /// A normalized operator-facing name begins with the query.
  NamePrefix,
  /// A normalized operator-facing name contains the query.
  NameContains,
}

/// Safe bounded global-search result independent of full resource details.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceSearchSummary {
  resource: ResourceSearchResource,
  label: String,
  context: Option<String>,
}

impl ResourceSearchSummary {
  /// Constructs one result after enforcing safe text bounds.
  pub fn new(
    resource: ResourceSearchResource,
    label: impl Into<String>,
    context: Option<String>,
  ) -> Result<Self, ResourceSearchValueError> {
    let label = label.into();
    validate_safe_text(&label, MAX_RESOURCE_SEARCH_LABEL_BYTES)?;
    if let Some(context) = &context {
      validate_safe_text(context, MAX_RESOURCE_SEARCH_CONTEXT_BYTES)?;
    }
    Ok(Self {
      resource,
      label,
      context,
    })
  }

  /// Returns the typed stable resource identity.
  #[must_use]
  pub const fn resource(&self) -> ResourceSearchResource {
    self.resource
  }

  /// Returns the bounded operator-facing label.
  #[must_use]
  pub fn label(&self) -> &str {
    &self.label
  }

  /// Returns bounded non-secret context when available.
  #[must_use]
  pub fn context(&self) -> Option<&str> {
    self.context.as_deref()
  }
}

/// Composite ascending position in stable ranked search order.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ResourceSearchPagePosition {
  /// Match strength.
  pub rank: ResourceSearchRank,
  /// Stable resource-kind tie breaker.
  pub kind: ResourceSearchKind,
  /// Normalized display label tie breaker.
  pub normalized_label: String,
  /// Stable identity tie breaker.
  pub resource: ResourceSearchResource,
}

/// Normalized bounded query shared by application and persistence adapters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedResourceSearchQuery(String);

impl NormalizedResourceSearchQuery {
  /// Normalizes ASCII case and whitespace and rejects empty or oversized values.
  pub fn new(value: &str) -> Result<Self, ResourceSearchValueError> {
    let normalized = normalize_resource_search_text(value);
    if normalized.is_empty() {
      return Err(ResourceSearchValueError::EmptyQuery);
    }
    if normalized.len() > MAX_RESOURCE_SEARCH_QUERY_BYTES {
      return Err(ResourceSearchValueError::QueryTooLong);
    }
    Ok(Self(normalized))
  }

  /// Borrows the canonical normalized query.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

/// Bounded validated global-search request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchResources {
  query: NormalizedResourceSearchQuery,
  kinds: BTreeSet<ResourceSearchKind>,
  after: Option<ResourceSearchPagePosition>,
  limit: NonZeroU16,
  visibility: ResourceSearchVisibility,
}

impl SearchResources {
  /// Constructs one search after validating kind and page bounds.
  pub fn new(
    query: NormalizedResourceSearchQuery,
    kinds: BTreeSet<ResourceSearchKind>,
    after: Option<ResourceSearchPagePosition>,
    limit: u16,
    visibility: ResourceSearchVisibility,
  ) -> Result<Self, StoreError> {
    if kinds.is_empty() || kinds.len() > ResourceSearchKind::ALL.len() {
      return Err(StoreError::invalid(
        StoreOperation::SearchResources,
        StoreInputError::InvalidResourceSearchKinds,
      ));
    }
    if after.as_ref().is_some_and(|position| {
      position.kind != position.resource.kind()
        || !kinds.contains(&position.kind)
        || position.normalized_label.is_empty()
        || position.normalized_label.len() > MAX_RESOURCE_SEARCH_LABEL_BYTES
        || normalize_resource_search_text(&position.normalized_label) != position.normalized_label
    }) {
      return Err(StoreError::invalid(
        StoreOperation::SearchResources,
        StoreInputError::InvalidResourceSearchCursor,
      ));
    }
    let limit = NonZeroU16::new(limit)
      .filter(|limit| limit.get() <= MAX_RESOURCE_SEARCH_PAGE_SIZE)
      .ok_or(StoreError::invalid(
        StoreOperation::SearchResources,
        StoreInputError::InvalidResourceSearchPageSize,
      ))?;
    Ok(Self {
      query,
      kinds,
      after,
      limit,
      visibility,
    })
  }

  /// Returns the normalized query.
  #[must_use]
  pub const fn query(&self) -> &NormalizedResourceSearchQuery {
    &self.query
  }

  /// Returns the selected non-empty kind set.
  #[must_use]
  pub const fn kinds(&self) -> &BTreeSet<ResourceSearchKind> {
    &self.kinds
  }

  /// Returns the exclusive ranked continuation position.
  #[must_use]
  pub const fn after(&self) -> Option<&ResourceSearchPagePosition> {
    self.after.as_ref()
  }

  /// Returns the positive bounded page size.
  #[must_use]
  pub const fn limit(&self) -> NonZeroU16 {
    self.limit
  }

  /// Returns authorization-derived mixed-resource visibility.
  #[must_use]
  pub const fn visibility(&self) -> &ResourceSearchVisibility {
    &self.visibility
  }
}

/// One deterministic page of visible ranked resources.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceSearchPage {
  /// Safe summaries in stable rank order.
  pub items: Vec<ResourceSearchSummary>,
  /// Exclusive position for the following page.
  pub next_cursor: Option<ResourceSearchPagePosition>,
}

/// Authorization-derived visibility across every searchable resource kind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResourceSearchVisibility {
  /// Every matching resource is visible.
  All,
  /// No matching resource is visible.
  None,
  /// Only this non-empty bounded mixed-resource identity set is visible.
  Restricted(BTreeSet<ResourceSearchResource>),
}

/// Borrowed exhaustive view of mixed-resource search visibility.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceSearchVisibilityView<'a> {
  /// Every matching resource is visible.
  All,
  /// No matching resource is visible.
  None,
  /// Only the listed typed resources are visible.
  Restricted(&'a BTreeSet<ResourceSearchResource>),
}

impl ResourceSearchVisibility {
  /// Allows every resource matching the query.
  #[must_use]
  pub const fn all() -> Self {
    Self::All
  }

  /// Allows no resource matching the query.
  #[must_use]
  pub const fn none() -> Self {
    Self::None
  }

  /// Allows only the supplied unique, non-empty, bounded resources.
  pub fn restricted(
    resources: impl IntoIterator<Item = ResourceSearchResource>,
  ) -> Result<Self, ResourceSearchVisibilityError> {
    let mut restricted = BTreeSet::new();
    for resource in resources {
      if !restricted.insert(resource) {
        return Err(ResourceSearchVisibilityError::Duplicate);
      }
      if restricted.len() > crate::MAX_READ_VISIBILITY_IDENTITIES {
        return Err(ResourceSearchVisibilityError::TooLarge);
      }
    }
    if restricted.is_empty() {
      return Err(ResourceSearchVisibilityError::Empty);
    }
    Ok(Self::Restricted(restricted))
  }

  /// Returns whether one typed identity is visible.
  #[must_use]
  pub fn allows(&self, resource: &ResourceSearchResource) -> bool {
    match self {
      Self::All => true,
      Self::None => false,
      Self::Restricted(resources) => resources.contains(resource),
    }
  }

  /// Borrows the exhaustive visibility value for adapter translation.
  #[must_use]
  pub const fn view(&self) -> ResourceSearchVisibilityView<'_> {
    match self {
      Self::All => ResourceSearchVisibilityView::All,
      Self::None => ResourceSearchVisibilityView::None,
      Self::Restricted(resources) => ResourceSearchVisibilityView::Restricted(resources),
    }
  }
}

/// Invalid global-search text supplied at a typed boundary.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ResourceSearchValueError {
  /// Query is empty after normalization.
  #[error("resource search query is empty")]
  EmptyQuery,
  /// Query exceeds its UTF-8 byte bound.
  #[error("resource search query is too long")]
  QueryTooLong,
  /// A result label or context is empty, non-canonical, or oversized.
  #[error("resource search summary text is invalid")]
  InvalidSummaryText,
}

/// Invalid restricted mixed-resource visibility.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ResourceSearchVisibilityError {
  /// Restricted visibility was empty.
  #[error("restricted resource search visibility is empty")]
  Empty,
  /// Restricted visibility exceeded its fixed identity bound.
  #[error("restricted resource search visibility is too large")]
  TooLarge,
  /// Restricted visibility repeated an identity.
  #[error("restricted resource search visibility contains a duplicate")]
  Duplicate,
}

/// Applies the deliberately locale-independent search normalization contract.
///
/// ASCII whitespace is trimmed and collapsed, ASCII letters are folded to
/// lowercase, and all other Unicode scalar values are preserved verbatim.
#[must_use]
pub fn normalize_resource_search_text(value: &str) -> String {
  let mut normalized = String::with_capacity(value.len());
  let mut pending_space = false;
  for character in value
    .trim_matches(|character: char| character.is_ascii_whitespace())
    .chars()
  {
    if character.is_ascii_whitespace() {
      pending_space = !normalized.is_empty();
      continue;
    }
    if pending_space {
      normalized.push(' ');
      pending_space = false;
    }
    normalized.push(character.to_ascii_lowercase());
  }
  normalized
}

fn validate_safe_text(value: &str, maximum_bytes: usize) -> Result<(), ResourceSearchValueError> {
  if value.is_empty() || value.len() > maximum_bytes || value.trim() != value || value.chars().any(char::is_control) {
    Err(ResourceSearchValueError::InvalidSummaryText)
  } else {
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn query_normalization_is_bounded_and_locale_independent() {
    assert_eq!(
      NormalizedResourceSearchQuery::new("  MAIN\tProject  ")
        .unwrap()
        .as_str(),
      "main project"
    );
    assert_eq!(
      NormalizedResourceSearchQuery::new(" \n "),
      Err(ResourceSearchValueError::EmptyQuery)
    );
    assert_eq!(
      NormalizedResourceSearchQuery::new(&"a".repeat(MAX_RESOURCE_SEARCH_QUERY_BYTES + 1)),
      Err(ResourceSearchValueError::QueryTooLong)
    );
  }

  #[test]
  fn store_request_revalidates_page_kind_and_cursor_bounds() {
    let query = NormalizedResourceSearchQuery::new("alpha").unwrap();
    assert_eq!(
      SearchResources::new(query.clone(), BTreeSet::new(), None, 1, ResourceSearchVisibility::all()),
      Err(StoreError::invalid(
        StoreOperation::SearchResources,
        StoreInputError::InvalidResourceSearchKinds,
      ))
    );
    assert_eq!(
      SearchResources::new(
        query.clone(),
        BTreeSet::from([ResourceSearchKind::Project]),
        None,
        MAX_RESOURCE_SEARCH_PAGE_SIZE + 1,
        ResourceSearchVisibility::all(),
      ),
      Err(StoreError::invalid(
        StoreOperation::SearchResources,
        StoreInputError::InvalidResourceSearchPageSize,
      ))
    );
    let build = BuildId::generate();
    assert_eq!(
      SearchResources::new(
        query,
        BTreeSet::from([ResourceSearchKind::Project]),
        Some(ResourceSearchPagePosition {
          rank: ResourceSearchRank::NamePrefix,
          kind: ResourceSearchKind::Build,
          normalized_label: "alpha".to_owned(),
          resource: ResourceSearchResource::Build(build),
        }),
        1,
        ResourceSearchVisibility::all(),
      ),
      Err(StoreError::invalid(
        StoreOperation::SearchResources,
        StoreInputError::InvalidResourceSearchCursor,
      ))
    );
  }
}
