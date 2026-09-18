//! Transport-independent OctaCity server use cases.
//!
//! Typed commands, queries, transaction coordination, projections, and
//! application error mapping belong here. Transport and concrete persistence
//! types must remain outside this crate.

#![forbid(unsafe_code)]

mod project_policy;

use std::sync::Arc;

use octacity_server_domain::ProjectId;
use octacity_server_store::{
  LogIndexWorkStore, LogSearchError, LogSearchFreshness, LogSearchIndex, LogSearchPage, LogSearchQuery, StoreError,
};
use thiserror::Error;

pub use project_policy::{
  ArtifactPolicy, CacheNamespace, CachePolicy, ConcurrencyPolicy, EffectiveProjectPolicy, IdentityProfileName,
  MAX_POLICY_REFERENCE_BYTES, PolicyCategory, PolicyDirective, PolicyResolutionError, PolicySource, ProjectPolicy,
  ProjectPolicyLayer, RetentionPolicy, RuntimeClass, SecretProfileName, resolve_project_policy,
};

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
  pub async fn search(&self, query: LogSearchQuery) -> Result<LogSearchPage, BuildLogSearchError> {
    query.validate().map_err(|source| {
      BuildLogSearchError::SearchIndex(LogSearchError::invalid(
        octacity_server_store::LogSearchOperation::Search,
        source,
      ))
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
      .map(|indexed| LogSearchPage {
        hits: indexed.hits,
        next_cursor: indexed.next_cursor,
        freshness: LogSearchFreshness {
          indexed_through: indexed.indexed_through,
          committed_through,
        },
      })
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
