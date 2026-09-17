use std::num::NonZeroU64;

use octacity_server_domain::{AttemptId, BuildId, JobId, LogChunkId, LogIndexingWorkId, ProjectId, Timestamp};
use thiserror::Error;

/// Maximum UTF-8 bytes accepted in one Build-log search query.
pub const MAX_LOG_SEARCH_QUERY_BYTES: usize = 1_024;
/// Maximum redacted UTF-8 bytes accepted in one indexed log document.
pub const MAX_LOG_SEARCH_DOCUMENT_BYTES: usize = 256 * 1_024;
/// Maximum number of hits returned by one search page.
pub const MAX_LOG_SEARCH_PAGE_SIZE: u16 = 100;
/// Maximum UTF-8 bytes returned in one search-result snippet.
pub const MAX_LOG_SEARCH_SNIPPET_BYTES: usize = 512;

/// Contiguous project-local position of indexing work in the authoritative outbox.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LogIndexPosition(NonZeroU64);

impl LogIndexPosition {
  /// Constructs a positive durable outbox position.
  pub fn new(value: u64) -> Result<Self, LogSearchInputError> {
    NonZeroU64::new(value)
      .map(Self)
      .ok_or(LogSearchInputError::ZeroPosition)
  }

  /// Returns the positive numeric position.
  #[must_use]
  pub const fn get(self) -> u64 {
    self.0.get()
  }
}

/// Logical stream within a Job's Build log.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BuildLogStream {
  /// Process standard output.
  Stdout,
  /// Process standard error.
  Stderr,
}

/// Provider-neutral interpretation of query text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogSearchMode {
  /// Match all whitespace-separated terms without language stemming.
  FullText,
  /// Match the supplied UTF-8 fragment literally.
  Literal,
}

/// Stable logical cursor for deterministic newest-first pagination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LogSearchCursor {
  /// Source time of the last document returned by the preceding page.
  pub occurred_at: Timestamp,
  /// Stable tie-breaker of the last document returned by the preceding page.
  pub chunk_id: LogChunkId,
}

/// Bounded backend-neutral Build-log query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogSearchQuery {
  /// Project boundary that every result must belong to.
  pub project_id: ProjectId,
  /// Non-empty bounded UTF-8 query text.
  pub text: String,
  /// Query interpretation selected by the caller.
  pub mode: LogSearchMode,
  /// Optional Build filter.
  pub build_id: Option<BuildId>,
  /// Optional Attempt filter.
  pub attempt_id: Option<AttemptId>,
  /// Optional Job filter.
  pub job_id: Option<JobId>,
  /// Optional output-stream filter.
  pub stream: Option<BuildLogStream>,
  /// Inclusive lower source-time bound.
  pub occurred_from: Option<Timestamp>,
  /// Inclusive upper source-time bound.
  pub occurred_through: Option<Timestamp>,
  /// Exclusive cursor from a preceding page.
  pub after: Option<LogSearchCursor>,
  /// Maximum number of hits requested for this page.
  pub limit: u16,
}

impl LogSearchQuery {
  /// Validates all adapter-independent query bounds.
  pub fn validate(&self) -> Result<(), LogSearchInputError> {
    if self.text.is_empty()
      || self.text.len() > MAX_LOG_SEARCH_QUERY_BYTES
      || self.text.trim() != self.text
      || self.text.chars().any(|character| character == '\0')
    {
      return Err(LogSearchInputError::InvalidQueryText);
    }
    if self.limit == 0 || self.limit > MAX_LOG_SEARCH_PAGE_SIZE {
      return Err(LogSearchInputError::InvalidPageSize);
    }
    if self
      .occurred_from
      .zip(self.occurred_through)
      .is_some_and(|(from, through)| from > through)
    {
      return Err(LogSearchInputError::InvalidTimeRange);
    }
    Ok(())
  }
}

/// One immutable redacted log chunk projected into the search index.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogSearchDocument {
  /// Immutable archived chunk identity.
  pub chunk_id: LogChunkId,
  /// Owning Project.
  pub project_id: ProjectId,
  /// Owning Build.
  pub build_id: BuildId,
  /// Owning Build Attempt.
  pub attempt_id: AttemptId,
  /// Owning materialized Job.
  pub job_id: JobId,
  /// Logical output stream.
  pub stream: BuildLogStream,
  /// First event sequence represented by this chunk.
  pub first_sequence: u64,
  /// Last event sequence represented by this chunk.
  pub last_sequence: u64,
  /// Source time used for filtering and deterministic ordering.
  pub occurred_at: Timestamp,
  /// Normalized text after mandatory secret redaction.
  pub redacted_text: String,
}

impl LogSearchDocument {
  /// Validates document bounds and its contiguous logical sequence range.
  pub fn validate(&self) -> Result<(), LogSearchInputError> {
    if self.first_sequence == 0 || self.last_sequence < self.first_sequence {
      return Err(LogSearchInputError::InvalidSequenceRange);
    }
    if self.redacted_text.is_empty()
      || self.redacted_text.len() > MAX_LOG_SEARCH_DOCUMENT_BYTES
      || self.redacted_text.chars().any(|character| character == '\0')
    {
      return Err(LogSearchInputError::InvalidDocumentText);
    }
    Ok(())
  }
}

/// Idempotent request to index or rebuild one committed log document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteLogSearchDocument {
  /// Durable identity used to deduplicate delivery and retries.
  pub work_id: LogIndexingWorkId,
  /// Monotonic authoritative position represented by this work.
  pub position: LogIndexPosition,
  /// Complete provider-neutral projection document.
  pub document: LogSearchDocument,
}

/// Idempotent request to hide all search documents for one logical Build.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeleteLogSearchDocuments {
  /// Durable identity used to deduplicate delivery and retries.
  pub work_id: LogIndexingWorkId,
  /// Monotonic authoritative position represented by this work.
  pub position: LogIndexPosition,
  /// Project boundary of the Build being removed.
  pub project_id: ProjectId,
  /// Build whose derived documents must become invisible.
  pub build_id: BuildId,
}

/// Current projection progress for one Project.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LogSearchFreshness {
  /// Greatest contiguous authoritative work position applied, if any.
  pub indexed_through: Option<LogIndexPosition>,
  /// Latest committed authoritative work position observed by the caller.
  pub committed_through: Option<LogIndexPosition>,
}

/// Outcome of applying durable work to the derived log-search projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogSearchMutationDisposition {
  /// The projection changed during this call.
  Applied,
  /// An identical work item had already produced this result.
  Replayed,
  /// A newer durable tombstone made this work obsolete.
  Superseded,
}

impl LogSearchFreshness {
  /// Reports whether the projection has applied every committed position.
  #[must_use]
  pub fn is_caught_up(self) -> bool {
    match self.committed_through {
      None => true,
      Some(committed) => self.indexed_through.is_some_and(|indexed| indexed >= committed),
    }
  }
}

/// One backend-neutral Build-log search result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogSearchHit {
  /// Immutable archived chunk containing the match.
  pub chunk_id: LogChunkId,
  /// Owning Build.
  pub build_id: BuildId,
  /// Owning Build Attempt.
  pub attempt_id: AttemptId,
  /// Owning materialized Job.
  pub job_id: JobId,
  /// Logical output stream.
  pub stream: BuildLogStream,
  /// First event sequence represented by the matched chunk.
  pub first_sequence: u64,
  /// Last event sequence represented by the matched chunk.
  pub last_sequence: u64,
  /// Source time of the matched chunk.
  pub occurred_at: Timestamp,
  /// Bounded redacted context around the match.
  pub snippet: String,
}

/// One deterministic page of Build-log search results.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogSearchPage {
  /// Matching documents in stable newest-first order.
  pub hits: Vec<LogSearchHit>,
  /// Cursor for the following page, absent when this is the final page.
  pub next_cursor: Option<LogSearchCursor>,
  /// Projection position observed by this search.
  pub freshness: LogSearchFreshness,
}

/// Search results and projection progress returned by a derived index.
///
/// The index does not receive or report the authoritative committed watermark;
/// the application layer combines this value with [`LogSearchFreshness`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexedLogSearchPage {
  /// Matching documents in stable newest-first order.
  pub hits: Vec<LogSearchHit>,
  /// Cursor for the following page, absent when this is the final page.
  pub next_cursor: Option<LogSearchCursor>,
  /// Greatest contiguous authoritative work position applied by the index.
  pub indexed_through: Option<LogIndexPosition>,
}

/// Search-projection operation associated with a classified failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogSearchOperation {
  /// Query indexed log documents.
  Search,
  /// Index one newly committed document.
  Index,
  /// Read projection progress.
  ReadFreshness,
  /// Hide documents for one deleted Build.
  Delete,
  /// Restore one document from committed authoritative storage.
  Rebuild,
}

/// Invalid adapter-independent Build-log search input.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum LogSearchInputError {
  /// Durable outbox positions start at one.
  #[error("an indexing position must be greater than zero")]
  ZeroPosition,
  /// Query text is empty, malformed, or exceeds its UTF-8 byte bound.
  #[error("log search query text is invalid")]
  InvalidQueryText,
  /// The requested page size is zero or exceeds the public bound.
  #[error("log search page size is invalid")]
  InvalidPageSize,
  /// The lower time bound follows the upper time bound.
  #[error("log search time range is invalid")]
  InvalidTimeRange,
  /// A document sequence range is empty, reversed, or starts at zero.
  #[error("log search document sequence range is invalid")]
  InvalidSequenceRange,
  /// Redacted document text is empty, malformed, or exceeds its byte bound.
  #[error("log search document text is invalid")]
  InvalidDocumentText,
}

/// Backend-neutral failure returned by a Build-log search projection.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum LogSearchError {
  /// The request is invalid independently of current projection state.
  #[error("invalid {operation:?} input: {source}")]
  InvalidInput {
    /// Operation that rejected the input.
    operation: LogSearchOperation,
    /// Stable validation failure.
    source: LogSearchInputError,
  },
  /// One durable work identity was reused for different content or operations.
  #[error("log indexing work {work_id} conflicts with an earlier request")]
  WorkConflict {
    /// Reused durable work identity.
    work_id: LogIndexingWorkId,
  },
  /// An immutable chunk identity already describes different searchable content.
  #[error("log chunk {chunk_id} conflicts with an earlier document")]
  DocumentConflict {
    /// Reused immutable chunk identity.
    chunk_id: LogChunkId,
  },
  /// The derived projection cannot currently execute the operation.
  #[error("log search index is unavailable")]
  Unavailable,
}

impl LogSearchError {
  /// Classifies adapter-independent validation at the attempted operation.
  #[must_use]
  pub const fn invalid(operation: LogSearchOperation, source: LogSearchInputError) -> Self {
    Self::InvalidInput { operation, source }
  }
}
