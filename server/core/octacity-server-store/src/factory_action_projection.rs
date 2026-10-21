//! Derived read index; authoritative Run versions invalidate it in the source transaction.
use crate::{FactoryNodeActionDescription, FactoryRunSnapshot, StoreError, StoreInputError, StoreOperation};
use octacity_server_domain::Timestamp;
use octacity_server_factory::{FactoryRunId, FactoryRunVersion, NodeAttemptId};

/// One resumable backfill/invalidation target; it reserves no execution ownership.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FactoryActionProjectionWork {
  /// Exact source aggregate.
  pub run_id: FactoryRunId,
  /// Version observed while selecting this work.
  pub source_version: FactoryRunVersion,
}
/// A bounded resumable pass; completed source versions are not selected again.
#[derive(Clone, Debug)]
pub struct FactoryActionProjectionWorkBatch {
  /// At most the requested common diagnostic page bound.
  pub items: Vec<FactoryActionProjectionWork>,
  /// More source versions still need materialization.
  pub has_more: bool,
}
/// Source coverage is independent of whether the bounded page contains any action.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryActionProjectionCoverage {
  /// This source version has been fully materialized, including an empty result.
  Complete,
  /// Missing, invalidated or newer-than-observation projection; emptiness proves nothing.
  Incomplete,
}
/// Bounded internal read; caller authorization precedes discovery of this exact Run.
#[derive(Clone, Debug)]
pub struct FactoryActionProjectionQuery {
  /// Original Run, never an Issue version or latest-run hint.
  pub run_id: FactoryRunId,
  /// Trusted observation time for expiry and index freshness.
  pub observed_at: Timestamp,
  /// Exclusive immutable target position.
  pub after: Option<NodeAttemptId>,
  /// Bounded page size, validated again at the store boundary.
  pub limit: u16,
}
impl FactoryActionProjectionQuery {
  /// Uses the existing Factory diagnostic page ceiling.
  pub fn new(
    run_id: FactoryRunId,
    observed_at: Timestamp,
    after: Option<NodeAttemptId>,
    limit: u16,
  ) -> Result<Self, StoreError> {
    validate_action_projection_limit(limit)?;
    Ok(Self {
      run_id,
      observed_at,
      after,
      limit,
    })
  }
}
/// One exact source observation; sensitive execution bodies never enter this index.
#[derive(Clone, Debug)]
pub struct FactoryActionProjectionPage {
  /// Current authoritative source version observed independently of the index.
  pub run_version: FactoryRunVersion,
  /// Original successful indexing time, absent when this source is not covered.
  pub observed_at: Option<Timestamp>,
  /// Complete or explicitly unfinished source coverage.
  pub coverage: FactoryActionProjectionCoverage,
  /// Exact safe descriptions after expiry filtering and before cursor generation.
  pub items: Vec<FactoryNodeActionDescription>,
  /// Exclusive next target position, absent on incomplete observations.
  pub next: Option<NodeAttemptId>,
}
/// Safe derived materialization shared by memory and PostgreSQL adapters.
#[derive(Clone, Debug)]
pub struct FactoryActionProjection {
  /// Source version, which is also the invalidation fence.
  pub source_version: FactoryRunVersion,
  /// Trusted time used by the common eligibility descriptor.
  pub observed_at: Timestamp,
  /// Eligible descriptions only, without private input, reason or Context bodies.
  pub items: Vec<FactoryNodeActionDescription>,
}
impl FactoryActionProjection {
  /// Derives the entire bounded Run from the same descriptor as direct inspection.
  pub fn from_snapshot(snapshot: &FactoryRunSnapshot, at: Timestamp) -> Result<Self, StoreError> {
    if snapshot
      .lifecycle_checkpoints
      .iter()
      .find(|row| row.id == snapshot.current.lifecycle_checkpoint_id)
      .is_none_or(|row| row.recorded_at > at)
    {
      return Err(invalid_action_projection());
    }
    let mut items = snapshot
      .flow
      .attempts
      .iter()
      .filter_map(|attempt| FactoryNodeActionDescription::from_snapshot(snapshot, attempt.id(), at))
      .collect::<Vec<_>>();
    items.sort_by_key(|row| row.node_attempt_id);
    Ok(Self {
      source_version: snapshot.run.version(),
      observed_at: at,
      items,
    })
  }
  /// Filters time/source fences before bounded ordering and continuation.
  #[must_use]
  pub fn page(
    projection: Option<&Self>,
    version: FactoryRunVersion,
    query: &FactoryActionProjectionQuery,
  ) -> FactoryActionProjectionPage {
    let mut page = FactoryActionProjectionPage {
      run_version: version,
      observed_at: None,
      coverage: FactoryActionProjectionCoverage::Incomplete,
      items: vec![],
      next: None,
    };
    let Some(projection) =
      projection.filter(|row| row.source_version == version && row.observed_at <= query.observed_at)
    else {
      return page;
    };
    page.coverage = FactoryActionProjectionCoverage::Complete;
    page.observed_at = Some(projection.observed_at);
    page.items = projection
      .items
      .iter()
      .filter(|row| {
        row.deadline.is_none_or(|deadline| query.observed_at <= deadline)
          && query.after.is_none_or(|after| row.node_attempt_id > after)
      })
      .cloned()
      .collect();
    page.items.sort_by_key(|row| row.node_attempt_id);
    if page.items.len() > usize::from(query.limit) {
      page.items.truncate(usize::from(query.limit));
      page.next = page.items.last().map(|row| row.node_attempt_id);
    }
    page
  }
}
pub(crate) fn invalid_action_projection() -> StoreError {
  StoreError::invalid(
    StoreOperation::ProjectFactoryActions,
    StoreInputError::InvalidFactoryActionProjection,
  )
}
/// Validates maintenance and read bounds at every adapter entry point.
pub fn validate_action_projection_limit(limit: u16) -> Result<(), StoreError> {
  if limit == 0 || limit > crate::MAX_FACTORY_RUN_DIAGNOSTIC_PAGE_SIZE {
    return Err(invalid_action_projection());
  }
  Ok(())
}
/// Internal derived-index maintenance, isolated from admission, controls and execution ownership.
#[async_trait::async_trait]
pub trait FactoryActionProjectionStore: Send + Sync {
  /// Finds missing/stale source versions in a bounded resumable backfill pass.
  async fn factory_action_projection_work(&self, limit: u16) -> Result<FactoryActionProjectionWorkBatch, StoreError>;
  /// Rechecks the source under its aggregate lock before replacing derived rows atomically.
  async fn refresh_factory_action_projection(
    &self,
    work: FactoryActionProjectionWork,
    at: Timestamp,
  ) -> Result<(), StoreError>;
  /// Reads without repair or execution effects; a stopped worker leaves incomplete coverage.
  async fn read_factory_action_projection(
    &self,
    query: FactoryActionProjectionQuery,
  ) -> Result<FactoryActionProjectionPage, StoreError>;
}
