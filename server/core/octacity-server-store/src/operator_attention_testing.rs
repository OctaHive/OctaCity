use std::{collections::BTreeMap, collections::btree_map::Entry, sync::Mutex};

use async_trait::async_trait;
use thiserror::Error;

use crate::pagination::finish_bounded_page;
use crate::{
  ListOperatorAttention, OperatorAttentionEvent, OperatorAttentionId, OperatorAttentionPage,
  OperatorAttentionPagePosition, OperatorAttentionStore, StoreError,
};

/// Invalid deterministic operator-attention fixture.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OperatorAttentionSeedError {
  /// One fixture repeated a stable attention identity.
  #[error("operator attention fixture repeats an identity")]
  DuplicateEvent,
  /// The in-memory adapter lock was poisoned.
  #[error("operator attention fixture is unavailable")]
  Unavailable,
}

/// Deterministic process-local adapter for operator-attention tests.
#[derive(Default)]
pub struct InMemoryOperatorAttentionStore {
  events: Mutex<BTreeMap<OperatorAttentionId, OperatorAttentionEvent>>,
}

impl InMemoryOperatorAttentionStore {
  /// Creates an empty operator-attention adapter.
  #[must_use]
  pub fn new() -> Self {
    Self::default()
  }

  /// Seeds one validated immutable source event.
  pub fn seed(&self, event: OperatorAttentionEvent) -> Result<(), OperatorAttentionSeedError> {
    let mut events = self
      .events
      .lock()
      .map_err(|_| OperatorAttentionSeedError::Unavailable)?;
    match events.entry(event.id()) {
      Entry::Vacant(entry) => {
        entry.insert(event);
        Ok(())
      }
      Entry::Occupied(_) => Err(OperatorAttentionSeedError::DuplicateEvent),
    }
  }
}

#[async_trait]
impl OperatorAttentionStore for InMemoryOperatorAttentionStore {
  async fn list_operator_attention(&self, request: ListOperatorAttention) -> Result<OperatorAttentionPage, StoreError> {
    let events = self.events.lock().map_err(|_| StoreError::Unavailable)?;
    let after = request.after();
    let mut matches = events
      .values()
      .filter(|event| request.source_is_visible(event))
      .filter(|event| request.source_is_selected(event))
      .filter_map(OperatorAttentionEvent::classify)
      .filter(|item| after.is_none_or(|position| (item.occurred_at, item.id) < (position.occurred_at, position.id)))
      .collect::<Vec<_>>();
    matches.sort_unstable_by_key(|item| std::cmp::Reverse((item.occurred_at, item.id)));
    matches.truncate(usize::from(request.limit().get()).saturating_add(1));
    let next_cursor = finish_bounded_page(&mut matches, request.limit(), |item| OperatorAttentionPagePosition {
      occurred_at: item.occurred_at,
      id: item.id,
    });
    Ok(OperatorAttentionPage {
      items: matches,
      next_cursor,
    })
  }
}
