use async_trait::async_trait;
use octacity_server_domain::ProjectId;

use crate::{
  DeleteLogSearchDocuments, IndexedLogSearchPage, LogIndexPosition, LogSearchError, LogSearchMutationDisposition,
  LogSearchQuery, WriteLogSearchDocument,
};

/// Backend-neutral derived projection for bounded Build-log search.
///
/// The authoritative store commits log chunks and durable indexing work before
/// this port is called. An unavailable implementation therefore delays search
/// freshness but must never invalidate or block the committed Build history.
#[async_trait]
pub trait LogSearchIndex: Send + Sync {
  /// Searches committed redacted log projections with deterministic pagination.
  ///
  /// The returned progress belongs solely to this derived projection. The
  /// application obtains the authoritative watermark independently.
  async fn search(&self, query: LogSearchQuery) -> Result<IndexedLogSearchPage, LogSearchError>;

  /// Applies one newly committed indexing work item idempotently.
  async fn index(&self, request: WriteLogSearchDocument) -> Result<LogSearchMutationDisposition, LogSearchError>;

  /// Returns the greatest contiguous work position applied for one Project.
  async fn indexed_through(&self, project_id: ProjectId) -> Result<Option<LogIndexPosition>, LogSearchError>;

  /// Hides every derived document for one deleted Build idempotently.
  async fn delete(&self, request: DeleteLogSearchDocuments) -> Result<LogSearchMutationDisposition, LogSearchError>;

  /// Restores one document from authoritative committed chunks idempotently.
  ///
  /// Callers rebuild a lost projection with bounded repeated calls; adapters
  /// must not require a complete Build or index snapshot in memory.
  async fn rebuild(&self, request: WriteLogSearchDocument) -> Result<LogSearchMutationDisposition, LogSearchError>;
}

/// Authoritative source of committed project-local log-indexing work.
///
/// This is deliberately separate from the derived search projection: losing
/// or rebuilding an index must not lose the durable watermark used to report
/// whether search results are complete.
#[async_trait]
pub trait LogIndexWorkStore: Send + Sync {
  /// Returns the greatest committed work position for one Project.
  async fn committed_log_index_position(
    &self,
    project_id: ProjectId,
  ) -> Result<Option<crate::LogIndexPosition>, crate::StoreError>;
}
