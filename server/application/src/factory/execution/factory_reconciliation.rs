use octacity_server_domain::{EntityKind, Timestamp};
use octacity_server_factory::{FactoryError, FactoryKey, FactoryRunId};
use octacity_server_store::{
  ClaimFactoryRuns, ClaimedFactoryRun, FactoryRunSnapshot, FactoryRunStore, MAX_FACTORY_RECONCILIATION_BATCH_SIZE,
  StoreError,
};
use std::{
  future::{Future, poll_fn},
  num::NonZeroU16,
  pin::Pin,
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
  task::Poll,
};
use thiserror::Error;

/// Runtime-neutral graceful-shutdown signal for Factory reconciliation.
#[derive(Debug, Default)]
pub struct FactoryReconciliationShutdown {
  requested: AtomicBool,
}

impl FactoryReconciliationShutdown {
  /// Creates a signal in the running state.
  #[must_use]
  pub const fn new() -> Self {
    Self {
      requested: AtomicBool::new(false),
    }
  }

  /// Prevents new claimed Runs from starting.
  pub fn request(&self) {
    self.requested.store(true, Ordering::Release);
  }

  /// Reports whether graceful shutdown has been requested.
  #[must_use]
  pub fn is_requested(&self) -> bool {
    self.requested.load(Ordering::Acquire)
  }
}

/// Summary of one bounded Factory reconciliation pass.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FactoryReconciliationBatchOutcome {
  /// Durable Run claims returned by the authoritative store.
  pub claimed: u16,
  /// Newly committed logical next actions.
  pub actions_committed: u16,
  /// Previously committed logical actions observed after replay or restart.
  pub actions_replayed: u16,
  /// Runs whose pure decision currently permits no mutation.
  pub waiting: u16,
  /// Individual Run failures retained for retry under their original claim expiry.
  /// These do not back off unrelated Runs; claim/selection failures still fail the pass.
  pub failed: u16,
  /// Claimed Runs intentionally left for claim expiry after shutdown began.
  pub shutdown_skipped: u16,
}

/// Failure from one bounded Factory reconciliation pass.
#[derive(Debug, Error)]
pub enum FactoryReconciliationError {
  /// Worker bounds are zero or inconsistent.
  #[error("Factory reconciler bounds are invalid")]
  InvalidBounds,
  /// Persisted state cannot form the complete decision snapshot it claims to represent.
  #[error("Factory reconciliation snapshot is invalid")]
  InvalidSnapshot,
  /// A configured flow executor or validation port failed without changing ordinary CI/CD.
  #[error("Factory flow failed")]
  Flow(#[from] crate::ApplicationError),
  /// The pure Factory decision rejected inconsistent authoritative facts.
  #[error("Factory lifecycle decision failed")]
  Decision(#[from] FactoryError),
  /// The authoritative store rejected or could not complete an operation.
  #[error("Factory reconciliation store failed")]
  Store(#[from] StoreError),
}

/// Restart-safe bounded worker that delegates every Run to its configured graph owner.
pub struct FactoryReconciler<S> {
  store: Arc<S>,
  owner: FactoryKey,
  batch_size: NonZeroU16,
  concurrency: NonZeroU16,
  clock: Arc<dyn crate::FactoryExecutionClock>,
  flow: Arc<dyn crate::FactoryFlowCoordinator>,
  phase_pools: Option<Arc<dyn crate::FactoryPhasePoolCoordination>>,
}

impl<S> FactoryReconciler<S>
where
  S: FactoryRunStore + 'static,
{
  /// Creates a worker with explicit bounds and a required configured graph owner.
  pub fn new(
    store: Arc<S>,
    owner: FactoryKey,
    batch_size: u16,
    concurrency: u16,
    flow: Arc<dyn crate::FactoryFlowCoordinator>,
  ) -> Result<Self, FactoryReconciliationError> {
    let batch_size = NonZeroU16::new(batch_size)
      .filter(|value| value.get() <= MAX_FACTORY_RECONCILIATION_BATCH_SIZE)
      .ok_or(FactoryReconciliationError::InvalidBounds)?;
    let concurrency = NonZeroU16::new(concurrency)
      .filter(|value| value.get() <= batch_size.get())
      .ok_or(FactoryReconciliationError::InvalidBounds)?;
    Ok(Self {
      store,
      owner,
      batch_size,
      concurrency,
      clock: Arc::new(crate::SystemFactoryExecutionClock),
      flow,
      phase_pools: None,
    })
  }

  /// Adds bounded native admission before claiming Runs. Disabled runtimes never install it.
  #[must_use]
  pub fn with_phase_pool_selector(mut self, selector: Arc<dyn crate::FactoryPhasePoolCoordination>) -> Self {
    self.phase_pools = Some(selector);
    self
  }

  #[cfg(test)]
  pub(crate) fn with_clock(
    store: Arc<S>,
    owner: FactoryKey,
    batch_size: u16,
    concurrency: u16,
    clock: Arc<dyn crate::FactoryExecutionClock>,
    flow: Arc<dyn crate::FactoryFlowCoordinator>,
  ) -> Result<Self, FactoryReconciliationError> {
    let mut reconciler = Self::new(store, owner, batch_size, concurrency, flow)?;
    reconciler.clock = clock;
    Ok(reconciler)
  }

  /// Claims and reconciles one bounded batch at caller-supplied authoritative times.
  ///
  /// Shutdown stops new claimed Runs from starting but does not cancel an
  /// in-flight authoritative transition, whose response may otherwise become
  /// ambiguous.
  pub async fn run_once(
    &self,
    observed_at: Timestamp,
    claim_expires_at: Timestamp,
    shutdown: &FactoryReconciliationShutdown,
  ) -> Result<FactoryReconciliationBatchOutcome, FactoryReconciliationError> {
    if shutdown.is_requested() {
      return Ok(FactoryReconciliationBatchOutcome::default());
    }
    if let Some(selector) = &self.phase_pools {
      selector.select_once(&self.owner, observed_at, claim_expires_at).await?;
    }
    let claims = self
      .store
      .claim_factory_runs(ClaimFactoryRuns::new(
        self.owner.clone(),
        observed_at,
        claim_expires_at,
        self.batch_size.get(),
      )?)
      .await?;
    let claimed = u16::try_from(claims.len()).map_err(|_| FactoryReconciliationError::InvalidSnapshot)?;
    let mut outcome = FactoryReconciliationBatchOutcome {
      claimed,
      ..FactoryReconciliationBatchOutcome::default()
    };
    let mut queue = ReconciliationQueue::new(
      Arc::clone(&self.store),
      claims,
      Arc::clone(&self.clock),
      self.concurrency,
      self.flow.clone(),
    );
    outcome.shutdown_skipped += queue.fill(shutdown);
    while let Some((run_id, result)) = queue.next().await {
      match result {
        Ok(ReconcileOne::Committed) => outcome.actions_committed += 1,
        Ok(ReconcileOne::Replayed) => outcome.actions_replayed += 1,
        Ok(ReconcileOne::Waiting) => outcome.waiting += 1,
        Err(error) => {
          outcome.failed += 1;
          let class = match &error {
            FactoryReconciliationError::Flow(_) => "flow",
            FactoryReconciliationError::Store(_) => "store",
            FactoryReconciliationError::Decision(_) => "decision",
            FactoryReconciliationError::InvalidSnapshot => "invalid_snapshot",
            FactoryReconciliationError::InvalidBounds => "invalid_bounds",
          };
          tracing::warn!(run_id = %run_id, error.class = class,
            "Factory Run reconciliation failed; retaining its claim until expiry");
        }
      }
      // Local errors retain their own lease. Backing off the whole worker can
      // repeatedly reclaim an old failing batch and starve newer Runs.
      outcome.shutdown_skipped += queue.fill(shutdown);
    }
    Ok(outcome)
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReconcileOne {
  Committed,
  Replayed,
  Waiting,
}

type ReconciliationFuture =
  Pin<Box<dyn Future<Output = (FactoryRunId, Result<ReconcileOne, FactoryReconciliationError>)> + Send>>;

struct ReconciliationQueue<S> {
  store: Arc<S>,
  pending: std::vec::IntoIter<ClaimedFactoryRun>,
  in_flight: Vec<ReconciliationFuture>,
  clock: Arc<dyn crate::FactoryExecutionClock>,
  concurrency: usize,
  flow: Arc<dyn crate::FactoryFlowCoordinator>,
}

impl<S> ReconciliationQueue<S>
where
  S: FactoryRunStore + 'static,
{
  fn new(
    store: Arc<S>,
    claims: Vec<ClaimedFactoryRun>,
    clock: Arc<dyn crate::FactoryExecutionClock>,
    concurrency: NonZeroU16,
    flow: Arc<dyn crate::FactoryFlowCoordinator>,
  ) -> Self {
    Self {
      store,
      pending: claims.into_iter(),
      in_flight: Vec::with_capacity(usize::from(concurrency.get())),
      clock,
      concurrency: usize::from(concurrency.get()),
      flow,
    }
  }

  fn fill(&mut self, shutdown: &FactoryReconciliationShutdown) -> u16 {
    if shutdown.is_requested() {
      return u16::try_from(self.pending.by_ref().count()).unwrap_or(u16::MAX);
    }
    while self.in_flight.len() < self.concurrency && !shutdown.is_requested() {
      let Some(claim) = self.pending.next() else {
        break;
      };
      let store = Arc::clone(&self.store);
      let clock = Arc::clone(&self.clock);
      let flow = self.flow.clone();
      self.in_flight.push(Box::pin(async move {
        (
          claim.run_id,
          reconcile_one(store.as_ref(), claim, clock.as_ref(), flow.as_ref()).await,
        )
      }));
    }
    if shutdown.is_requested() {
      u16::try_from(self.pending.by_ref().count()).unwrap_or(u16::MAX)
    } else {
      0
    }
  }

  async fn next(&mut self) -> Option<(FactoryRunId, Result<ReconcileOne, FactoryReconciliationError>)> {
    if self.in_flight.is_empty() {
      return None;
    }
    let (index, result) = poll_fn(|context| {
      for (index, future) in self.in_flight.iter_mut().enumerate() {
        if let Poll::Ready(result) = future.as_mut().poll(context) {
          return Poll::Ready((index, result));
        }
      }
      Poll::Pending
    })
    .await;
    drop(self.in_flight.swap_remove(index));
    Some(result)
  }
}

async fn reconcile_one<S>(
  store: &S,
  claimed: ClaimedFactoryRun,
  clock: &dyn crate::FactoryExecutionClock,
  flow: &dyn crate::FactoryFlowCoordinator,
) -> Result<ReconcileOne, FactoryReconciliationError>
where
  S: FactoryRunStore,
{
  let snapshot = store.factory_run_snapshot(claimed.run_id).await?;
  validate_claimed_snapshot(&snapshot, &claimed)?;
  match flow
    .reconcile_flow(snapshot.run.id(), claimed.record.id, clock.now()?)
    .await?
  {
    crate::FactoryFlowStep::Advanced | crate::FactoryFlowStep::Resolved(_) | crate::FactoryFlowStep::Cancelled => {
      let after = store.factory_run_snapshot(snapshot.run.id()).await?;
      Ok(if after.run.version() > snapshot.run.version() {
        ReconcileOne::Committed
      } else {
        ReconcileOne::Replayed
      })
    }
    crate::FactoryFlowStep::Waiting | crate::FactoryFlowStep::Ready(_) => Ok(ReconcileOne::Waiting),
  }
}

pub(crate) fn validate_claimed_snapshot(
  snapshot: &FactoryRunSnapshot,
  claimed: &ClaimedFactoryRun,
) -> Result<(), FactoryReconciliationError> {
  if snapshot.run.version() != claimed.expected_version
    || snapshot.current_claim.as_ref() != Some(&claimed.record)
    || snapshot.run.id() != claimed.run_id
  {
    return Err(FactoryReconciliationError::Store(StoreError::Conflict {
      entity: EntityKind::FactoryRun,
    }));
  }
  Ok(())
}
