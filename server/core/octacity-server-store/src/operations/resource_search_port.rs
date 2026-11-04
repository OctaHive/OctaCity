use async_trait::async_trait;

use crate::{ResourceSearchPage, SearchResources, StoreError};

/// Backend-neutral ranked global resource-search port.
#[async_trait]
pub trait ResourceSearchStore: Send + Sync {
  /// Searches visible resources in deterministic rank order.
  async fn search_resources(&self, request: SearchResources) -> Result<ResourceSearchPage, StoreError>;
}
