use crate::{
  BudgetLimit, DecisionSignalPurpose, DecisionSignalRequestId, ExactSubject, FactoryDigest, FactoryError, FactoryRun,
  FactoryRunId, FactoryStageKind, MacroCallId, MacroCallKind, StageAttemptId, StageAttemptNumber,
};

/// Canonical input and deterministic policy digests for one Decision Signal request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecisionSignalDigests {
  input: FactoryDigest,
  policy: FactoryDigest,
}

impl DecisionSignalDigests {
  /// Constructs the exact immutable request digests.
  #[must_use]
  pub const fn new(input: FactoryDigest, policy: FactoryDigest) -> Self {
    Self { input, policy }
  }

  /// Returns the canonical redacted input digest.
  #[must_use]
  pub const fn input(self) -> FactoryDigest {
    self.input
  }

  /// Returns the deterministic policy digest.
  #[must_use]
  pub const fn policy(self) -> FactoryDigest {
    self.policy
  }
}

/// Append-only attempt to execute one program-selected Factory stage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StageAttempt {
  id: StageAttemptId,
  run_id: FactoryRunId,
  subject: ExactSubject,
  number: StageAttemptNumber,
  kind: FactoryStageKind,
  budget: BudgetLimit,
  input_digest: FactoryDigest,
}

impl StageAttempt {
  /// Constructs an attempt bound to the exact Run subject.
  #[must_use]
  pub fn new(
    id: StageAttemptId,
    run: &FactoryRun,
    number: StageAttemptNumber,
    kind: FactoryStageKind,
    budget: BudgetLimit,
    input_digest: FactoryDigest,
  ) -> Self {
    Self {
      id,
      run_id: run.id(),
      subject: run.subject().clone(),
      number,
      kind,
      budget,
      input_digest,
    }
  }

  /// Returns the Stage Attempt identity.
  #[must_use]
  pub const fn id(&self) -> StageAttemptId {
    self.id
  }

  /// Returns the owning Run identity.
  #[must_use]
  pub const fn run_id(&self) -> FactoryRunId {
    self.run_id
  }

  /// Returns the exact immutable subject.
  #[must_use]
  pub const fn subject(&self) -> &ExactSubject {
    &self.subject
  }

  /// Returns the append-only attempt number.
  #[must_use]
  pub const fn number(&self) -> StageAttemptNumber {
    self.number
  }

  /// Returns the stage kind.
  #[must_use]
  pub const fn kind(&self) -> FactoryStageKind {
    self.kind
  }

  /// Returns the immutable hard budget.
  #[must_use]
  pub const fn budget(&self) -> BudgetLimit {
    self.budget
  }

  /// Returns the canonical stage input digest.
  #[must_use]
  pub const fn input_digest(&self) -> FactoryDigest {
    self.input_digest
  }
}

/// One bounded node in a Stage Attempt's durable macro-call DAG.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacroCall {
  id: MacroCallId,
  stage_attempt_id: StageAttemptId,
  run_id: FactoryRunId,
  subject: ExactSubject,
  kind: MacroCallKind,
  context_digest: FactoryDigest,
  budget: BudgetLimit,
  parent_id: Option<MacroCallId>,
}

impl MacroCall {
  /// Constructs a call and validates its optional parent belongs to the same stage and subject.
  pub fn new(
    id: MacroCallId,
    stage: &StageAttempt,
    kind: MacroCallKind,
    context_digest: FactoryDigest,
    budget: BudgetLimit,
    parent: Option<&Self>,
  ) -> Result<Self, FactoryError> {
    if parent.is_some_and(|parent| {
      parent.id == id
        || parent.stage_attempt_id != stage.id
        || parent.run_id != stage.run_id
        || parent.subject != stage.subject
    }) {
      return Err(FactoryError::InvalidReference {
        relationship: "macro call parent",
      });
    }
    Ok(Self {
      id,
      stage_attempt_id: stage.id,
      run_id: stage.run_id,
      subject: stage.subject.clone(),
      kind,
      context_digest,
      budget,
      parent_id: parent.map(|value| value.id),
    })
  }

  /// Returns the call identity.
  #[must_use]
  pub const fn id(&self) -> MacroCallId {
    self.id
  }

  /// Returns the owning Stage Attempt.
  #[must_use]
  pub const fn stage_attempt_id(&self) -> StageAttemptId {
    self.stage_attempt_id
  }

  /// Returns the owning Run.
  #[must_use]
  pub const fn run_id(&self) -> FactoryRunId {
    self.run_id
  }

  /// Returns the exact immutable subject.
  #[must_use]
  pub const fn subject(&self) -> &ExactSubject {
    &self.subject
  }

  /// Returns the call purpose.
  #[must_use]
  pub const fn kind(&self) -> MacroCallKind {
    self.kind
  }

  /// Returns the exact Context Manifest digest.
  #[must_use]
  pub const fn context_digest(&self) -> FactoryDigest {
    self.context_digest
  }

  /// Returns the immutable call budget.
  #[must_use]
  pub const fn budget(&self) -> BudgetLimit {
    self.budget
  }

  /// Returns the optional parent call.
  #[must_use]
  pub const fn parent_id(&self) -> Option<MacroCallId> {
    self.parent_id
  }
}

/// Provider-neutral request for one bounded, non-authoritative Decision Signal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecisionSignalRequest {
  id: DecisionSignalRequestId,
  stage_attempt_id: StageAttemptId,
  run_id: FactoryRunId,
  subject: ExactSubject,
  purpose: DecisionSignalPurpose,
  input_digest: FactoryDigest,
  policy_digest: FactoryDigest,
  budget: BudgetLimit,
}

impl DecisionSignalRequest {
  /// Constructs a request bound to one exact Stage Attempt.
  #[must_use]
  pub fn new(
    id: DecisionSignalRequestId,
    stage: &StageAttempt,
    purpose: DecisionSignalPurpose,
    digests: DecisionSignalDigests,
    budget: BudgetLimit,
  ) -> Self {
    Self {
      id,
      stage_attempt_id: stage.id,
      run_id: stage.run_id,
      subject: stage.subject.clone(),
      purpose,
      input_digest: digests.input,
      policy_digest: digests.policy,
      budget,
    }
  }

  /// Returns the stable logical request identity.
  #[must_use]
  pub const fn id(&self) -> DecisionSignalRequestId {
    self.id
  }

  /// Returns the owning Stage Attempt.
  #[must_use]
  pub const fn stage_attempt_id(&self) -> StageAttemptId {
    self.stage_attempt_id
  }

  /// Returns the owning Run.
  #[must_use]
  pub const fn run_id(&self) -> FactoryRunId {
    self.run_id
  }

  /// Returns the exact subject.
  #[must_use]
  pub const fn subject(&self) -> &ExactSubject {
    &self.subject
  }

  /// Returns the only seam in which this signal may be consumed.
  #[must_use]
  pub const fn purpose(&self) -> DecisionSignalPurpose {
    self.purpose
  }

  /// Returns the canonical redacted input digest.
  #[must_use]
  pub const fn input_digest(&self) -> FactoryDigest {
    self.input_digest
  }

  /// Returns the immutable deterministic policy digest.
  #[must_use]
  pub const fn policy_digest(&self) -> FactoryDigest {
    self.policy_digest
  }

  /// Returns the immutable call budget.
  #[must_use]
  pub const fn budget(&self) -> BudgetLimit {
    self.budget
  }
}

#[cfg(test)]
mod tests {
  use octacity_server_domain::{ArtifactId, ImmutableRevision, ProjectId, RepositoryId};

  use crate::{
    ExternalWorkIdentity, FactoryConfigurationId, FactoryConfigurationRef, FactoryConfigurationVersion,
    FactoryMetadata, FactoryRunId, RiskClass, WorkArtifacts, WorkClassification, WorkEnvelope, WorkEnvelopeId,
    WorkPriority,
  };

  use super::*;

  fn digest(byte: u8) -> FactoryDigest {
    FactoryDigest::from_bytes([byte; 32])
  }

  fn stage() -> StageAttempt {
    let subject = ExactSubject::new(
      ProjectId::generate(),
      RepositoryId::generate(),
      ImmutableRevision::new("base").expect("fixture revision"),
    );
    let configuration = FactoryConfigurationRef::new(
      FactoryConfigurationId::generate(),
      FactoryConfigurationVersion::INITIAL,
      subject.project_id(),
      digest(1),
    );
    let work = WorkEnvelope::new(
      WorkEnvelopeId::generate(),
      configuration,
      ExternalWorkIdentity::new("source/1").expect("fixture identity"),
      subject,
      WorkArtifacts::new(ArtifactId::generate(), ArtifactId::generate(), vec![]).expect("fixture artifacts"),
      WorkClassification::new(
        WorkPriority::new(0).expect("fixture priority"),
        RiskClass::Low,
        FactoryMetadata::default(),
      ),
    )
    .expect("fixture work");
    let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
    StageAttempt::new(
      StageAttemptId::generate(),
      &run,
      StageAttemptNumber::INITIAL,
      FactoryStageKind::Implementation,
      BudgetLimit::new(1, 1, 1, 1, 1).expect("fixture budget"),
      digest(2),
    )
  }

  #[test]
  fn macro_call_rejects_a_parent_from_another_stage() {
    let first_stage = stage();
    let parent = MacroCall::new(
      MacroCallId::generate(),
      &first_stage,
      MacroCallKind::Implement,
      digest(3),
      first_stage.budget(),
      None,
    )
    .expect("fixture parent");
    assert!(matches!(
      MacroCall::new(
        MacroCallId::generate(),
        &stage(),
        MacroCallKind::Summarize,
        digest(4),
        first_stage.budget(),
        Some(&parent),
      ),
      Err(FactoryError::InvalidReference { .. })
    ));
  }
}
