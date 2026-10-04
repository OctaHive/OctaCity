use std::{collections::BTreeSet, num::NonZeroU16, sync::Arc};

use async_trait::async_trait;
use octacity_server_domain::{AgentId, BuildId, PoolId, ProjectId};
use octacity_server_store::{
  NormalizedResourceSearchQuery, ResourceSearchPagePosition, ResourceSearchResource, ResourceSearchStore,
  ResourceSearchSummary, SearchResources,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::opaque_cursor;
use crate::{
  ApplicationError, ManagementAction, ManagementAuthorizationMapping, ManagementAuthorizationTarget,
  ManagementQueryUseCase, ManagementResource, ManagementResourceKind, ManagementResourceResult, Query,
};

const RESOURCE_SEARCH_CURSOR_VERSION: u8 = 1;

/// Maximum number of results accepted by one global resource-search page.
pub const MAX_RESOURCE_SEARCH_RESULT_PAGE_SIZE: u16 = octacity_server_store::MAX_RESOURCE_SEARCH_PAGE_SIZE;
/// Maximum UTF-8 bytes returned in one safe result label.
pub const MAX_RESOURCE_SEARCH_RESULT_LABEL_BYTES: usize = octacity_server_store::MAX_RESOURCE_SEARCH_LABEL_BYTES;
/// Maximum UTF-8 bytes returned in one optional non-secret result context.
pub const MAX_RESOURCE_SEARCH_RESULT_CONTEXT_BYTES: usize = octacity_server_store::MAX_RESOURCE_SEARCH_CONTEXT_BYTES;
/// Maximum encoded UTF-8 bytes accepted for one opaque search cursor.
pub const MAX_RESOURCE_SEARCH_CURSOR_BYTES: usize = 1_024;

pub use octacity_server_store::ResourceSearchKind;

/// Validated optional kind filter for global search.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceSearchKinds(BTreeSet<ResourceSearchKind>);

impl ResourceSearchKinds {
  /// Selects every searchable resource kind.
  #[must_use]
  pub fn all() -> Self {
    Self(ResourceSearchKind::ALL.into_iter().collect())
  }

  /// Selects one non-empty duplicate-free set of resource kinds.
  pub fn try_only(kinds: impl IntoIterator<Item = ResourceSearchKind>) -> Result<Self, ResourceSearchInputError> {
    let mut selected = BTreeSet::new();
    for kind in kinds {
      if !selected.insert(kind) {
        return Err(ResourceSearchInputError::InvalidKinds);
      }
    }
    if selected.is_empty() || selected.len() > ResourceSearchKind::ALL.len() {
      return Err(ResourceSearchInputError::InvalidKinds);
    }
    Ok(Self(selected))
  }

  /// Borrows the stable selected kind set.
  #[must_use]
  pub const fn as_set(&self) -> &BTreeSet<ResourceSearchKind> {
    &self.0
  }
}

impl Default for ResourceSearchKinds {
  fn default() -> Self {
    Self::all()
  }
}

/// Invalid global resource-search input.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ResourceSearchInputError {
  /// The normalized query is empty or exceeds its fixed byte bound.
  #[error("resource search query is invalid")]
  InvalidQuery,
  /// The optional kind selection is empty or repeats a kind.
  #[error("resource search kind filter is invalid")]
  InvalidKinds,
  /// The requested page size is zero or exceeds its fixed bound.
  #[error("resource search page size is invalid")]
  InvalidLimit,
  /// The cursor belongs to a different normalized query or kind selection.
  #[error("resource search cursor does not match the query")]
  CursorScopeMismatch,
}

/// Invalid, non-canonical, or unsupported search continuation cursor.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("resource search cursor is invalid")]
pub struct ResourceSearchCursorError;

/// Opaque versioned cursor bound to a normalized query and kind selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceSearchCursor {
  query: String,
  kinds: Vec<ResourceSearchKind>,
  position: ResourceSearchPagePosition,
}

#[derive(Deserialize, Serialize)]
struct ResourceSearchCursorWire {
  version: u8,
  query: String,
  kinds: Vec<ResourceSearchKind>,
  position: ResourceSearchPagePosition,
}

impl ResourceSearchCursor {
  fn new(
    query: &NormalizedResourceSearchQuery,
    kinds: &ResourceSearchKinds,
    position: ResourceSearchPagePosition,
  ) -> Self {
    Self {
      query: query.as_str().to_owned(),
      kinds: kinds.0.iter().copied().collect(),
      position,
    }
  }

  /// Decodes and canonically revalidates one bounded URL-safe cursor.
  pub fn decode(encoded: &str) -> Result<Self, ResourceSearchCursorError> {
    let wire: ResourceSearchCursorWire = opaque_cursor::decode_canonical(encoded, MAX_RESOURCE_SEARCH_CURSOR_BYTES)
      .map_err(|()| ResourceSearchCursorError)?;
    let normalized_query = NormalizedResourceSearchQuery::new(&wire.query).map_err(|_| ResourceSearchCursorError)?;
    let kinds = ResourceSearchKinds::try_only(wire.kinds.clone()).map_err(|_| ResourceSearchCursorError)?;
    if wire.version != RESOURCE_SEARCH_CURSOR_VERSION
      || normalized_query.as_str() != wire.query
      || wire.kinds != kinds.0.iter().copied().collect::<Vec<_>>()
      || wire.position.kind != wire.position.resource.kind()
      || wire.position.normalized_label.is_empty()
      || wire.position.normalized_label.len() > octacity_server_store::MAX_RESOURCE_SEARCH_LABEL_BYTES
      || octacity_server_store::normalize_resource_search_text(&wire.position.normalized_label)
        != wire.position.normalized_label
    {
      return Err(ResourceSearchCursorError);
    }
    Ok(Self {
      query: wire.query,
      kinds: wire.kinds,
      position: wire.position,
    })
  }

  /// Encodes this cursor as canonical URL-safe opaque text.
  #[must_use]
  pub fn encode(&self) -> String {
    opaque_cursor::encode(&ResourceSearchCursorWire {
      version: RESOURCE_SEARCH_CURSOR_VERSION,
      query: self.query.clone(),
      kinds: self.kinds.clone(),
      position: self.position.clone(),
    })
  }

  fn matches(&self, query: &NormalizedResourceSearchQuery, kinds: &ResourceSearchKinds) -> bool {
    self.query == query.as_str() && self.kinds.iter().copied().eq(kinds.0.iter().copied())
  }
}

/// Searches visible Projects, Builds, Agents, and Agent Pools.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchResourcesQuery {
  query: NormalizedResourceSearchQuery,
  kinds: ResourceSearchKinds,
  after: Option<ResourceSearchCursor>,
  limit: NonZeroU16,
}

impl SearchResourcesQuery {
  /// Normalizes and validates one bounded search request.
  pub fn try_new(
    query: &str,
    kinds: ResourceSearchKinds,
    after: Option<ResourceSearchCursor>,
    limit: u16,
  ) -> Result<Self, ResourceSearchInputError> {
    let query = NormalizedResourceSearchQuery::new(query).map_err(|_| ResourceSearchInputError::InvalidQuery)?;
    let limit = NonZeroU16::new(limit)
      .filter(|limit| limit.get() <= MAX_RESOURCE_SEARCH_RESULT_PAGE_SIZE)
      .ok_or(ResourceSearchInputError::InvalidLimit)?;
    if after.as_ref().is_some_and(|cursor| !cursor.matches(&query, &kinds)) {
      return Err(ResourceSearchInputError::CursorScopeMismatch);
    }
    Ok(Self {
      query,
      kinds,
      after,
      limit,
    })
  }

  /// Returns the canonical normalized query.
  #[must_use]
  pub fn normalized_query(&self) -> &str {
    self.query.as_str()
  }

  /// Returns the selected resource kinds.
  #[must_use]
  pub const fn kinds(&self) -> &ResourceSearchKinds {
    &self.kinds
  }

  /// Returns the validated exclusive continuation cursor, when supplied.
  #[must_use]
  pub const fn after(&self) -> Option<&ResourceSearchCursor> {
    self.after.as_ref()
  }

  /// Returns the positive bounded page size.
  #[must_use]
  pub const fn limit(&self) -> u16 {
    self.limit.get()
  }
}

impl Query for SearchResourcesQuery {
  type Outcome = ResourceSearchPageProjection;
}

impl ManagementAuthorizationTarget for SearchResourcesQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::collection(ManagementAction::Search, ManagementResourceKind::ControlPlane);

  fn management_resource(&self) -> ManagementResourceResult {
    ManagementResource::collection(ManagementResourceKind::ControlPlane)
  }
}

/// Typed stable identity returned by global search.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum ResourceSearchIdentityProjection {
  /// Project identity.
  Project(ProjectId),
  /// Build identity.
  Build(BuildId),
  /// Agent identity.
  Agent(AgentId),
  /// Agent Pool identity.
  AgentPool(PoolId),
}

/// Safe bounded global-search result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResourceSearchSummaryProjection {
  /// Typed stable resource identity.
  pub resource: ResourceSearchIdentityProjection,
  /// Bounded operator-facing label.
  pub label: String,
  /// Optional bounded non-secret context.
  pub context: Option<String>,
}

impl From<ResourceSearchSummary> for ResourceSearchSummaryProjection {
  fn from(summary: ResourceSearchSummary) -> Self {
    let resource = match summary.resource() {
      ResourceSearchResource::Project(identity) => ResourceSearchIdentityProjection::Project(identity),
      ResourceSearchResource::Build(identity) => ResourceSearchIdentityProjection::Build(identity),
      ResourceSearchResource::Agent(identity) => ResourceSearchIdentityProjection::Agent(identity),
      ResourceSearchResource::AgentPool(identity) => ResourceSearchIdentityProjection::AgentPool(identity),
    };
    Self {
      resource,
      label: summary.label().to_owned(),
      context: summary.context().map(str::to_owned),
    }
  }
}

/// One deterministic page of globally ranked resources.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceSearchPageProjection {
  /// Safe visible summaries in stable rank order.
  pub items: Vec<ResourceSearchSummaryProjection>,
  /// Opaque cursor bound to the query and filters.
  pub next_cursor: Option<ResourceSearchCursor>,
}

/// Application handlers for global resource discovery.
pub struct ResourceSearchHandlers<S> {
  store: Arc<S>,
}

impl<S> ResourceSearchHandlers<S> {
  /// Creates handlers backed by one search store.
  #[must_use]
  pub const fn new(store: Arc<S>) -> Self {
    Self { store }
  }
}

#[async_trait]
impl<S> ManagementQueryUseCase<SearchResourcesQuery> for ResourceSearchHandlers<S>
where
  S: ResourceSearchStore + 'static,
{
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &crate::ManagementRequestContext,
    grant: &crate::ManagementAuthorizationGrant,
    query: SearchResourcesQuery,
  ) -> Result<ResourceSearchPageProjection, Self::Error> {
    let visibility = grant
      .visibility_for::<SearchResourcesQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    let page = self
      .store
      .search_resources(SearchResources::new(
        query.query.clone(),
        query.kinds.0.clone(),
        query.after.as_ref().map(|cursor| cursor.position.clone()),
        query.limit(),
        visibility,
      )?)
      .await?;
    Ok(ResourceSearchPageProjection {
      items: page.items.into_iter().map(Into::into).collect(),
      next_cursor: page
        .next_cursor
        .map(|position| ResourceSearchCursor::new(&query.query, &query.kinds, position)),
    })
  }
}
