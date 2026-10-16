//! Bounded provider-neutral discovery, preserving first page observations under the existing scan fence.
use crate::{ApplicationError, FactoryExecutionClock, WorkSourceSnapshot};
use async_trait::async_trait;
use octacity_server_domain::Timestamp;
use octacity_server_factory::FactoryDigest;
use octacity_server_store::{
  FactorySourceStore, MAX_SOURCE_POLL_PAGE_BYTES, MAX_SOURCE_POLL_PAGE_ITEMS, WorkSourcePendingPage,
  WorkSourcePollPass, WorkSourcePollingState, WorkSourceProgress, WorkSourceScan, WorkSourceScanClaim,
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
    if !(1..=MAX_SOURCE_POLL_PAGE_ITEMS).contains(&policy.page_items)
      || !(1..=MAX_POLL_PAGES).contains(&policy.max_pages)
      || !(1..=MAX_POLL_ITEMS).contains(&policy.max_items)
      || !(1..=MAX_SOURCE_POLL_PAGE_BYTES).contains(&policy.max_page_bytes)
      || policy.max_bytes < policy.max_page_bytes
      || policy.max_bytes > MAX_POLL_BYTES
      || policy.reconciliation_interval_millis == 0
      || policy.reconciliation_interval_millis > i64::MAX as u64
      || policy.overlap_millis > i64::MAX as u64
      || policy.retention_millis == 0
      || policy.retention_millis > octacity_server_store::MAX_SOURCE_RETENTION_MILLIS
    {
      return Err(ApplicationError::invalid());
    }
    Ok(Self {
      store,
      reader,
      processor,
      clock,
      policy,
    })
  }
  /// Retains a complete page before processing; restart resumes its original remaining snapshots.
  pub async fn run_once(&self, scan: WorkSourceScan) -> Result<SourcePollOutcome, ApplicationError> {
    let mut outcome = SourcePollOutcome::default();
    let mut charged = false;
    let mut expected = scan.progress;
    let now = self.clock.now()?;
    require_time(&scan.claim, now)?;
    let mut progress = expected.clone().unwrap_or(WorkSourceProgress {
      polling: None,
      window_start: Timestamp::from_unix_millis(0).map_err(|_| ApplicationError::invalid())?,
      window_end: now,
      continuation: None,
      reconciliation: false,
    });
    if progress
      .polling
      .as_ref()
      .is_none_or(|state| state.pending_page.as_ref().is_some_and(|page| page.expires_at <= now))
    {
      // A legacy cursor has no original operation context; an expired tail is no longer covered.
      // Both restart conservatively and rely on the existing admission identity for replay.
      progress.window_start = Timestamp::from_unix_millis(0).map_err(|_| ApplicationError::invalid())?;
      progress.window_end = now;
      progress.continuation = None;
      progress.reconciliation = false;
      progress.polling = Some(WorkSourcePollingState {
        other_pass: None,
        reconciliation_due_at: Timestamp::from_unix_millis(
          now
            .unix_millis()
            .checked_add(self.policy.reconciliation_interval_millis as i64)
            .ok_or_else(ApplicationError::invalid)?,
        )
        .map_err(|_| ApplicationError::invalid())?,
        switch_after_page: false,
        scan_id: pass(&scan.claim, progress.window_start, now, false)?.scan_id,
        finished: false,
        pending_page: None,
      });
      self.save(&scan.claim, &mut expected, &progress).await?;
    }
    while outcome.items_processed < self.policy.max_items {
      require_time(&scan.claim, self.clock.now()?)?;
      if progress.polling.as_ref().unwrap().pending_page.is_none() {
        if outcome.pages_read >= self.policy.max_pages {
          break;
        }
        if u64::from(outcome.bytes_examined) + u64::from(self.policy.max_page_bytes) > u64::from(self.policy.max_bytes)
        {
          break;
        }
        if !self.advance_phase(&scan.claim, &mut progress, self.clock.now()?)? {
          break;
        }
        if expected.as_ref() != Some(&progress) {
          self.save(&scan.claim, &mut expected, &progress).await?;
        }
        let page = self
          .reader
          .read(SourcePollRequest {
            claim: scan.claim.clone(),
            scan_id: progress.polling.as_ref().unwrap().scan_id,
            window: if progress.reconciliation {
              None
            } else {
              Some((progress.window_start, progress.window_end))
            },
            continuation: progress.continuation.clone(),
            max_items: self.policy.page_items,
            max_page_bytes: self.policy.max_page_bytes,
          })
          .await?;
        let page_bytes = serde_json::to_vec(&page)
          .map_err(|_| ApplicationError::invalid())?
          .len();
        if page_bytes > self.policy.max_page_bytes as usize
          || page.candidates.len() > usize::from(self.policy.page_items)
        {
          return Err(ApplicationError::invalid());
        }
        outcome.bytes_examined += page_bytes as u32;
        charged = true;
        outcome.pages_read += 1;
        let candidates = page
          .candidates
          .into_iter()
          .map(serde_json::to_value)
          .collect::<Result<_, _>>()
          .map_err(|_| ApplicationError::invalid())?;
        progress.polling.as_mut().unwrap().pending_page = Some(WorkSourcePendingPage {
          candidates,
          next_cursor: page.next_cursor,
          expires_at: Timestamp::from_unix_millis(
            self
              .clock
              .now()?
              .unix_millis()
              .checked_add(self.policy.retention_millis as i64)
              .ok_or_else(ApplicationError::invalid)?,
          )
          .map_err(|_| ApplicationError::invalid())?,
        });
        self.save(&scan.claim, &mut expected, &progress).await?;
      }
      let pending = progress.polling.as_mut().unwrap().pending_page.as_mut().unwrap();
      if !charged {
        let bytes = pending.encoded_payload_bytes()?;
        if bytes > self.policy.max_page_bytes as usize || bytes > self.policy.max_bytes as usize {
          return Err(ApplicationError::invalid());
        }
        outcome.bytes_examined = bytes as u32;
        charged = true;
      }
      if !pending.candidates.is_empty() {
        if pending.expires_at <= self.clock.now()? {
          return Err(ApplicationError::invalid());
        }
        require_time(&scan.claim, self.clock.now()?)?;
        let snapshot =
          serde_json::from_value(pending.candidates[0].clone()).map_err(|_| ApplicationError::invalid())?;
        self.processor.settle(scan.claim.clone(), snapshot).await?;
        pending.candidates.remove(0);
        outcome.items_processed += 1;
      }
      if pending.candidates.is_empty() {
        progress.continuation = pending.next_cursor.clone();
        let state = progress.polling.as_mut().unwrap();
        state.pending_page = None;
        state.finished = progress.continuation.is_none();
        state.switch_after_page = true;
        if progress.reconciliation && state.finished {
          state.reconciliation_due_at = Timestamp::from_unix_millis(
            self
              .clock
              .now()?
              .unix_millis()
              .checked_add(self.policy.reconciliation_interval_millis as i64)
              .ok_or_else(ApplicationError::invalid)?,
          )
          .map_err(|_| ApplicationError::invalid())?;
        }
        charged = false;
      }
      self.save(&scan.claim, &mut expected, &progress).await?;
    }
    Ok(outcome)
  }
  fn advance_phase(
    &self,
    claim: &WorkSourceScanClaim,
    progress: &mut WorkSourceProgress,
    now: Timestamp,
  ) -> Result<bool, ApplicationError> {
    let state = progress.polling.as_ref().unwrap();
    let switch = state.switch_after_page;
    if progress.reconciliation && (switch || state.finished) {
      let mut next = state.other_pass.clone().ok_or_else(ApplicationError::invalid)?;
      if next.finished {
        if now <= next.window_end {
          return Ok(!state.finished);
        }
        next = pass(claim, overlap(next.window_end, self.policy.overlap_millis)?, now, false)?;
      }
      let previous = capture(progress);
      activate(progress, next);
      progress.polling.as_mut().unwrap().other_pass = Some(previous);
    } else if !progress.reconciliation
      && switch
      && (now >= state.reconciliation_due_at || state.other_pass.as_ref().is_some_and(|pass| !pass.finished))
    {
      let next = match &state.other_pass {
        Some(pass) if !pass.finished => pass.clone(),
        _ => pass(
          claim,
          Timestamp::from_unix_millis(0).map_err(|_| ApplicationError::invalid())?,
          now,
          true,
        )?,
      };
      let previous = capture(progress);
      activate(progress, next);
      progress.polling.as_mut().unwrap().other_pass = Some(previous);
    }
    let state = progress.polling.as_ref().unwrap();
    if state.finished {
      if now <= progress.window_end {
        return Ok(false);
      }
      let next = pass(
        claim,
        overlap(progress.window_end, self.policy.overlap_millis)?,
        now,
        false,
      )?;
      activate(progress, next);
    }
    progress.polling.as_mut().unwrap().switch_after_page = false;
    Ok(true)
  }
  async fn save(
    &self,
    claim: &WorkSourceScanClaim,
    expected: &mut Option<WorkSourceProgress>,
    progress: &WorkSourceProgress,
  ) -> Result<(), ApplicationError> {
    self
      .store
      .checkpoint_source_scan(claim.clone(), self.clock.now()?, expected.clone(), progress.clone())
      .await?;
    *expected = Some(progress.clone());
    Ok(())
  }
}
fn require_time(claim: &WorkSourceScanClaim, now: Timestamp) -> Result<(), ApplicationError> {
  if now < claim.acquired_at || now >= claim.until {
    return Err(octacity_server_store::source_conflict().into());
  }
  Ok(())
}
fn overlap(end: Timestamp, millis: u64) -> Result<Timestamp, ApplicationError> {
  Timestamp::from_unix_millis(end.unix_millis().saturating_sub(millis as i64).max(0))
    .map_err(|_| ApplicationError::invalid())
}
fn pass(
  claim: &WorkSourceScanClaim,
  start: Timestamp,
  end: Timestamp,
  reconciliation: bool,
) -> Result<WorkSourcePollPass, ApplicationError> {
  Ok(WorkSourcePollPass {
    scan_id: FactoryDigest::sha256(
      "octacity.factory.source-poll.v1",
      &[
        claim.project_id.to_string().as_bytes(),
        &serde_json::to_vec(&claim.target).map_err(|_| ApplicationError::invalid())?,
        &start.unix_millis().to_be_bytes(),
        &end.unix_millis().to_be_bytes(),
        &[u8::from(reconciliation)],
      ],
    ),
    window_start: start,
    window_end: end,
    continuation: None,
    reconciliation,
    finished: false,
  })
}
fn capture(progress: &WorkSourceProgress) -> WorkSourcePollPass {
  let state = progress.polling.as_ref().unwrap();
  WorkSourcePollPass {
    scan_id: state.scan_id,
    window_start: progress.window_start,
    window_end: progress.window_end,
    continuation: progress.continuation.clone(),
    reconciliation: progress.reconciliation,
    finished: state.finished,
  }
}
fn activate(progress: &mut WorkSourceProgress, pass: WorkSourcePollPass) {
  progress.window_start = pass.window_start;
  progress.window_end = pass.window_end;
  progress.continuation = pass.continuation;
  progress.reconciliation = pass.reconciliation;
  let state = progress.polling.as_mut().unwrap();
  state.scan_id = pass.scan_id;
  state.finished = pass.finished;
}
