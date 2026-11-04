use std::{collections::BTreeMap, sync::Mutex, sync::MutexGuard};

use async_trait::async_trait;
use thiserror::Error;

use crate::pagination::finish_bounded_page;
use crate::{
  ResourceSearchPage, ResourceSearchPagePosition, ResourceSearchRank, ResourceSearchResource, ResourceSearchStore,
  ResourceSearchSummary, SearchResources, StoreError, normalize_resource_search_text,
};

/// Invalid deterministic search fixture.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ResourceSearchSeedError {
  /// One fixture repeated a stable resource identity.
  #[error("resource search fixture repeats an identity")]
  DuplicateResource,
  /// The in-memory adapter lock was poisoned.
  #[error("resource search fixture is unavailable")]
  Unavailable,
}

/// Deterministic process-local adapter for ranked global-search tests.
#[derive(Default)]
pub struct InMemoryResourceSearchStore {
  resources: Mutex<BTreeMap<ResourceSearchResource, SearchRecord>>,
}

impl InMemoryResourceSearchStore {
  /// Creates an empty global-search adapter.
  #[must_use]
  pub fn new() -> Self {
    Self::default()
  }

  /// Seeds one typed summary whose label is the authoritative search name.
  pub fn seed(&self, summary: ResourceSearchSummary) -> Result<(), ResourceSearchSeedError> {
    let resource = summary.resource();
    let mut resources = self.lock()?;
    if resources.contains_key(&resource) {
      return Err(ResourceSearchSeedError::DuplicateResource);
    }
    resources.insert(resource, SearchRecord { summary });
    Ok(())
  }

  fn lock(&self) -> Result<MutexGuard<'_, BTreeMap<ResourceSearchResource, SearchRecord>>, ResourceSearchSeedError> {
    self.resources.lock().map_err(|_| ResourceSearchSeedError::Unavailable)
  }
}

struct SearchRecord {
  summary: ResourceSearchSummary,
}

impl SearchRecord {
  fn rank(&self, query: &str) -> Option<ResourceSearchRank> {
    if self.summary.resource().identity() == query {
      return Some(ResourceSearchRank::ExactIdentifier);
    }
    let normalized_label = normalize_resource_search_text(self.summary.label());
    if normalized_label.starts_with(query) {
      return Some(ResourceSearchRank::NamePrefix);
    }
    normalized_label
      .contains(query)
      .then_some(ResourceSearchRank::NameContains)
  }

  fn position(&self, rank: ResourceSearchRank) -> ResourceSearchPagePosition {
    ResourceSearchPagePosition {
      rank,
      kind: self.summary.resource().kind(),
      normalized_label: normalize_resource_search_text(self.summary.label()),
      resource: self.summary.resource(),
    }
  }
}

#[async_trait]
impl ResourceSearchStore for InMemoryResourceSearchStore {
  async fn search_resources(&self, request: SearchResources) -> Result<ResourceSearchPage, StoreError> {
    let resources = self.resources.lock().map_err(|_| StoreError::Unavailable)?;
    let query = request.query().as_str();
    let after = request.after();
    let mut matches = resources
      .values()
      .filter(|record| request.visibility().allows(&record.summary.resource()))
      .filter(|record| request.kinds().contains(&record.summary.resource().kind()))
      .filter_map(|record| record.rank(query).map(|rank| (record.position(rank), record)))
      .filter(|(position, _)| after.is_none_or(|after| position > after))
      .collect::<Vec<_>>();
    matches.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    matches.truncate(usize::from(request.limit().get()).saturating_add(1));
    let next_cursor = finish_bounded_page(&mut matches, request.limit(), |(position, _)| position.clone());
    Ok(ResourceSearchPage {
      items: matches.into_iter().map(|(_, record)| record.summary.clone()).collect(),
      next_cursor,
    })
  }
}
