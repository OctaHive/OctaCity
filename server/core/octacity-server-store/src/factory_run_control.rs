use octacity_server_domain::Timestamp;
use octacity_server_factory::{
  ChangeSetId, DecisionId, EscalationId, FactoryDigest, FactoryEscalationDisposition, FactoryRunId, FactoryRunState,
  FactoryRunVersion, FactoryText, StageAttemptId,
};

use crate::{IdempotencyKey, MutationDisposition};

/// One closed, code-owned Factory Run control intent.
///
/// The variants contain only identities used as strong confirmations. There
/// is intentionally no caller-provided lifecycle state, candidate digest,
/// Assessment, Decision outcome, or success value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactoryRunControlIntent {
  /// Request cancellation through the normal reconciler path.
  Cancel,
  /// Confirm retry of the current infrastructure-retryable Stage Attempt.
  RetryInfrastructure {
    /// Current Stage Attempt whose failure was observed.
    stage_attempt_id: StageAttemptId,
  },
  /// Apply one permitted disposition to the current escalation.
  ResolveEscalation {
    /// Current escalation used as a stale-request precondition.
    escalation_id: EscalationId,
    /// Closed policy disposition.
    disposition: FactoryEscalationDisposition,
    /// Bounded operator reason retained in immutable audit material.
    reason: FactoryText,
  },
  /// Approve delivery of the exact candidate accepted by the current Decision.
  RequestDelivery {
    /// Current accepted candidate used as a stale-request precondition.
    candidate_id: ChangeSetId,
    /// Current accepting Decision used as a stale-request precondition.
    decision_id: DecisionId,
  },
}

impl FactoryRunControlIntent {
  #[cfg(any(test, feature = "test-support"))]
  pub(crate) const fn operation(&self) -> &'static str {
    match self {
      Self::Cancel => "cancel-factory-run",
      Self::RetryInfrastructure { .. } => "retry-factory-stage",
      Self::ResolveEscalation { .. } => "resolve-factory-escalation",
      Self::RequestDelivery { .. } => "request-factory-delivery",
    }
  }

  fn digest(&self) -> FactoryDigest {
    match self {
      Self::Cancel => FactoryDigest::sha256("octacity.factory.control-intent.v1", &[b"cancel"]),
      Self::RetryInfrastructure { stage_attempt_id } => FactoryDigest::sha256(
        "octacity.factory.control-intent.v1",
        &[b"retry", stage_attempt_id.as_uuid().as_bytes()],
      ),
      Self::ResolveEscalation {
        escalation_id,
        disposition,
        reason,
      } => FactoryDigest::sha256(
        "octacity.factory.control-intent.v1",
        &[
          b"escalation",
          escalation_id.as_uuid().as_bytes(),
          disposition.as_str().as_bytes(),
          reason.as_str().as_bytes(),
        ],
      ),
      Self::RequestDelivery {
        candidate_id,
        decision_id,
      } => FactoryDigest::sha256(
        "octacity.factory.control-intent.v1",
        &[
          b"delivery",
          candidate_id.as_uuid().as_bytes(),
          decision_id.as_uuid().as_bytes(),
        ],
      ),
    }
  }
}

/// Immutable accepted operator intent appended to Factory Run history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryRunControlRecord {
  /// Content-derived record identity.
  pub id: FactoryDigest,
  /// Controlled Run identity.
  pub run_id: FactoryRunId,
  /// Aggregate version produced by this intent.
  pub run_version: FactoryRunVersion,
  /// Closed accepted intent, including its bounded human reason when required.
  pub intent: FactoryRunControlIntent,
  /// Authoritative acceptance time.
  pub recorded_at: Timestamp,
}

impl FactoryRunControlRecord {
  /// Constructs one canonical immutable control record.
  #[must_use]
  pub fn new(
    run_id: FactoryRunId,
    run_version: FactoryRunVersion,
    intent: FactoryRunControlIntent,
    recorded_at: Timestamp,
  ) -> Self {
    let id = FactoryDigest::sha256(
      "octacity.factory.control-record.v1",
      &[
        run_id.as_uuid().as_bytes(),
        &run_version.get().to_be_bytes(),
        &intent.digest().as_bytes(),
        &recorded_at.unix_millis().to_be_bytes(),
      ],
    );
    Self {
      id,
      run_id,
      run_version,
      intent,
      recorded_at,
    }
  }
}

/// Idempotent, preconditioned mutation of one Factory Run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplyFactoryRunControl {
  /// Run whose current state is being controlled.
  pub run_id: FactoryRunId,
  /// Exact aggregate version observed by the caller.
  pub expected_version: FactoryRunVersion,
  /// Stable transport replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Closed command intent.
  pub intent: FactoryRunControlIntent,
  /// Authoritative request time.
  pub requested_at: Timestamp,
}

/// Safe result of applying or replaying one Factory Run control intent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryRunControlOutcome {
  /// Whether the original mutation was applied or replayed.
  pub disposition: MutationDisposition,
  /// Controlled Run identity.
  pub run_id: FactoryRunId,
  /// Aggregate version produced by the original mutation.
  pub version: FactoryRunVersion,
  /// Code-owned state after the mutation.
  pub state: FactoryRunState,
  /// Whether cancellation is durably requested for reconciliation.
  pub cancellation_requested: bool,
}
