use std::sync::Arc;

use octacity_server_domain::{AttemptId, BuildId, JobId, LogChunkId, ProjectId, Timestamp};
use octacity_server_store::{
  BuildLogStream, IndexedLogSearchPage, LogIndexPosition, LogIndexWorkStore, LogSearchCursor, LogSearchError,
  LogSearchFreshness, LogSearchIndex, LogSearchMode, LogSearchOperation, LogSearchQuery, StoreError,
};
use thiserror::Error;

use crate::{ApplicationError, ApplicationFailure, Query, QueryHandler};

/// Typed application query for bounded redacted Build-log search.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchBuildLogsQuery {
  /// Backend-neutral validated search shape.
  pub search: LogSearchQuery,
}

/// Transport-independent input used to construct a validated Build-log query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildLogSearchInput {
  /// Project boundary that every result must belong to.
  pub project_id: String,
  /// Non-empty bounded UTF-8 query text.
  pub text: String,
  /// Explicit query interpretation.
  pub mode: LogSearchMode,
  /// Optional Build filter.
  pub build_id: Option<String>,
  /// Optional Attempt filter.
  pub attempt_id: Option<String>,
  /// Optional Job filter.
  pub job_id: Option<String>,
  /// Optional logical stream filter.
  pub stream: Option<BuildLogStream>,
  /// Inclusive lower source-time bound in Unix milliseconds.
  pub occurred_from_unix_ms: Option<i64>,
  /// Inclusive upper source-time bound in Unix milliseconds.
  pub occurred_through_unix_ms: Option<i64>,
  /// Optional exclusive continuation state.
  pub after: Option<BuildLogSearchCursorInput>,
  /// Maximum number of hits in the page.
  pub limit: u16,
}

/// Decoded opaque cursor input at the application boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildLogSearchCursorInput {
  /// Source time of the final hit in the preceding page.
  pub occurred_at_unix_ms: i64,
  /// Logical chunk tie-breaker of the final hit in the preceding page.
  pub chunk_id: String,
}

impl SearchBuildLogsQuery {
  /// Parses identities and times, then validates all backend-neutral bounds.
  pub fn try_from_input(input: BuildLogSearchInput) -> Result<Self, ApplicationError> {
    let search = LogSearchQuery {
      project_id: input.project_id.parse().map_err(|_| ApplicationError::invalid())?,
      text: input.text,
      mode: input.mode,
      build_id: parse_optional_id(input.build_id)?,
      attempt_id: parse_optional_id(input.attempt_id)?,
      job_id: parse_optional_id(input.job_id)?,
      stream: input.stream,
      occurred_from: parse_optional_time(input.occurred_from_unix_ms)?,
      occurred_through: parse_optional_time(input.occurred_through_unix_ms)?,
      after: input
        .after
        .map(|cursor| {
          Ok::<_, ApplicationError>(LogSearchCursor {
            occurred_at: Timestamp::from_unix_millis(cursor.occurred_at_unix_ms)
              .map_err(|_| ApplicationError::invalid())?,
            chunk_id: cursor.chunk_id.parse().map_err(|_| ApplicationError::invalid())?,
          })
        })
        .transpose()?,
      limit: input.limit,
    };
    search.validate().map_err(|_| ApplicationError::invalid())?;
    Ok(Self { search })
  }
}

fn parse_optional_id<T>(value: Option<String>) -> Result<Option<T>, ApplicationError>
where
  T: std::str::FromStr,
{
  value
    .map(|value| value.parse().map_err(|_| ApplicationError::invalid()))
    .transpose()
}

fn parse_optional_time(value: Option<i64>) -> Result<Option<Timestamp>, ApplicationError> {
  value
    .map(|value| Timestamp::from_unix_millis(value).map_err(|_| ApplicationError::invalid()))
    .transpose()
}

impl Query for SearchBuildLogsQuery {
  type Outcome = BuildLogSearchPageProjection;
}

/// One backend-neutral redacted Build-log search result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildLogSearchHitProjection {
  /// Immutable archived chunk identity.
  pub chunk_id: LogChunkId,
  /// Owning Build identity.
  pub build_id: BuildId,
  /// Owning Attempt identity.
  pub attempt_id: AttemptId,
  /// Owning Job identity.
  pub job_id: JobId,
  /// Logical output stream.
  pub stream: BuildLogStream,
  /// First event sequence represented by the matched chunk.
  pub first_sequence: u64,
  /// Last event sequence represented by the matched chunk.
  pub last_sequence: u64,
  /// Source-observed Unix time in milliseconds.
  pub occurred_at_unix_ms: i64,
  /// Bounded redacted context around the match.
  pub snippet: String,
}

/// Opaque continuation state for deterministic Build-log pagination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildLogSearchCursorProjection {
  /// Source time of the final hit in the preceding page.
  pub occurred_at_unix_ms: i64,
  /// Stable logical tie-breaker of the final hit in the preceding page.
  pub chunk_id: LogChunkId,
}

/// Projection progress compared with authoritative committed logs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildLogSearchFreshnessProjection {
  /// Greatest contiguous Project position applied by the search index.
  pub indexed_through: Option<u64>,
  /// Latest authoritative committed Project position.
  pub committed_through: Option<u64>,
  /// Whether every committed position is represented by the projection.
  pub caught_up: bool,
}

/// Deterministic bounded page returned by the Build-log query service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildLogSearchPageProjection {
  /// Matching logical log chunks in newest-first order.
  pub items: Vec<BuildLogSearchHitProjection>,
  /// Continuation state for the following page.
  pub next_cursor: Option<BuildLogSearchCursorProjection>,
  /// Projection progress observed by the query.
  pub freshness: BuildLogSearchFreshnessProjection,
}

/// Typed application query for Build-log projection freshness.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetBuildLogFreshnessQuery {
  /// Project whose authoritative and indexed watermarks are requested.
  pub project_id: ProjectId,
}

impl Query for GetBuildLogFreshnessQuery {
  type Outcome = LogSearchFreshness;
}

/// Application query service that combines authoritative indexing work with a
/// replaceable derived Build-log search projection.
pub struct BuildLogSearch<W, I> {
  work: Arc<W>,
  index: Arc<I>,
}

impl<W, I> BuildLogSearch<W, I>
where
  W: LogIndexWorkStore,
  I: LogSearchIndex,
{
  /// Creates a query service from authoritative and derived store ports.
  pub fn new(work: Arc<W>, index: Arc<I>) -> Self {
    Self { work, index }
  }

  /// Searches logs and reports freshness against the durable committed watermark.
  pub async fn search(&self, query: LogSearchQuery) -> Result<BuildLogSearchPageProjection, BuildLogSearchError> {
    query.validate().map_err(|source| {
      BuildLogSearchError::SearchIndex(LogSearchError::invalid(LogSearchOperation::Search, source))
    })?;
    let committed_through = self
      .work
      .committed_log_index_position(query.project_id)
      .await
      .map_err(BuildLogSearchError::AuthoritativeStore)?;
    self
      .index
      .search(query)
      .await
      .map(|indexed| project_log_search_page(indexed, committed_through))
      .map_err(BuildLogSearchError::SearchIndex)
  }

  /// Reports projection freshness against the durable committed watermark.
  pub async fn freshness(&self, project_id: ProjectId) -> Result<LogSearchFreshness, BuildLogSearchError> {
    let committed_through = self
      .work
      .committed_log_index_position(project_id)
      .await
      .map_err(BuildLogSearchError::AuthoritativeStore)?;
    let indexed_through = self
      .index
      .indexed_through(project_id)
      .await
      .map_err(BuildLogSearchError::SearchIndex)?;
    Ok(LogSearchFreshness {
      indexed_through,
      committed_through,
    })
  }
}

#[async_trait::async_trait]
impl<W, I> QueryHandler<SearchBuildLogsQuery> for BuildLogSearch<W, I>
where
  W: LogIndexWorkStore + 'static,
  I: LogSearchIndex + 'static,
{
  type Error = BuildLogSearchError;

  async fn handle_query(&self, query: SearchBuildLogsQuery) -> Result<BuildLogSearchPageProjection, Self::Error> {
    self.search(query.search).await
  }
}

fn project_log_search_page(
  indexed: IndexedLogSearchPage,
  committed_through: Option<LogIndexPosition>,
) -> BuildLogSearchPageProjection {
  let freshness = LogSearchFreshness {
    indexed_through: indexed.indexed_through,
    committed_through,
  };
  BuildLogSearchPageProjection {
    items: indexed
      .hits
      .into_iter()
      .map(|hit| BuildLogSearchHitProjection {
        chunk_id: hit.chunk_id,
        build_id: hit.build_id,
        attempt_id: hit.attempt_id,
        job_id: hit.job_id,
        stream: hit.stream,
        first_sequence: hit.first_sequence,
        last_sequence: hit.last_sequence,
        occurred_at_unix_ms: hit.occurred_at.unix_millis(),
        snippet: hit.snippet,
      })
      .collect(),
    next_cursor: indexed.next_cursor.map(|cursor| BuildLogSearchCursorProjection {
      occurred_at_unix_ms: cursor.occurred_at.unix_millis(),
      chunk_id: cursor.chunk_id,
    }),
    freshness: BuildLogSearchFreshnessProjection {
      indexed_through: freshness.indexed_through.map(LogIndexPosition::get),
      committed_through: freshness.committed_through.map(LogIndexPosition::get),
      caught_up: freshness.is_caught_up(),
    },
  }
}

#[async_trait::async_trait]
impl<W, I> QueryHandler<GetBuildLogFreshnessQuery> for BuildLogSearch<W, I>
where
  W: LogIndexWorkStore + 'static,
  I: LogSearchIndex + 'static,
{
  type Error = BuildLogSearchError;

  async fn handle_query(&self, query: GetBuildLogFreshnessQuery) -> Result<LogSearchFreshness, Self::Error> {
    self.freshness(query.project_id).await
  }
}

/// Safe application-level failure from a Build-log search query.
#[derive(Debug, Error)]
pub enum BuildLogSearchError {
  /// The authoritative watermark could not be read.
  #[error("authoritative log-index watermark is unavailable")]
  AuthoritativeStore(#[source] StoreError),
  /// The derived search projection rejected or could not execute the query.
  #[error("build-log search index failed")]
  SearchIndex(#[source] LogSearchError),
}

impl BuildLogSearchError {
  /// Returns the stable transport-neutral classification of this query failure.
  #[must_use]
  pub const fn classification(&self) -> ApplicationFailure {
    match self {
      Self::AuthoritativeStore(StoreError::InvalidInput { .. })
      | Self::SearchIndex(LogSearchError::InvalidInput { .. }) => ApplicationFailure::Invalid,
      Self::AuthoritativeStore(StoreError::NotFound { .. }) => ApplicationFailure::NotFound,
      Self::AuthoritativeStore(
        StoreError::Conflict { .. }
        | StoreError::Duplicate { .. }
        | StoreError::Fenced { .. }
        | StoreError::Expired { .. }
        | StoreError::EventGap { .. }
        | StoreError::EventsMissing { .. }
        | StoreError::CredentialRejected,
      ) => ApplicationFailure::Conflict,
      Self::AuthoritativeStore(StoreError::Unavailable) | Self::SearchIndex(LogSearchError::Unavailable) => {
        ApplicationFailure::Unavailable
      }
      Self::SearchIndex(LogSearchError::WorkConflict { .. } | LogSearchError::DocumentConflict { .. }) => {
        ApplicationFailure::Internal
      }
    }
  }
}
