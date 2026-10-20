use std::num::NonZeroU16;

use octacity_server_domain::Timestamp;
use octacity_server_factory::{
  AdmittedFlow, BudgetUsage, FactoryClaim, FactoryClaimFence, FactoryDigest, FactoryKey, FactoryLifecycleProgress,
  FactoryRun, FactoryRunId, FactoryRunVersion, FactoryWipUsage, FlowRun, NodeAttempt, NodeAttemptCompletion,
  WorkEnvelope, WorkflowCycle,
};

use crate::{AuditActorKind, MutationDisposition, StoreError, StoreInputError, StoreOperation};

/// Maximum immutable history rows appended by one Factory transition.
pub const MAX_FACTORY_TRANSITION_RECORDS: usize = 256;
/// Maximum outbox state rows appended by one Factory transition.
pub const MAX_FACTORY_TRANSITION_OUTBOX_RECORDS: usize = 32;
/// Maximum immutable rows loaded in one complete Factory Run snapshot.
pub const MAX_FACTORY_RUN_SNAPSHOT_RECORDS: usize = 32_768;
/// Maximum immutable diagnostic rows returned by one paged read.
pub const MAX_FACTORY_RUN_DIAGNOSTIC_PAGE_SIZE: u16 = 100;
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
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
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
  /// Technical root execution progress for the admitted graph.
  pub progress: FactoryLifecycleProgress,
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
      cancellation_requested,
      recorded_at,
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
  /// Technical delivery is terminally unavailable or its retry policy was exhausted.
  Failed,
}

/// Accepted owner of a deferred effect; no synthetic Work or Run is introduced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FactoryOutboxSubject {
  /// Existing configured Factory action.
  Run(FactoryRunId),
  /// Authorized metadata revision of a local task.
  Issue(crate::IssueProjectionSubject),
}
impl From<FactoryRunId> for FactoryOutboxSubject {
  fn from(run: FactoryRunId) -> Self {
    Self::Run(run)
  }
}
impl FactoryOutboxSubject {
  /// Original Run when the effect belongs to execution.
  pub fn run_id(self) -> Option<FactoryRunId> {
    match self {
      Self::Run(id) => Some(id),
      Self::Issue(_) => None,
    }
  }
}

/// Immutable outbox state row for one stable logical side effect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryOutboxRecord {
  /// Unique immutable row identity.
  pub id: FactoryDigest,
  /// Stable identity shared by every state row for the logical side effect.
  pub operation_id: FactoryDigest,
  /// Original accepted owner; an Issue effect requires no execution.
  pub subject: FactoryOutboxSubject,
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
  /// Canonical immutable journal order; a later attempt supersedes an earlier terminal row.
  #[must_use]
  pub fn order_key(&self) -> (u16, u8, Timestamp, FactoryDigest) {
    let state = match self.state {
      FactoryOutboxState::Pending => 0,
      FactoryOutboxState::Claimed => 1,
      FactoryOutboxState::Delivered | FactoryOutboxState::Failed => 2,
    };
    (self.attempt, state, self.recorded_at, self.id)
  }

  /// Constructs an initial pending outbox state row.
  #[must_use]
  pub fn pending(
    operation_id: FactoryDigest,
    subject: impl Into<FactoryOutboxSubject>,
    kind: FactoryKey,
    input_digest: FactoryDigest,
    available_at: Timestamp,
    recorded_at: Timestamp,
  ) -> Self {
    Self::new_state(
      operation_id,
      subject.into(),
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
      previous.subject,
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
      previous.subject,
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
      previous.subject,
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
    subject: FactoryOutboxSubject,
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
    let subject_bytes = match subject {
      FactoryOutboxSubject::Run(run) => run.as_uuid().as_bytes().to_vec(),
      FactoryOutboxSubject::Issue(issue) => serde_json::to_vec(&issue).expect("finite Issue subject serializes"),
    };
    let id = FactoryDigest::sha256(
      "octacity.factory.outbox.v1",
      &[
        &operation_id.as_bytes(),
        &subject_bytes,
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
      subject,
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

  /// Returns whether this row is the canonical representation of its fields.
  #[must_use]
  pub fn is_canonical(&self) -> bool {
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
          self.subject,
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
  /// Exact effect families; empty preserves the unfiltered administrative claim.
  pub kinds: Vec<FactoryKey>,
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
      kinds: vec![],
    })
  }
  /// Narrows a bounded pass to the installed effect families.
  pub fn only_kinds(mut self, kinds: Vec<FactoryKey>) -> Result<Self, StoreError> {
    if kinds.is_empty() || kinds.len() > 256 {
      return Err(crate::source_conflict());
    }
    self.kinds = kinds;
    Ok(self)
  }
  /// Narrows this bounded claim to one configured effect family.
  #[must_use]
  pub fn only_kind(mut self, kind: FactoryKey) -> Self {
    self.kinds = vec![kind];
    self
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

/// Common runtime and retained data owned by a configured Flow.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FactoryFlowHistory {
  /// Root and nested Flow Runs in append-only creation order.
  pub runs: Vec<FlowRun>,
  /// Append-only workflow cycles.
  pub cycles: Vec<WorkflowCycle>,
  /// Append-only generic Node Attempts.
  pub attempts: Vec<NodeAttempt>,
  /// Append-only typed generic Node Attempt results.
  pub completions: Vec<NodeAttemptCompletion>,
  /// Common frozen inputs and verified retained results for configured nodes.
  pub data: octacity_server_factory::FlowDataHistory,
}

impl FactoryFlowHistory {
  /// Borrows all runtime facts, including technical stops, for common interpretation.
  #[must_use]
  pub fn runtime(&self) -> octacity_server_factory::FlowRuntimeHistory<'_> {
    octacity_server_factory::FlowRuntimeHistory {
      stops: &self.data.stops,
      flow_runs: &self.runs,
      cycles: &self.cycles,
      attempts: &self.attempts,
      completions: &self.completions,
    }
  }

  fn record_count(&self) -> usize {
    self.runs.len() + self.cycles.len() + self.attempts.len() + self.completions.len() + self.data.record_count()
  }
}

/// Bounded immutable rows appended by the configured Flow owner.
///
/// Capability data is retained in the common node result rather than a parallel stage journal.
///
/// ```compile_fail
/// use octacity_server_store::FactoryRunHistoryAppend;
/// let _ = FactoryRunHistoryAppend { stage_attempts: vec![], ..Default::default() };
/// ```
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FactoryRunHistoryAppend {
  /// Generic Flow runtime rows kept behind one cohesive boundary.
  pub flow: FactoryFlowHistory,
}

impl FactoryRunHistoryAppend {
  /// Returns the total number of immutable domain rows in this append.
  #[must_use]
  pub fn record_count(&self) -> usize {
    self.flow.record_count()
  }
}

/// Current indexed Run projection backed only by immutable history rows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryRunCurrentProjection {
  /// Current immutable budget row.
  pub budget_id: FactoryDigest,
  /// Current immutable lifecycle checkpoint.
  pub lifecycle_checkpoint_id: FactoryDigest,
}

/// Bounded working state used by Factory reconciliation.
/// Ownership and audit history are read through paged diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryRunSnapshot {
  /// Immutable admitted Work.
  pub work: WorkEnvelope,
  /// Current Run state and optimistic aggregate version.
  pub run: FactoryRun,
  /// Immutable admitted definition closure and initial root Flow records.
  pub admitted_flow: AdmittedFlow,
  /// Complete generic Flow runtime history.
  pub flow: FactoryFlowHistory,
  /// Current claim, when a reconciler owns the Run.
  pub current_claim: Option<FactoryRunClaimRecord>,
  /// Append-only budget history.
  pub budgets: Vec<FactoryBudgetRecord>,
  /// Append-only code-owned lifecycle checkpoints.
  pub lifecycle_checkpoints: Vec<FactoryLifecycleCheckpoint>,
  /// Append-only accepted operator control intents.
  pub controls: Vec<crate::FactoryRunControlRecord>,
  /// Append-only outbox state rows.
  pub outbox: Vec<FactoryOutboxRecord>,
  /// Current indexed pointers validated against the immutable collections above.
  pub current: FactoryRunCurrentProjection,
}

/// Allowlisted ordinary Build correlation without Job specifications or object locations.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryBuildExecutionDiagnostic {
  /// Immutable execution-link identity.
  pub id: FactoryDigest,
  /// Stable frozen operation identity shared by its ordinary retries.
  pub operation_id: FactoryDigest,
  /// Exact owning node execution.
  pub node_attempt_id: octacity_server_factory::NodeAttemptId,
  /// Actual ordinary Build.
  pub build_id: octacity_server_domain::BuildId,
  /// Actual ordinary execution attempt.
  pub attempt_id: octacity_server_domain::AttemptId,
  /// Complete bounded sorted Job identities.
  pub jobs: Vec<octacity_server_domain::JobId>,
}
impl From<&octacity_server_factory::FlowBuildExecution> for FactoryBuildExecutionDiagnostic {
  fn from(execution: &octacity_server_factory::FlowBuildExecution) -> Self {
    Self {
      id: execution
        .digest()
        .expect("validated bounded execution identities have a canonical digest"),
      operation_id: execution.operation_id(),
      node_attempt_id: execution.node_attempt_id(),
      build_id: execution.build_id(),
      attempt_id: execution.attempt_id(),
      jobs: execution.jobs().to_vec(),
    }
  }
}

/// Safe common result correlation; input, Context and provider payload are never projected.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryDomainDiagnostic {
  /// Complete immutable producing record identity.
  pub id: FactoryDigest,
  /// Exact producing node execution.
  pub node_attempt_id: octacity_server_factory::NodeAttemptId,
  /// Exact root or nested Flow execution.
  pub flow_run_id: octacity_server_factory::FlowRunId,
  /// Exact producing cycle.
  pub cycle_id: octacity_server_factory::WorkflowCycleId,
  /// Exact frozen input identity without its contents.
  pub input_digest: FactoryDigest,
  /// Trusted observation time retained with the completion.
  pub observed_at: Timestamp,
  /// Typed verified domain projection, excluding prose and protected material.
  pub facts: octacity_server_factory::FlowDiscoveryFacts,
}
impl TryFrom<&octacity_server_factory::FlowNodeRecord> for FactoryDomainDiagnostic {
  type Error = octacity_server_factory::FactoryError;
  fn try_from(record: &octacity_server_factory::FlowNodeRecord) -> Result<Self, Self::Error> {
    Ok(Self {
      id: record.digest()?,
      node_attempt_id: record.node_attempt_id(),
      flow_run_id: record.input().flow_run_id(),
      cycle_id: record.input().cycle_id(),
      input_digest: record.input().digest()?,
      observed_at: record.observed_at(),
      facts: record
        .discovery()
        .cloned()
        .ok_or(octacity_server_factory::FactoryError::InvalidConfiguration {
          field: "domain diagnostic",
        })?,
    })
  }
}

/// Safe frozen task and executor provenance for an ordinary Build-backed call.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryCallProfileDiagnostic {
  /// Complete immutable external request identity.
  pub operation_id: FactoryDigest,
  /// Selected immutable context identity without entries or protected references.
  pub context_id: octacity_server_factory::ContextManifestId,
  /// Content identity of the exact selected Context.
  pub context_digest: FactoryDigest,
  /// Exact executed harness or deterministic tool.
  pub tool: octacity_server_factory::ImmutableReference,
  /// Exact runner plugin.
  pub plugin: octacity_server_factory::ImmutableReference,
  /// Exact configured model or tool contract.
  pub model_or_tool: octacity_server_factory::ImmutableReference,
  /// Content identity of the exact task/prompt contract, excluding rendered text.
  pub task_digest: FactoryDigest,
  /// Digest of the narrowed permission set, excluding command and path details.
  pub permission_digest: FactoryDigest,
  /// Immutable ordinary Build definition selected for this request.
  pub build_configuration: octacity_server_factory::BuildConfigurationRef,
}
impl TryFrom<&octacity_server_factory::FlowBuildIntent> for FactoryCallProfileDiagnostic {
  type Error = octacity_server_factory::FactoryError;
  fn try_from(intent: &octacity_server_factory::FlowBuildIntent) -> Result<Self, Self::Error> {
    let context = intent
      .input()
      .context()
      .ok_or(octacity_server_factory::FactoryError::InvalidConfiguration {
        field: "call profile Context",
      })?;
    let profile = intent.profile();
    Ok(Self {
      operation_id: intent.operation_id()?,
      context_id: context.id(),
      context_digest: context.digest()?,
      tool: profile.tool.clone(),
      plugin: profile.plugin.clone(),
      model_or_tool: profile.model_or_tool.clone(),
      task_digest: profile.task_digest,
      permission_digest: intent.permissions().digest(),
      build_configuration: profile.build_configuration.clone(),
    })
  }
}

/// One append-only Factory Run diagnostic collection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FactoryRunDiagnosticKind {
  /// Confirmed technical stops without configured business outcomes.
  NodeStop,

  /// Immutable admitted Work identity and authority.
  Work,
  /// Exact external node invocation and safe frozen task metadata.
  Call,
  /// Safe verified observations retained in the common node journal.
  DomainFact(octacity_server_factory::FlowDiscoveryKind),
  /// Exact ordinary execution links.
  BuildExecution,
  /// Authoritative aggregate consumption observations.
  Budget,
  /// Exact workflow cycle and its causal predecessor.
  WorkflowCycle,
  /// Root and nested executions of the exact admitted definitions.
  FlowRun,
  /// Complete ownership history, independent of the working snapshot ceiling.
  Claim,
  /// Complete secret-safe audit history.
  Audit,
  /// Executions of arbitrary configured nodes.
  NodeAttempt,
  /// Verified completions of arbitrary configured nodes.
  NodeAttemptCompletion,
}

/// Typed immutable row returned by a common execution diagnostic collection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactoryRunDiagnosticRecord {
  /// Confirmed stopped execution, including its technical cause.
  NodeStop(Box<octacity_server_factory::FlowNodeStop>),

  /// Immutable admitted Work, projected through an allowlist by management.
  Work(Box<WorkEnvelope>),
  /// External invocation whose full execution input remains private.
  Call {
    /// Persisted execution, projected through an allowlist by management.
    node: Box<NodeAttempt>,
    /// Frozen task metadata when the invocation uses an ordinary Build.
    profile: Option<Box<FactoryCallProfileDiagnostic>>,
  },
  /// Allowlisted owner observation with exact producing journal identity.
  DomainFact(Box<FactoryDomainDiagnostic>),
  /// Safe ordinary execution links.
  BuildExecution(Box<FactoryBuildExecutionDiagnostic>),
  /// Aggregate consumption at an exact Run version.
  Budget(Box<FactoryBudgetRecord>),
  /// Exact workflow cycle without execution input.
  WorkflowCycle(Box<WorkflowCycle>),
  /// Exact Flow identity and parent-call relation, without input data.
  FlowRun(Box<FlowRun>),
  /// Canonical immutable ownership proof.
  Claim(Box<FactoryRunClaimRecord>),
  /// Canonical immutable audit fact.
  Audit(Box<FactoryAuditFact>),
  /// Common node execution.
  NodeAttempt(Box<NodeAttempt>),
  /// Common node completion.
  NodeAttemptCompletion(Box<NodeAttemptCompletion>),
}

impl FactoryRunDiagnosticRecord {
  /// Returns the stable cursor for deterministic pagination.
  #[must_use]
  pub fn cursor(&self) -> FactoryKey {
    let value = match self {
      Self::NodeStop(value) => value
        .digest()
        .expect("verified stops have canonical identities")
        .to_string(),
      Self::Work(value) => value.id().to_string(),
      Self::WorkflowCycle(value) => value.id().to_string(),
      Self::Call { node, .. } => node.id().to_string(),
      Self::DomainFact(value) => value.id.to_string(),
      Self::BuildExecution(value) => value.id.to_string(),
      Self::Budget(value) => value.id.to_string(),
      Self::FlowRun(value) => value.id().to_string(),
      Self::Claim(value) => value.id.to_string(),
      Self::Audit(value) => value.id.to_string(),
      Self::NodeAttempt(value) => value.id().to_string(),
      Self::NodeAttemptCompletion(value) => value.id().to_string(),
    };
    FactoryKey::new(value).expect("common diagnostic identities are canonical keys")
  }
}

/// Bounded deterministic Factory diagnostic page request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListFactoryRunDiagnostics {
  /// Factory Run owning every returned row.
  pub run_id: FactoryRunId,
  /// Requested typed collection.
  pub kind: FactoryRunDiagnosticKind,
  /// Exclusive last-seen identity.
  pub after: Option<FactoryKey>,
  /// Positive bounded page size.
  pub limit: NonZeroU16,
}

impl ListFactoryRunDiagnostics {
  /// Validates a bounded diagnostic page request.
  pub fn new(
    run_id: FactoryRunId,
    kind: FactoryRunDiagnosticKind,
    after: Option<FactoryKey>,
    limit: u16,
  ) -> Result<Self, StoreError> {
    let limit = NonZeroU16::new(limit).ok_or_else(|| {
      StoreError::invalid(
        StoreOperation::ListFactoryRunDiagnostics,
        StoreInputError::InvalidFactoryRunDiagnosticPageSize,
      )
    })?;
    let request = Self {
      run_id,
      kind,
      after,
      limit,
    };
    request.validate()?;
    Ok(request)
  }
  /// Revalidates bounded requests at the store boundary, including struct literals.
  pub fn validate(&self) -> Result<(), StoreError> {
    if self.limit.get() > MAX_FACTORY_RUN_DIAGNOSTIC_PAGE_SIZE {
      return Err(StoreError::invalid(
        StoreOperation::ListFactoryRunDiagnostics,
        StoreInputError::InvalidFactoryRunDiagnosticPageSize,
      ));
    }
    if let Some(key) = &self.after {
      let valid = match self.kind {
        FactoryRunDiagnosticKind::FlowRun
        | FactoryRunDiagnosticKind::WorkflowCycle
        | FactoryRunDiagnosticKind::NodeAttempt
        | FactoryRunDiagnosticKind::Call
        | FactoryRunDiagnosticKind::Work => key
          .as_str()
          .parse::<uuid::Uuid>()
          .is_ok_and(|id| id.to_string() == key.as_str()),
        FactoryRunDiagnosticKind::Claim
        | FactoryRunDiagnosticKind::Audit
        | FactoryRunDiagnosticKind::Budget
        | FactoryRunDiagnosticKind::BuildExecution
        | FactoryRunDiagnosticKind::DomainFact(_)
        | FactoryRunDiagnosticKind::NodeAttemptCompletion
        | FactoryRunDiagnosticKind::NodeStop => FactoryDigest::from_lower_hex(key.as_str()).is_ok(),
      };
      if !valid {
        return Err(StoreError::invalid(
          StoreOperation::ListFactoryRunDiagnostics,
          StoreInputError::InvalidFactoryRunDiagnosticCursor,
        ));
      }
    }
    Ok(())
  }
}

/// One deterministic bounded Factory diagnostic page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryRunDiagnosticPage {
  /// Typed rows in ascending immutable-identity order.
  pub items: Vec<FactoryRunDiagnosticRecord>,
  /// Cursor for the next page, absent when this page is terminal.
  pub next: Option<FactoryKey>,
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
