use octacity_server_domain::{EntityKind, Timestamp};
use octacity_server_factory::{
  BudgetUsage, DeliveryIntent, Escalation, EscalationId, EvaluationBranchState, EvaluationState, FactoryClaimOwnership,
  FactoryDecisionGuard, FactoryDigest, FactoryError, FactoryKey, FactoryLifecycleProgress, FactoryLifecycleSnapshot,
  FactoryNextAction, FactoryRun, FactoryRunState, FactoryRunVersion, FactoryStageProgress, FactoryStageTarget,
  FactoryText, ReportingProgress, StageAttempt, StageAttemptId, StageAttemptNumber, decide_next_action,
};
use octacity_server_store::{
  AuditActorKind, ClaimFactoryRuns, ClaimedFactoryRun, CommitFactoryRunTransition, FactoryAuditFact,
  FactoryBudgetRecord, FactoryConfigurationStore, FactoryLifecycleCheckpoint, FactoryOutboxRecord,
  FactoryRunHistoryAppend, FactoryRunSnapshot, FactoryRunStore, MAX_FACTORY_RECONCILIATION_BATCH_SIZE, StoreError,
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
  time::{SystemTime, UNIX_EPOCH},
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
  /// The authoritative wall clock could not be represented by the domain timestamp.
  #[error("Factory reconciliation clock is unavailable")]
  ClockUnavailable,
  /// The pure Factory decision rejected inconsistent authoritative facts.
  #[error("Factory lifecycle decision failed")]
  Decision(#[from] FactoryError),
  /// The authoritative store rejected or could not complete an operation.
  #[error("Factory reconciliation store failed")]
  Store(#[from] StoreError),
}

/// Restart-safe worker that records at most one pure next action per claimed Run.
pub struct FactoryReconciler<S> {
  store: Arc<S>,
  owner: FactoryKey,
  batch_size: NonZeroU16,
  concurrency: NonZeroU16,
  clock: Arc<dyn FactoryReconciliationClock>,
}

impl<S> FactoryReconciler<S>
where
  S: FactoryRunStore + FactoryConfigurationStore + 'static,
{
  /// Creates a worker with explicit bounded claim and processing limits.
  pub fn new(
    store: Arc<S>,
    owner: FactoryKey,
    batch_size: u16,
    concurrency: u16,
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
      clock: Arc::new(SystemFactoryReconciliationClock),
    })
  }

  #[cfg(test)]
  pub(super) fn with_clock(
    store: Arc<S>,
    owner: FactoryKey,
    batch_size: u16,
    concurrency: u16,
    clock: Arc<dyn FactoryReconciliationClock>,
  ) -> Result<Self, FactoryReconciliationError> {
    let mut reconciler = Self::new(store, owner, batch_size, concurrency)?;
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
    );
    outcome.shutdown_skipped += queue.fill(shutdown);
    let mut first_error = None;
    while let Some(result) = queue.next().await {
      match result {
        Ok(ReconcileOne::Committed) => outcome.actions_committed += 1,
        Ok(ReconcileOne::Replayed) => outcome.actions_replayed += 1,
        Ok(ReconcileOne::Waiting) => outcome.waiting += 1,
        Err(error) => {
          first_error.get_or_insert(error);
        }
      }
      if first_error.is_none() {
        outcome.shutdown_skipped += queue.fill(shutdown);
      }
    }
    first_error.map_or(Ok(outcome), Err)
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReconcileOne {
  Committed,
  Replayed,
  Waiting,
}

type ReconciliationFuture = Pin<Box<dyn Future<Output = Result<ReconcileOne, FactoryReconciliationError>> + Send>>;

struct ReconciliationQueue<S> {
  store: Arc<S>,
  pending: std::vec::IntoIter<ClaimedFactoryRun>,
  in_flight: Vec<ReconciliationFuture>,
  clock: Arc<dyn FactoryReconciliationClock>,
  concurrency: usize,
}

impl<S> ReconciliationQueue<S>
where
  S: FactoryRunStore + FactoryConfigurationStore + 'static,
{
  fn new(
    store: Arc<S>,
    claims: Vec<ClaimedFactoryRun>,
    clock: Arc<dyn FactoryReconciliationClock>,
    concurrency: NonZeroU16,
  ) -> Self {
    Self {
      store,
      pending: claims.into_iter(),
      in_flight: Vec::with_capacity(usize::from(concurrency.get())),
      clock,
      concurrency: usize::from(concurrency.get()),
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
      self.in_flight.push(Box::pin(async move {
        reconcile_one(store.as_ref(), claim, clock.as_ref()).await
      }));
    }
    if shutdown.is_requested() {
      u16::try_from(self.pending.by_ref().count()).unwrap_or(u16::MAX)
    } else {
      0
    }
  }

  async fn next(&mut self) -> Option<Result<ReconcileOne, FactoryReconciliationError>> {
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
  clock: &dyn FactoryReconciliationClock,
) -> Result<ReconcileOne, FactoryReconciliationError>
where
  S: FactoryRunStore + FactoryConfigurationStore,
{
  let snapshot = store.factory_run_snapshot(claimed.run_id).await?;
  validate_claimed_snapshot(&snapshot, &claimed)?;
  let configuration = store
    .factory_configuration_version(
      snapshot.run.configuration().id(),
      snapshot.run.configuration().version(),
    )
    .await?;
  if configuration.configuration.reference() != snapshot.run.configuration() {
    return Err(FactoryReconciliationError::InvalidSnapshot);
  }
  let checkpoint = snapshot
    .lifecycle_checkpoints
    .iter()
    .find(|checkpoint| checkpoint.id == snapshot.current.lifecycle_checkpoint_id)
    .ok_or(FactoryReconciliationError::InvalidSnapshot)?;
  let budget = snapshot
    .budgets
    .iter()
    .find(|budget| budget.id == snapshot.current.budget_id)
    .ok_or(FactoryReconciliationError::InvalidSnapshot)?;
  let decision_at = clock.now()?;
  let guard = FactoryDecisionGuard::new(
    claimed.record.claim,
    claimed.record.claim.fence(),
    decision_at,
    configuration.configuration.hard_budget(),
    budget.usage,
    configuration.configuration.wip_limits(),
    claimed.wip_usage,
  );
  let decision = decide_next_action(&FactoryLifecycleSnapshot::new(
    snapshot.run.state(),
    checkpoint.progress.clone(),
    checkpoint.signal,
    guard,
    checkpoint.cancellation_requested,
  ))?;
  if matches!(decision, FactoryNextAction::Wait(_)) {
    return Ok(ReconcileOne::Waiting);
  }

  let action_material = decision.canonical_key();
  let projection_material = projection_material(&snapshot);
  let configuration_digest = snapshot.run.configuration().definition_digest().as_bytes();
  let input_digest = FactoryDigest::sha256(
    "octacity.factory.next-action-input.v1",
    &[
      snapshot.run.id().as_uuid().as_bytes(),
      &configuration_digest,
      action_material.as_bytes(),
      projection_material.as_bytes(),
    ],
  );
  let operation_id = FactoryDigest::sha256(
    "octacity.factory.next-action-operation.v1",
    &[snapshot.run.id().as_uuid().as_bytes(), &input_digest.as_bytes()],
  );
  let outbox_kind = outbox_kind(&decision)?;
  if let Some(kind) = &outbox_kind
    && let Some(existing) = snapshot
      .outbox
      .iter()
      .find(|record| record.operation_id == operation_id)
  {
    if existing.run_id != snapshot.run.id() || &existing.kind != kind || existing.input_digest != input_digest {
      return Err(FactoryReconciliationError::InvalidSnapshot);
    }
    return Ok(ReconcileOne::Replayed);
  }

  let next_version = snapshot
    .run
    .version()
    .get()
    .checked_add(1)
    .and_then(|value| FactoryRunVersion::new(value).ok())
    .ok_or(FactoryReconciliationError::InvalidSnapshot)?;
  let (next_state, next_progress, next_usage, append, next_stage_id) = transition_for_action(
    &snapshot,
    &configuration.configuration,
    &claimed,
    &decision,
    input_digest,
    budget.usage,
  )?;
  let committed_at = clock.now()?;
  let next_run = FactoryRun::restore(
    snapshot.run.id(),
    snapshot.run.configuration().clone(),
    &snapshot.work,
    snapshot.run.subject().clone(),
    next_state,
    next_version,
  )?;
  let next_budget = FactoryBudgetRecord::new(snapshot.run.id(), next_version, next_usage, committed_at);
  let next_checkpoint = FactoryLifecycleCheckpoint::new(
    snapshot.run.id(),
    next_version,
    next_progress,
    checkpoint.signal,
    checkpoint.cancellation_requested,
    committed_at,
  );
  let mut current = snapshot.current;
  current.budget_id = next_budget.id;
  current.lifecycle_checkpoint_id = next_checkpoint.id;
  if let Some(stage_id) = next_stage_id {
    current.stage_attempt_id = Some(stage_id);
    current.build_id = None;
  }
  if let Some(escalation) = append.escalations.first() {
    current.escalation_id = Some(escalation.id());
  }
  let owner_digest = FactoryDigest::sha256(
    "octacity.factory.audit-worker.v1",
    &[claimed.record.owner.as_str().as_bytes()],
  );
  store
    .commit_factory_run_transition(CommitFactoryRunTransition {
      run_id: next_run.id(),
      expected_version: claimed.expected_version,
      claim_id: claimed.record.id,
      owner: claimed.record.owner,
      fence: claimed.record.claim.fence(),
      committed_at,
      next_run,
      budget: next_budget,
      lifecycle_checkpoint: next_checkpoint,
      append,
      current,
      audit: FactoryAuditFact::new(
        snapshot.run.id(),
        AuditActorKind::Worker,
        Some(owner_digest),
        key("factory.next-action.recorded")?,
        operation_id,
        key("accepted")?,
        committed_at,
      ),
      outbox: outbox_kind
        .map(|kind| {
          FactoryOutboxRecord::pending(
            operation_id,
            snapshot.run.id(),
            kind,
            input_digest,
            committed_at,
            committed_at,
          )
        })
        .into_iter()
        .collect(),
    })
    .await?;
  Ok(ReconcileOne::Committed)
}

fn transition_for_action(
  snapshot: &FactoryRunSnapshot,
  configuration: &octacity_server_factory::FactoryConfiguration,
  claimed: &ClaimedFactoryRun,
  action: &FactoryNextAction,
  input_digest: FactoryDigest,
  usage: BudgetUsage,
) -> Result<
  (
    FactoryRunState,
    FactoryLifecycleProgress,
    BudgetUsage,
    FactoryRunHistoryAppend,
    Option<StageAttemptId>,
  ),
  FactoryReconciliationError,
> {
  let checkpoint = snapshot
    .lifecycle_checkpoints
    .iter()
    .find(|checkpoint| checkpoint.id == snapshot.current.lifecycle_checkpoint_id)
    .ok_or(FactoryReconciliationError::InvalidSnapshot)?;
  let target = match action {
    FactoryNextAction::CreateStageAttempt(target) | FactoryNextAction::RetryStage(target) => target,
    FactoryNextAction::RequestRework => {
      if !configuration
        .stages()
        .iter()
        .any(|stage| stage.kind() == octacity_server_factory::FactoryStageKind::Rework)
      {
        return Err(FactoryReconciliationError::InvalidSnapshot);
      }
      return Ok((
        FactoryRunState::Reworking,
        FactoryLifecycleProgress::Stage {
          target: FactoryStageTarget::Rework,
          progress: FactoryStageProgress::Ready,
        },
        usage,
        FactoryRunHistoryAppend::default(),
        None,
      ));
    }
    FactoryNextAction::Escalate(reason) => {
      let decision = if matches!(reason, octacity_server_factory::FactoryEscalationReason::Decision) {
        let id = snapshot
          .current
          .decision_id
          .ok_or(FactoryReconciliationError::InvalidSnapshot)?;
        Some(
          snapshot
            .decisions
            .iter()
            .find(|decision| decision.id() == id)
            .ok_or(FactoryReconciliationError::InvalidSnapshot)?,
        )
      } else {
        None
      };
      let escalation = Escalation::new(
        EscalationId::generate(),
        &snapshot.run,
        decision,
        FactoryText::new(action.canonical_key())?,
      )?;
      return Ok((
        FactoryRunState::Escalated,
        FactoryLifecycleProgress::Escalated(ReportingProgress::Ready),
        usage,
        FactoryRunHistoryAppend {
          escalations: vec![escalation],
          ..FactoryRunHistoryAppend::default()
        },
        None,
      ));
    }
    FactoryNextAction::Reject => {
      return Ok((
        FactoryRunState::Rejected,
        FactoryLifecycleProgress::Rejected(ReportingProgress::Ready),
        usage,
        FactoryRunHistoryAppend::default(),
        None,
      ));
    }
    FactoryNextAction::Cancel => {
      return Ok((
        FactoryRunState::Cancelled,
        FactoryLifecycleProgress::Cancelled(ReportingProgress::Ready),
        usage,
        FactoryRunHistoryAppend::default(),
        None,
      ));
    }
    FactoryNextAction::PrepareDelivery => {
      return Ok((
        FactoryRunState::ReadyForDelivery,
        FactoryLifecycleProgress::ReadyForDelivery(DeliveryIntent::AwaitingApproval),
        usage,
        FactoryRunHistoryAppend::default(),
        None,
      ));
    }
    FactoryNextAction::Complete => {
      return Ok((
        FactoryRunState::Completed,
        FactoryLifecycleProgress::Completed,
        usage,
        FactoryRunHistoryAppend::default(),
        None,
      ));
    }
    FactoryNextAction::Wait(_) => return Err(FactoryReconciliationError::InvalidSnapshot),
    FactoryNextAction::RequestDecisionSignal(_)
    | FactoryNextAction::ConsumeDecisionSignal
    | FactoryNextAction::CreateBuild(_)
    | FactoryNextAction::CaptureCandidate
    | FactoryNextAction::ConstructEvidence
    | FactoryNextAction::PlanEvaluations
    | FactoryNextAction::Decide
    | FactoryNextAction::RequestDelivery
    | FactoryNextAction::Report => {
      return Ok((
        snapshot.run.state(),
        checkpoint.progress.clone(),
        usage,
        FactoryRunHistoryAppend::default(),
        None,
      ));
    }
  };
  let stage_definition = configuration
    .stages()
    .iter()
    .find(|stage| stage.kind() == target.kind())
    .ok_or(FactoryReconciliationError::InvalidSnapshot)?;
  let number = snapshot
    .stage_attempts
    .iter()
    .map(StageAttempt::number)
    .map(StageAttemptNumber::get)
    .max()
    .unwrap_or(0)
    .checked_add(1)
    .and_then(|value| StageAttemptNumber::new(value).ok())
    .ok_or(FactoryReconciliationError::InvalidSnapshot)?;
  let attempts = usage
    .attempts
    .checked_add(1)
    .ok_or(FactoryReconciliationError::InvalidSnapshot)?;
  let next_usage = BudgetUsage { attempts, ..usage };
  next_usage.validate(configuration.hard_budget())?;
  let stage = StageAttempt::new(
    StageAttemptId::generate(),
    &snapshot.run,
    number,
    target.kind(),
    stage_definition.budget(),
    input_digest,
    FactoryClaimOwnership::new(claimed.record.owner.clone(), claimed.record.claim),
  );
  let (state, progress) = match target {
    FactoryStageTarget::Implementation => (
      FactoryRunState::Implementing,
      FactoryLifecycleProgress::Stage {
        target: target.clone(),
        progress: FactoryStageProgress::AttemptCreated,
      },
    ),
    FactoryStageTarget::Validation => (
      FactoryRunState::Validating,
      FactoryLifecycleProgress::Stage {
        target: target.clone(),
        progress: FactoryStageProgress::AttemptCreated,
      },
    ),
    FactoryStageTarget::Rework => (
      FactoryRunState::Reworking,
      FactoryLifecycleProgress::Stage {
        target: target.clone(),
        progress: FactoryStageProgress::AttemptCreated,
      },
    ),
    FactoryStageTarget::Evaluation(key) => {
      let FactoryLifecycleProgress::Evaluating(EvaluationState::Branches(branches)) = &checkpoint.progress else {
        return Err(FactoryReconciliationError::InvalidSnapshot);
      };
      let branches = branches.advance_branch(key, EvaluationBranchState::AttemptCreated)?;
      (
        FactoryRunState::Evaluating,
        FactoryLifecycleProgress::Evaluating(EvaluationState::Branches(branches)),
      )
    }
  };
  let stage_id = stage.id();
  Ok((
    state,
    progress,
    next_usage,
    FactoryRunHistoryAppend {
      stage_attempts: vec![stage],
      ..FactoryRunHistoryAppend::default()
    },
    Some(stage_id),
  ))
}

fn outbox_kind(action: &FactoryNextAction) -> Result<Option<FactoryKey>, FactoryReconciliationError> {
  action.dispatch_key().map(key).transpose()
}

pub(super) trait FactoryReconciliationClock: Send + Sync {
  fn now(&self) -> Result<Timestamp, FactoryReconciliationError>;
}

struct SystemFactoryReconciliationClock;

impl FactoryReconciliationClock for SystemFactoryReconciliationClock {
  fn now(&self) -> Result<Timestamp, FactoryReconciliationError> {
    let milliseconds = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .ok()
      .and_then(|duration| i64::try_from(duration.as_millis()).ok())
      .ok_or(FactoryReconciliationError::ClockUnavailable)?;
    Timestamp::from_unix_millis(milliseconds).map_err(|_| FactoryReconciliationError::ClockUnavailable)
  }
}

fn validate_claimed_snapshot(
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

fn projection_material(snapshot: &FactoryRunSnapshot) -> String {
  let current = &snapshot.current;
  let mut value = format!("{}|", snapshot.run.state().as_str());
  for identity in [
    current.stage_attempt_id.map(|id| id.as_uuid()),
    current.macro_call_id.map(|id| id.as_uuid()),
    current.signal_request_id.map(|id| id.as_uuid()),
    current.signal_receipt_id.map(|id| id.as_uuid()),
    current.build_id.map(|id| id.as_uuid()),
    current.candidate_id.map(|id| id.as_uuid()),
    current.evidence_id.map(|id| id.as_uuid()),
    current.evaluation_plan_id.map(|id| id.as_uuid()),
    current.decision_id.map(|id| id.as_uuid()),
    current.escalation_id.map(|id| id.as_uuid()),
    current.delivery_attempt_id.map(|id| id.as_uuid()),
    current.reporting_attempt_id.map(|id| id.as_uuid()),
  ] {
    match identity {
      Some(identity) => value.push_str(&format!("{identity}|")),
      None => value.push_str("-|"),
    }
  }
  value
}

fn key(value: &str) -> Result<FactoryKey, FactoryReconciliationError> {
  FactoryKey::new(value).map_err(|_| FactoryReconciliationError::InvalidSnapshot)
}
