use std::num::NonZeroU16;

use octacity_server_domain::{BuildId, Timestamp};
use octacity_server_factory::{
  Assessment, BudgetUsage, ChangeSet, Decision, DecisionSignalProgress, DecisionSignalReceipt, DecisionSignalRequest,
  DeliveryAttempt, Escalation, EvaluationPlan, EvidenceManifest, FactoryClaim, FactoryClaimFence, FactoryDigest,
  FactoryKey, FactoryLifecycleProgress, FactoryRun, FactoryRunId, FactoryRunVersion, FactoryWipUsage, MacroCall,
  ReportingAttempt, StageAttempt, StageAttemptCompletion, WorkEnvelope,
};

use crate::{AuditActorKind, MutationDisposition, StoreError};

/// Maximum immutable history rows appended by one Factory transition.
pub const MAX_FACTORY_TRANSITION_RECORDS: usize = 256;
/// Maximum outbox state rows appended by one Factory transition.
pub const MAX_FACTORY_TRANSITION_OUTBOX_RECORDS: usize = 32;
/// Maximum immutable rows loaded in one complete Factory Run snapshot.
pub const MAX_FACTORY_RUN_SNAPSHOT_RECORDS: usize = 32_768;
/// Maximum Factory Runs claimed by one bounded reconciliation pass.
pub const MAX_FACTORY_RECONCILIATION_BATCH_SIZE: u16 = 100;
/// Maximum Factory outbox operations claimed by one bounded dispatch pass.
pub const MAX_FACTORY_OUTBOX_BATCH_SIZE: u16 = 100;

/// Immutable ownership record appended whenever a reconciler acquires a Run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryRunClaimRecord {
  /// Content-derived identity of this claim row.
  pub id: FactoryDigest,
  /// Bounded process-instance identity.
  pub owner: FactoryKey,
  /// Exclusive fenced ownership window.
  pub claim: FactoryClaim,
}

impl FactoryRunClaimRecord {
  /// Constructs a content-addressed claim row for one Run.
  #[must_use]
  pub fn new(run_id: FactoryRunId, owner: FactoryKey, claim: FactoryClaim) -> Self {
    let claimed_at = claim.claimed_at().unix_millis().to_be_bytes();
    let expires_at = claim.expires_at().unix_millis().to_be_bytes();
    let id = FactoryDigest::sha256(
      "octacity.factory.claim.v1",
      &[
        run_id.as_uuid().as_bytes(),
        owner.as_str().as_bytes(),
        &claim.fence().digest().as_bytes(),
        &claimed_at,
        &expires_at,
      ],
    );
    Self { id, owner, claim }
  }
}

/// Immutable aggregate budget observation recorded at one Run version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryBudgetRecord {
  /// Content-derived identity of this budget row.
  pub id: FactoryDigest,
  /// Aggregate version at which this usage became current.
  pub run_version: FactoryRunVersion,
  /// Monotonic authoritative consumption.
  pub usage: BudgetUsage,
  /// Authoritative observation time.
  pub recorded_at: Timestamp,
}

impl FactoryBudgetRecord {
  /// Constructs a content-addressed aggregate budget observation.
  #[must_use]
  pub fn new(run_id: FactoryRunId, run_version: FactoryRunVersion, usage: BudgetUsage, recorded_at: Timestamp) -> Self {
    let version = run_version.get().to_be_bytes();
    let attempts = usage.attempts.to_be_bytes();
    let elapsed = usage.elapsed_millis.to_be_bytes();
    let tokens = usage.tokens.to_be_bytes();
    let cost = usage.cost_micro_units.to_be_bytes();
    let output = usage.output_bytes.to_be_bytes();
    let timestamp = recorded_at.unix_millis().to_be_bytes();
    let id = FactoryDigest::sha256(
      "octacity.factory.budget.v1",
      &[
        run_id.as_uuid().as_bytes(),
        &version,
        &attempts,
        &elapsed,
        &tokens,
        &cost,
        &output,
        &timestamp,
      ],
    );
    Self {
      id,
      run_version,
      usage,
      recorded_at,
    }
  }
}

/// Immutable lifecycle checkpoint published for one aggregate version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryLifecycleCheckpoint {
  /// Stable identity derived from the Run version and commit time.
  pub id: FactoryDigest,
  /// Factory Run owning this checkpoint.
  pub run_id: FactoryRunId,
  /// Aggregate version whose current state this checkpoint describes.
  pub run_version: FactoryRunVersion,
  /// Complete code-owned lifecycle progress.
  pub progress: FactoryLifecycleProgress,
  /// Current non-authoritative Decision Signal progress.
  pub signal: DecisionSignalProgress,
  /// Whether durable cancellation intent has been accepted.
  pub cancellation_requested: bool,
  /// Authoritative commit time.
  pub recorded_at: Timestamp,
}

impl FactoryLifecycleCheckpoint {
  /// Constructs the immutable checkpoint identity for one Run version.
  #[must_use]
  pub fn new(
    run_id: FactoryRunId,
    run_version: FactoryRunVersion,
    progress: FactoryLifecycleProgress,
    signal: DecisionSignalProgress,
    cancellation_requested: bool,
    recorded_at: Timestamp,
  ) -> Self {
    let version = run_version.get().to_be_bytes();
    let timestamp = recorded_at.unix_millis().to_be_bytes();
    let id = FactoryDigest::sha256(
      "octacity.factory.lifecycle-checkpoint.v1",
      &[run_id.as_uuid().as_bytes(), &version, &timestamp],
    );
    Self {
      id,
      run_id,
      run_version,
      progress,
      signal,
      cancellation_requested,
      recorded_at,
    }
  }
}

/// Immutable causal link from one Factory Stage Attempt to an ordinary Build.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryBuildLink {
  /// Factory Run owning the causal link.
  pub run_id: FactoryRunId,
  /// Stage Attempt that created or selected the Build.
  pub stage_attempt_id: octacity_server_factory::StageAttemptId,
  /// Ordinary immutable Build identity.
  pub build_id: BuildId,
  /// Digest of the exact Build creation input.
  pub input_digest: FactoryDigest,
}

impl FactoryBuildLink {
  /// Constructs one immutable ordinary-Build link from its producing stage.
  #[must_use]
  pub fn new(stage: &StageAttempt, build_id: BuildId, input_digest: FactoryDigest) -> Self {
    Self {
      run_id: stage.run_id(),
      stage_attempt_id: stage.id(),
      build_id,
      input_digest,
    }
  }
}

/// Immutable secret-safe audit fact committed with Factory state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryAuditFact {
  /// Content-derived immutable audit identity.
  pub id: FactoryDigest,
  /// Factory Run whose state changed.
  pub run_id: FactoryRunId,
  /// Trusted actor classification.
  pub actor_kind: AuditActorKind,
  /// Optional digest of the accepted actor identity, never the raw identity.
  pub actor_identity_digest: Option<FactoryDigest>,
  /// Stable bounded operation classification.
  pub operation: FactoryKey,
  /// Digest of the accepted request identity.
  pub request_identity_digest: FactoryDigest,
  /// Stable bounded outcome classification.
  pub outcome: FactoryKey,
  /// Authoritative commit time.
  pub recorded_at: Timestamp,
}

impl FactoryAuditFact {
  /// Constructs a content-addressed, secret-safe audit fact.
  #[must_use]
  pub fn new(
    run_id: FactoryRunId,
    actor_kind: AuditActorKind,
    actor_identity_digest: Option<FactoryDigest>,
    operation: FactoryKey,
    request_identity_digest: FactoryDigest,
    outcome: FactoryKey,
    recorded_at: Timestamp,
  ) -> Self {
    let timestamp = recorded_at.unix_millis().to_be_bytes();
    let actor_identity = actor_identity_digest.map_or([0_u8; 32], FactoryDigest::as_bytes);
    let actor_identity_present = [u8::from(actor_identity_digest.is_some())];
    let id = FactoryDigest::sha256(
      "octacity.factory.audit.v1",
      &[
        run_id.as_uuid().as_bytes(),
        actor_kind.as_str().as_bytes(),
        &actor_identity_present,
        &actor_identity,
        operation.as_str().as_bytes(),
        &request_identity_digest.as_bytes(),
        outcome.as_str().as_bytes(),
        &timestamp,
      ],
    );
    Self {
      id,
      run_id,
      actor_kind,
      actor_identity_digest,
      operation,
      request_identity_digest,
      outcome,
      recorded_at,
    }
  }
}

/// Append-only lifecycle state of one stable external side effect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FactoryOutboxState {
  /// Durable work is ready for dispatch.
  Pending,
  /// One worker owns the current bounded dispatch attempt.
  Claimed,
  /// The external effect was observed or completed idempotently.
  Delivered,
  /// Bounded retry policy was exhausted.
  Failed,
}

/// Immutable outbox state row for one stable logical side effect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryOutboxRecord {
  /// Unique immutable row identity.
  pub id: FactoryDigest,
  /// Stable identity shared by every state row for the logical side effect.
  pub operation_id: FactoryDigest,
  /// Factory Run that owns the side effect.
  pub run_id: FactoryRunId,
  /// Bounded side-effect kind.
  pub kind: FactoryKey,
  /// Digest of the complete immutable side-effect input.
  pub input_digest: FactoryDigest,
  /// Append-only state observed by workers.
  pub state: FactoryOutboxState,
  /// Zero-based dispatch attempt associated with this state row.
  pub attempt: u16,
  /// Earliest authoritative time at which work may be dispatched.
  pub available_at: Timestamp,
  /// Authoritative time at which this row was committed.
  pub recorded_at: Timestamp,
  /// Current dispatch owner for claimed and terminal state rows.
  pub owner: Option<FactoryKey>,
  /// Fenced dispatch window for claimed and terminal state rows.
  pub claim: Option<FactoryClaim>,
}

impl FactoryOutboxRecord {
  /// Constructs an initial pending outbox state row.
  #[must_use]
  pub fn pending(
    operation_id: FactoryDigest,
    run_id: FactoryRunId,
    kind: FactoryKey,
    input_digest: FactoryDigest,
    available_at: Timestamp,
    recorded_at: Timestamp,
  ) -> Self {
    Self::new_state(
      operation_id,
      run_id,
      kind,
      input_digest,
      FactoryOutboxState::Pending,
      0,
      available_at,
      recorded_at,
      None,
      None,
    )
  }

  /// Appends one fenced dispatch claim while preserving logical operation identity.
  #[must_use]
  pub fn claimed(
    previous: &Self,
    owner: FactoryKey,
    claim: FactoryClaim,
    attempt: u16,
    recorded_at: Timestamp,
  ) -> Self {
    Self::new_state(
      previous.operation_id,
      previous.run_id,
      previous.kind.clone(),
      previous.input_digest,
      FactoryOutboxState::Claimed,
      attempt,
      previous.available_at,
      recorded_at,
      Some(owner),
      Some(claim),
    )
  }

  /// Appends a delivered or terminally failed observation for one owned claim.
  #[must_use]
  pub fn terminal(previous: &Self, state: FactoryOutboxState, recorded_at: Timestamp) -> Self {
    debug_assert!(matches!(
      state,
      FactoryOutboxState::Delivered | FactoryOutboxState::Failed
    ));
    Self::new_state(
      previous.operation_id,
      previous.run_id,
      previous.kind.clone(),
      previous.input_digest,
      state,
      previous.attempt,
      previous.available_at,
      recorded_at,
      previous.owner.clone(),
      previous.claim,
    )
  }

  /// Appends a due retry after an owned claim produced no authoritative outcome.
  #[must_use]
  pub fn retry(previous: &Self, available_at: Timestamp, recorded_at: Timestamp) -> Option<Self> {
    let attempt = previous.attempt.checked_add(1)?;
    Some(Self::new_state(
      previous.operation_id,
      previous.run_id,
      previous.kind.clone(),
      previous.input_digest,
      FactoryOutboxState::Pending,
      attempt,
      available_at,
      recorded_at,
      None,
      None,
    ))
  }

  #[allow(clippy::too_many_arguments)]
  fn new_state(
    operation_id: FactoryDigest,
    run_id: FactoryRunId,
    kind: FactoryKey,
    input_digest: FactoryDigest,
    state: FactoryOutboxState,
    attempt: u16,
    available_at: Timestamp,
    recorded_at: Timestamp,
    owner: Option<FactoryKey>,
    claim: Option<FactoryClaim>,
  ) -> Self {
    let attempt_bytes = attempt.to_be_bytes();
    let available = available_at.unix_millis().to_be_bytes();
    let recorded = recorded_at.unix_millis().to_be_bytes();
    let state_name = match state {
      FactoryOutboxState::Pending => "pending",
      FactoryOutboxState::Claimed => "claimed",
      FactoryOutboxState::Delivered => "delivered",
      FactoryOutboxState::Failed => "failed",
    };
    let owner_value = owner.as_ref().map_or("", FactoryKey::as_str);
    let fence = claim.map_or([0_u8; 32], |claim| claim.fence().digest().as_bytes());
    let claimed_at = claim
      .map_or(0_i64, |claim| claim.claimed_at().unix_millis())
      .to_be_bytes();
    let expires_at = claim
      .map_or(0_i64, |claim| claim.expires_at().unix_millis())
      .to_be_bytes();
    let id = FactoryDigest::sha256(
      "octacity.factory.outbox.v1",
      &[
        &operation_id.as_bytes(),
        run_id.as_uuid().as_bytes(),
        kind.as_str().as_bytes(),
        &input_digest.as_bytes(),
        state_name.as_bytes(),
        &attempt_bytes,
        &available,
        &recorded,
        owner_value.as_bytes(),
        &fence,
        &claimed_at,
        &expires_at,
      ],
    );
    Self {
      id,
      operation_id,
      run_id,
      kind,
      input_digest,
      state,
      attempt,
      available_at,
      recorded_at,
      owner,
      claim,
    }
  }

  #[cfg(any(test, feature = "test-support"))]
  pub(crate) fn is_canonical(&self) -> bool {
    let ownership_is_valid = match self.state {
      FactoryOutboxState::Pending => self.owner.is_none() && self.claim.is_none(),
      FactoryOutboxState::Claimed | FactoryOutboxState::Delivered | FactoryOutboxState::Failed => {
        self.owner.is_some() && self.claim.is_some()
      }
    };
    ownership_is_valid
      && self
        == &Self::new_state(
          self.operation_id,
          self.run_id,
          self.kind.clone(),
          self.input_digest,
          self.state,
          self.attempt,
          self.available_at,
          self.recorded_at,
          self.owner.clone(),
          self.claim,
        )
  }
}

/// Bounded request to claim due Factory outbox work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimFactoryOutbox {
  /// Bounded worker identity.
  pub owner: FactoryKey,
  /// Authoritative selection time.
  pub observed_at: Timestamp,
  /// Exclusive dispatch-claim deadline.
  pub claim_expires_at: Timestamp,
  /// Positive bounded batch size.
  pub limit: NonZeroU16,
}

impl ClaimFactoryOutbox {
  /// Validates one bounded outbox claim request.
  pub fn new(
    owner: FactoryKey,
    observed_at: Timestamp,
    claim_expires_at: Timestamp,
    limit: u16,
  ) -> Result<Self, StoreError> {
    let limit = NonZeroU16::new(limit)
      .filter(|value| value.get() <= MAX_FACTORY_OUTBOX_BATCH_SIZE)
      .ok_or_else(|| {
        StoreError::invalid(
          crate::StoreOperation::ClaimFactoryOutbox,
          crate::StoreInputError::InvalidWorkerClaim,
        )
      })?;
    if claim_expires_at <= observed_at {
      return Err(StoreError::invalid(
        crate::StoreOperation::ClaimFactoryOutbox,
        crate::StoreInputError::InvalidWorkerClaim,
      ));
    }
    Ok(Self {
      owner,
      observed_at,
      claim_expires_at,
      limit,
    })
  }
}

/// One durably claimed Factory side effect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimedFactoryOutbox {
  /// Newly appended fenced claimed-state row.
  pub record: FactoryOutboxRecord,
}

/// Outcome used to settle one owned Factory outbox operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FactoryOutboxSettlement {
  /// The idempotent external effect was authoritatively observed.
  Delivered,
  /// A bounded retry may begin at this authoritative time.
  RetryAt(Timestamp),
  /// Retry policy is exhausted and the operation remains terminally failed.
  Failed,
}

/// Fenced settlement request for one claimed Factory outbox operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettleFactoryOutbox {
  /// Stable logical operation identity.
  pub operation_id: FactoryDigest,
  /// Worker presenting current ownership.
  pub owner: FactoryKey,
  /// Fence presented by the worker.
  pub fence: FactoryClaimFence,
  /// Authoritative settlement time.
  pub observed_at: Timestamp,
  /// Delivered, retryable, or terminal result.
  pub settlement: FactoryOutboxSettlement,
}

/// Bounded immutable rows appended by one code-owned Factory transition.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FactoryRunHistoryAppend {
  /// Newly created Stage Attempts.
  pub stage_attempts: Vec<StageAttempt>,
  /// Newly observed terminal Stage Attempt results and consumed budgets.
  pub stage_attempt_completions: Vec<StageAttemptCompletion>,
  /// Newly created macro-call DAG nodes.
  pub macro_calls: Vec<MacroCall>,
  /// Newly created non-authoritative Decision Signal requests.
  pub signal_requests: Vec<DecisionSignalRequest>,
  /// Newly accepted immutable Decision Signal receipts.
  pub signal_receipts: Vec<DecisionSignalReceipt>,
  /// Newly linked ordinary Builds.
  pub linked_builds: Vec<FactoryBuildLink>,
  /// Newly captured exact candidates.
  pub candidates: Vec<ChangeSet>,
  /// Newly constructed deterministic evidence.
  pub evidence: Vec<EvidenceManifest>,
  /// Newly frozen evaluation plans.
  pub evaluation_plans: Vec<EvaluationPlan>,
  /// Newly accepted evaluator Assessments.
  pub assessments: Vec<Assessment>,
  /// Newly computed deterministic Decisions.
  pub decisions: Vec<Decision>,
  /// Newly raised escalations.
  pub escalations: Vec<Escalation>,
  /// Newly started or observed delivery attempts.
  pub delivery_attempts: Vec<DeliveryAttempt>,
  /// Newly started or observed reporter attempts.
  pub reporting_attempts: Vec<ReportingAttempt>,
}

impl FactoryRunHistoryAppend {
  /// Returns the total number of immutable domain rows in this append.
  #[must_use]
  pub fn record_count(&self) -> usize {
    self.stage_attempts.len()
      + self.stage_attempt_completions.len()
      + self.macro_calls.len()
      + self.signal_requests.len()
      + self.signal_receipts.len()
      + self.linked_builds.len()
      + self.candidates.len()
      + self.evidence.len()
      + self.evaluation_plans.len()
      + self.assessments.len()
      + self.decisions.len()
      + self.escalations.len()
      + self.delivery_attempts.len()
      + self.reporting_attempts.len()
  }
}

/// Current indexed Run projection backed only by immutable history rows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryRunCurrentProjection {
  /// Current immutable budget row.
  pub budget_id: FactoryDigest,
  /// Current immutable lifecycle checkpoint.
  pub lifecycle_checkpoint_id: FactoryDigest,
  /// Current Stage Attempt, when a stage is selected.
  pub stage_attempt_id: Option<octacity_server_factory::StageAttemptId>,
  /// Current macro call, when one is active or most recently consumed.
  pub macro_call_id: Option<octacity_server_factory::MacroCallId>,
  /// Current Decision Signal request.
  pub signal_request_id: Option<octacity_server_factory::DecisionSignalRequestId>,
  /// Current Decision Signal receipt.
  pub signal_receipt_id: Option<octacity_server_factory::DecisionSignalReceiptId>,
  /// Current ordinary Build linked to the stage.
  pub build_id: Option<BuildId>,
  /// Current exact candidate.
  pub candidate_id: Option<octacity_server_factory::ChangeSetId>,
  /// Current deterministic evidence.
  pub evidence_id: Option<octacity_server_factory::EvidenceManifestId>,
  /// Current immutable evaluation plan.
  pub evaluation_plan_id: Option<octacity_server_factory::EvaluationPlanId>,
  /// Latest authoritative deterministic Decision.
  pub decision_id: Option<octacity_server_factory::DecisionId>,
  /// Current unresolved or most recent escalation.
  pub escalation_id: Option<octacity_server_factory::EscalationId>,
  /// Current delivery attempt.
  pub delivery_attempt_id: Option<octacity_server_factory::DeliveryAttemptId>,
  /// Current reporter attempt.
  pub reporting_attempt_id: Option<octacity_server_factory::ReportingAttemptId>,
}

/// Complete bounded durable state used by Factory reconciliation and diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryRunSnapshot {
  /// Immutable admitted Work.
  pub work: WorkEnvelope,
  /// Current Run state and optimistic aggregate version.
  pub run: FactoryRun,
  /// Current claim, when a reconciler owns the Run.
  pub current_claim: Option<FactoryRunClaimRecord>,
  /// Append-only ownership history.
  pub claims: Vec<FactoryRunClaimRecord>,
  /// Append-only budget history.
  pub budgets: Vec<FactoryBudgetRecord>,
  /// Append-only code-owned lifecycle checkpoints.
  pub lifecycle_checkpoints: Vec<FactoryLifecycleCheckpoint>,
  /// Append-only Stage Attempt history.
  pub stage_attempts: Vec<StageAttempt>,
  /// Append-only terminal Stage Attempt observations.
  pub stage_attempt_completions: Vec<StageAttemptCompletion>,
  /// Append-only macro-call DAG nodes.
  pub macro_calls: Vec<MacroCall>,
  /// Append-only Decision Signal requests.
  pub signal_requests: Vec<DecisionSignalRequest>,
  /// Append-only Decision Signal receipts.
  pub signal_receipts: Vec<DecisionSignalReceipt>,
  /// Append-only ordinary Build links.
  pub linked_builds: Vec<FactoryBuildLink>,
  /// Append-only exact candidates.
  pub candidates: Vec<ChangeSet>,
  /// Append-only deterministic evidence.
  pub evidence: Vec<EvidenceManifest>,
  /// Append-only evaluation plans.
  pub evaluation_plans: Vec<EvaluationPlan>,
  /// Append-only evaluator Assessments.
  pub assessments: Vec<Assessment>,
  /// Append-only deterministic Decisions.
  pub decisions: Vec<Decision>,
  /// Append-only escalations.
  pub escalations: Vec<Escalation>,
  /// Append-only delivery attempts.
  pub delivery_attempts: Vec<DeliveryAttempt>,
  /// Append-only reporting attempts.
  pub reporting_attempts: Vec<ReportingAttempt>,
  /// Append-only secret-safe audit facts.
  pub audit: Vec<FactoryAuditFact>,
  /// Append-only outbox state rows.
  pub outbox: Vec<FactoryOutboxRecord>,
  /// Current indexed pointers validated against the immutable collections above.
  pub current: FactoryRunCurrentProjection,
}

/// Request to append and publish one exclusive Factory claim.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimFactoryRun {
  /// Run to claim.
  pub run_id: FactoryRunId,
  /// Aggregate version observed before claiming.
  pub expected_version: FactoryRunVersion,
  /// Immutable claim row.
  pub record: FactoryRunClaimRecord,
  /// Same-transaction audit fact.
  pub audit: FactoryAuditFact,
}

/// Result of acquiring or exactly replaying one claim.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimFactoryRunOutcome {
  /// Whether this invocation appended the claim or replayed it.
  pub disposition: MutationDisposition,
  /// Immutable claim that owns the Run.
  pub record: FactoryRunClaimRecord,
}

/// Bounded request to claim eligible Factory Runs for reconciliation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimFactoryRuns {
  /// Bounded process-instance identity.
  pub owner: FactoryKey,
  /// Authoritative selection and claim time.
  pub observed_at: Timestamp,
  /// Exclusive deadline after which another replica may take over.
  pub claim_expires_at: Timestamp,
  /// Positive bounded number of Runs returned by this pass.
  pub limit: NonZeroU16,
}

impl ClaimFactoryRuns {
  /// Validates one bounded reconciliation claim request.
  pub fn new(
    owner: FactoryKey,
    observed_at: Timestamp,
    claim_expires_at: Timestamp,
    limit: u16,
  ) -> Result<Self, StoreError> {
    let limit = NonZeroU16::new(limit)
      .filter(|value| value.get() <= MAX_FACTORY_RECONCILIATION_BATCH_SIZE)
      .ok_or_else(|| {
        StoreError::invalid(
          crate::StoreOperation::ClaimFactoryRuns,
          crate::StoreInputError::InvalidFactoryRunClaim,
        )
      })?;
    if claim_expires_at <= observed_at {
      return Err(StoreError::invalid(
        crate::StoreOperation::ClaimFactoryRuns,
        crate::StoreInputError::InvalidFactoryRunClaim,
      ));
    }
    Ok(Self {
      owner,
      observed_at,
      claim_expires_at,
      limit,
    })
  }
}

/// One Factory Run durably owned by a bounded reconciliation pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimedFactoryRun {
  /// Factory Run owned by this claim.
  pub run_id: FactoryRunId,
  /// Whether ownership was newly appended or exactly replayed.
  pub disposition: MutationDisposition,
  /// Aggregate version observed while acquiring ownership.
  pub expected_version: FactoryRunVersion,
  /// Immutable fenced claim record.
  pub record: FactoryRunClaimRecord,
  /// Authoritative configuration-scoped WIP usage observed at claim time.
  pub wip_usage: FactoryWipUsage,
}

/// Atomic fenced append and current-projection update for one Factory transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitFactoryRunTransition {
  /// Run whose immutable history is extended.
  pub run_id: FactoryRunId,
  /// Aggregate version loaded under the claim.
  pub expected_version: FactoryRunVersion,
  /// Current immutable claim row identity.
  pub claim_id: FactoryDigest,
  /// Owner presenting the fence.
  pub owner: FactoryKey,
  /// Fence presented by the current owner.
  pub fence: FactoryClaimFence,
  /// Authoritative commit time used for claim expiry checks.
  pub committed_at: Timestamp,
  /// Next Run projection with exactly the next aggregate version.
  pub next_run: FactoryRun,
  /// One new immutable monotonic budget observation.
  pub budget: FactoryBudgetRecord,
  /// One new immutable code-owned lifecycle checkpoint.
  pub lifecycle_checkpoint: FactoryLifecycleCheckpoint,
  /// Bounded immutable domain rows appended by the transition.
  pub append: FactoryRunHistoryAppend,
  /// Current projection after the append.
  pub current: FactoryRunCurrentProjection,
  /// Same-transaction immutable audit fact.
  pub audit: FactoryAuditFact,
  /// Same-transaction durable external work state.
  pub outbox: Vec<FactoryOutboxRecord>,
}

/// Result of one committed Factory transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitFactoryRunTransitionOutcome {
  /// Updated aggregate version.
  pub version: FactoryRunVersion,
  /// Current indexed projection backed by immutable rows.
  pub current: FactoryRunCurrentProjection,
}
