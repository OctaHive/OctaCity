//! Bounded provider-neutral discovery, preserving first page observations under the existing scan fence.
use crate::{ApplicationError, FactoryExecutionClock, WorkSourceSnapshot};
use async_trait::async_trait;
use octacity_server_domain::Timestamp;
use octacity_server_factory::FactoryDigest;
use octacity_server_store::{
  FactorySourceStore, MAX_SOURCE_POLL_PAGE_BYTES, MAX_SOURCE_POLL_PAGE_ITEMS, WorkSourceProgress, WorkSourceScan,
  WorkSourceScanClaim,
};
use std::sync::Arc;

// Application safety ceilings; installed policies may narrow them independently of provider quotas.
const MAX_POLL_PAGES: u16 = 100;
const MAX_POLL_ITEMS: u16 = 10_000;
const MAX_POLL_BYTES: u32 = 64 * 1024 * 1024;

/// Trusted installed page and invocation budgets; no provider data can enlarge them.
#[derive(Clone, Debug, serde::Serialize)]
pub struct SourcePollingPolicy {
  /// Maximum observations in one provider page.
  pub page_items: u16,
  /// Maximum provider pages fetched by one invocation.
  pub max_pages: u16,
  /// Maximum retained candidates settled by one invocation.
  pub max_items: u16,
  /// Maximum normalized page bytes examined by one invocation.
  pub max_bytes: u32,
  /// Maximum encoded normalized bytes in one page.
  pub max_page_bytes: u32,
  /// Inclusive overlap before the last completed incremental boundary.
  pub overlap_millis: u64,
  /// Minimum period between completed full reconciliations.
  pub reconciliation_interval_millis: u64,
  /// Fixed retention of the first observed private page; retries never renew it.
  pub retention_millis: u64,
}
impl SourcePollingPolicy {
  /// Checks the shared finite read/retention policy before provider activity.
  pub fn validate(&self) -> Result<(), ApplicationError> {
    if !(1..=MAX_SOURCE_POLL_PAGE_ITEMS).contains(&self.page_items)
      || !(1..=MAX_POLL_PAGES).contains(&self.max_pages)
      || !(1..=MAX_POLL_ITEMS).contains(&self.max_items)
      || !(1..=MAX_SOURCE_POLL_PAGE_BYTES).contains(&self.max_page_bytes)
      || self.max_bytes < self.max_page_bytes
      || self.max_bytes > MAX_POLL_BYTES
      || self.reconciliation_interval_millis == 0
      || self.reconciliation_interval_millis > i64::MAX as u64
      || self.overlap_millis > i64::MAX as u64
      || self.retention_millis == 0
      || self.retention_millis > octacity_server_store::MAX_SOURCE_RETENTION_MILLIS
    {
      return Err(ApplicationError::invalid());
    }
    Ok(())
  }
}
/// Exact provider read without Work admission or checkpoint authority.
#[derive(Clone)]
pub struct SourcePollRequest {
  /// Current installed selection and live owner; provider configuration is independently resolved.
  pub claim: WorkSourceScanClaim,
  /// Original pass identity; lease takeover cannot change a provider continuation context.
  pub scan_id: FactoryDigest,
  /// Frozen inclusive incremental window, or full reconciliation.
  pub window: Option<(Timestamp, Timestamp)>,
  /// Private opaque provider position.
  pub continuation: Option<String>,
  /// Exact page item ceiling.
  pub max_items: u16,
  /// Complete normalized page byte ceiling.
  pub max_page_bytes: u32,
}
impl std::fmt::Debug for SourcePollRequest {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("SourcePollRequest")
      .field("claim", &self.claim)
      .field("scan_id", &self.scan_id)
      .field("window", &self.window)
      .field("continuation", &self.continuation.as_ref().map(|_| "<redacted>"))
      .field("max_items", &self.max_items)
      .field("max_page_bytes", &self.max_page_bytes)
      .finish()
  }
}
/// Bounded normalized observations from a verified installed capability.
#[derive(Clone, serde::Serialize)]
pub struct SourcePollPage {
  /// Private candidates; source text cannot choose a Factory target.
  pub candidates: Vec<WorkSourceSnapshot>,
  /// Opaque continuation, including after an empty filtered page.
  pub next_cursor: Option<String>,
}
/// Provider boundary; concrete adapters own network, selection and wire translation.
#[async_trait]
pub trait FactorySourcePageReader: Send + Sync {
  /// Reads one bounded normalized page under the exact installed selection.
  async fn read(&self, request: SourcePollRequest) -> Result<SourcePollPage, ApplicationError>;
}
/// Trusted application boundary using existing selection, preparation and admission authority.
#[async_trait]
pub trait FactorySourceCandidateProcessor: Send + Sync {
  /// Returns success only after durable admission/replay or a durable classified disposition.
  /// Implementations recheck this claim before effects and preserve original retained intent.
  async fn settle(&self, claim: WorkSourceScanClaim, snapshot: WorkSourceSnapshot) -> Result<(), ApplicationError>;
}
/// Safe per-invocation counters; no snapshot or cursor is a diagnostic.
#[derive(Default, Debug, Eq, PartialEq)]
pub struct SourcePollOutcome {
  /// Canonical normalized bytes fetched or resumed within this invocation.
  pub bytes_examined: u32,
  /// Provider pages actually fetched.
  pub pages_read: u16,
  /// Candidates whose processing and checkpoint both completed.
  pub items_processed: u16,
}
/// One finite polling slice; scheduling and verified process composition are separate owners.
pub struct FactorySourcePoller<S, R, P> {
  store: Arc<S>,
  reader: Arc<R>,
  processor: Arc<P>,
  clock: Arc<dyn FactoryExecutionClock>,
  policy: SourcePollingPolicy,
}
impl<S: FactorySourceStore, R: FactorySourcePageReader, P: FactorySourceCandidateProcessor>
  FactorySourcePoller<S, R, P>
{
  /// Installs trusted time, policy and the existing authoritative store/application boundaries.
  pub fn new(
    store: Arc<S>,
    reader: Arc<R>,
    processor: Arc<P>,
    clock: Arc<dyn FactoryExecutionClock>,
    policy: SourcePollingPolicy,
  ) -> Result<Self, ApplicationError> {
    policy.validate()?;
    Ok(Self {
      store,
      reader,
      processor,
      clock,
      policy,
    })
  }
  /// Uses the shared bounded driver under the existing exact Factory scan fence.
  pub async fn run_once(&self, scan: WorkSourceScan) -> Result<SourcePollOutcome, ApplicationError> {
    let lease = crate::source_polling::SourcePollingLease {
      project_id: scan.claim.project_id,
      target: serde_json::to_vec(&scan.claim.target).map_err(|_| ApplicationError::invalid())?,
      acquired_at: scan.claim.acquired_at,
      until: scan.claim.until,
    };
    let session = FactoryPollingSession {
      store: self.store.as_ref(),
      reader: self.reader.as_ref(),
      processor: self.processor.as_ref(),
      claim: scan.claim,
    };
    crate::source_polling::SourcePollingDriver {
      session: &session,
      clock: self.clock.as_ref(),
      policy: &self.policy,
      lease,
    }
    .run_once(scan.progress)
    .await
  }
}
struct FactoryPollingSession<'a, S, R, P> {
  store: &'a S,
  reader: &'a R,
  processor: &'a P,
  claim: WorkSourceScanClaim,
}
#[async_trait]
impl<S: FactorySourceStore, R: FactorySourcePageReader, P: FactorySourceCandidateProcessor>
  crate::source_polling::SourcePollingSession for FactoryPollingSession<'_, S, R, P>
{
  async fn read(
    &self,
    request: crate::source_polling::SourcePollingRequest,
  ) -> Result<crate::source_polling::SourcePollingPage, ApplicationError> {
    let page = self
      .reader
      .read(SourcePollRequest {
        claim: self.claim.clone(),
        scan_id: request.scan_id,
        window: request.window,
        continuation: request.continuation,
        max_items: request.max_items,
        max_page_bytes: request.max_page_bytes,
      })
      .await?;
    Ok(crate::source_polling::SourcePollingPage {
      expires_at: None,
      candidates: page
        .candidates
        .into_iter()
        .map(serde_json::to_value)
        .collect::<Result<_, _>>()
        .map_err(|_| ApplicationError::invalid())?,
      next_cursor: page.next_cursor,
    })
  }
  async fn settle(&self, candidate: serde_json::Value) -> Result<(), ApplicationError> {
    self
      .processor
      .settle(
        self.claim.clone(),
        serde_json::from_value(candidate).map_err(|_| ApplicationError::invalid())?,
      )
      .await
  }
  async fn checkpoint(
    &self,
    at: Timestamp,
    expected: Option<WorkSourceProgress>,
    progress: WorkSourceProgress,
  ) -> Result<(), ApplicationError> {
    Ok(
      self
        .store
        .checkpoint_source_scan(self.claim.clone(), at, expected, progress)
        .await?,
    )
  }
}
