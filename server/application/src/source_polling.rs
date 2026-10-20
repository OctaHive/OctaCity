//! Shared bounded page/reconciliation driver; consumers own authority and settlement.
use crate::{ApplicationError, FactoryExecutionClock, SourcePollOutcome, SourcePollingPolicy};
use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::FactoryDigest;
use octacity_server_store::{WorkSourcePendingPage, WorkSourcePollPass, WorkSourcePollingState, WorkSourceProgress};
#[derive(Clone)]
pub(crate) struct SourcePollingLease {
  pub project_id: ProjectId,
  pub target: Vec<u8>,
  pub acquired_at: Timestamp,
  pub until: Timestamp,
}
pub(crate) struct SourcePollingRequest {
  pub scan_id: FactoryDigest,
  pub window: Option<(Timestamp, Timestamp)>,
  pub continuation: Option<String>,
  pub max_items: u16,
  pub max_page_bytes: u32,
}
#[derive(serde::Serialize)]
pub(crate) struct SourcePollingPage {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub expires_at: Option<Timestamp>,
  pub candidates: Vec<serde_json::Value>,
  pub next_cursor: Option<String>,
}
#[async_trait::async_trait]
pub(crate) trait SourcePollingSession: Send + Sync {
  /// A targeted consumer can stop after its original receipt and covered checkpoint commit.
  fn complete(&self) -> bool {
    false
  }

  async fn read(&self, request: SourcePollingRequest) -> Result<SourcePollingPage, ApplicationError>;
  async fn settle(&self, candidate: serde_json::Value) -> Result<(), ApplicationError>;
  async fn checkpoint(
    &self,
    at: Timestamp,
    expected: Option<WorkSourceProgress>,
    progress: WorkSourceProgress,
  ) -> Result<(), ApplicationError>;
}
pub(crate) struct SourcePollingDriver<'a, S> {
  pub session: &'a S,
  pub clock: &'a dyn FactoryExecutionClock,
  pub policy: &'a SourcePollingPolicy,
  pub lease: SourcePollingLease,
}
impl<S: SourcePollingSession> SourcePollingDriver<'_, S> {
  /// Retains a complete page before processing; restart resumes its original remaining snapshots.
  pub async fn run_once(&self, progress: Option<WorkSourceProgress>) -> Result<SourcePollOutcome, ApplicationError> {
    let mut outcome = SourcePollOutcome::default();
    let mut charged = false;
    let mut expected = progress;
    let now = self.clock.now()?;
    require_time(&self.lease, now)?;
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
        scan_id: pass(&self.lease, progress.window_start, now, false)?.scan_id,
        finished: false,
        pending_page: None,
      });
      self.save(&self.lease, &mut expected, &progress).await?;
    }
    while outcome.items_processed < self.policy.max_items {
      require_time(&self.lease, self.clock.now()?)?;
      if progress.polling.as_ref().unwrap().pending_page.is_none() {
        if outcome.pages_read >= self.policy.max_pages {
          break;
        }
        if u64::from(outcome.bytes_examined) + u64::from(self.policy.max_page_bytes) > u64::from(self.policy.max_bytes)
        {
          break;
        }
        if !self.advance_phase(&self.lease, &mut progress, self.clock.now()?)? {
          break;
        }
        if expected.as_ref() != Some(&progress) {
          self.save(&self.lease, &mut expected, &progress).await?;
        }
        let page = self
          .session
          .read(SourcePollingRequest {
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
        let retention = Timestamp::from_unix_millis(
          self
            .clock
            .now()?
            .unix_millis()
            .checked_add(self.policy.retention_millis as i64)
            .ok_or_else(ApplicationError::invalid)?,
        )
        .map_err(|_| ApplicationError::invalid())?;
        progress.polling.as_mut().unwrap().pending_page = Some(WorkSourcePendingPage {
          candidates,
          next_cursor: page.next_cursor,
          expires_at: page.expires_at.map_or(retention, |original| original.min(retention)),
        });
        self.save(&self.lease, &mut expected, &progress).await?;
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
        require_time(&self.lease, self.clock.now()?)?;
        self.session.settle(pending.candidates[0].clone()).await?;
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
      self.save(&self.lease, &mut expected, &progress).await?;
      if self.session.complete() {
        break;
      }
    }
    Ok(outcome)
  }
  fn advance_phase(
    &self,
    claim: &SourcePollingLease,
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
    _claim: &SourcePollingLease,
    expected: &mut Option<WorkSourceProgress>,
    progress: &WorkSourceProgress,
  ) -> Result<(), ApplicationError> {
    self
      .session
      .checkpoint(self.clock.now()?, expected.clone(), progress.clone())
      .await?;
    *expected = Some(progress.clone());
    Ok(())
  }
}
fn require_time(claim: &SourcePollingLease, now: Timestamp) -> Result<(), ApplicationError> {
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
  claim: &SourcePollingLease,
  start: Timestamp,
  end: Timestamp,
  reconciliation: bool,
) -> Result<WorkSourcePollPass, ApplicationError> {
  Ok(WorkSourcePollPass {
    scan_id: FactoryDigest::sha256(
      "octacity.factory.source-poll.v1",
      &[
        claim.project_id.to_string().as_bytes(),
        &claim.target,
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

/// Shared finite cadence/backoff and provider retry hints for Source consumers.
/// Cancellation/capacity/retention do not spend a dependency attempt.
pub fn source_scan_schedule(
  previous: Option<&octacity_server_store::WorkSourceSchedule>,
  at: Timestamp,
  interval: std::time::Duration,
  retry: crate::DurableRetryPolicy,
  error: Option<&ApplicationError>,
) -> Result<octacity_server_store::WorkSourceSchedule, ApplicationError> {
  use octacity_server_store::{StoreError, WorkSourceFailure, WorkSourceSchedule};
  let add = |millis: u64| {
    Timestamp::from_unix_millis(
      at.unix_millis()
        .checked_add(i64::try_from(millis).map_err(|_| ApplicationError::invalid())?)
        .ok_or_else(ApplicationError::invalid)?,
    )
    .map_err(|_| ApplicationError::invalid())
  };
  let interval = u64::try_from(interval.as_millis()).map_err(|_| ApplicationError::invalid())?;
  if interval == 0 {
    return Err(ApplicationError::invalid());
  }
  let Some(error) = error else {
    return Ok(WorkSourceSchedule {
      retry_at: add(interval)?,
      attempts: 0,
      failure: None,
      exhausted: false,
    });
  };
  let (class, hint) = match error {
    ApplicationError::SourceDependency {
      class,
      retry_after_millis,
    } => (*class, *retry_after_millis),
    ApplicationError::CapabilityUnavailable => (WorkSourceFailure::Unavailable, None),
    ApplicationError::Store(StoreError::Unavailable) => (WorkSourceFailure::Transient, None),
    ApplicationError::Store(StoreError::Conflict {
      entity: octacity_server_domain::EntityKind::FactoryRun,
    }) => (WorkSourceFailure::AdmissionCapacity, None),
    ApplicationError::Store(_) => (WorkSourceFailure::Conflict, None),
    _ => (WorkSourceFailure::InvalidConfiguration, None),
  };
  let previous = previous.map_or(0, |v| v.attempts);
  let neutral = matches!(
    class,
    WorkSourceFailure::Cancelled | WorkSourceFailure::AdmissionCapacity | WorkSourceFailure::RetentionPending
  );
  let attempts = if neutral { previous } else { previous.saturating_add(1) };
  let earliest = hint.map(|v| add(u64::from(v))).transpose()?;
  let next = if neutral {
    Some(add(interval)?)
  } else {
    retry.retry_not_before(attempts, at, earliest)
  };
  Ok(WorkSourceSchedule {
    retry_at: next.unwrap_or(at),
    attempts,
    failure: Some(class),
    exhausted: next.is_none()
      || matches!(
        class,
        WorkSourceFailure::InvalidConfiguration | WorkSourceFailure::ProtocolFault | WorkSourceFailure::Conflict
      ),
  })
}
