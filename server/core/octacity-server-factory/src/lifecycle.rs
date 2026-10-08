use std::collections::HashSet;

use octacity_server_domain::ProjectId;
use serde::{Deserialize, Serialize};

use crate::{
  CandidateSubject, Decision, DecisionId, DecisionOutcome, DeliveryAttemptId, DeliveryAttemptNumber, DeliveryState,
  EscalationId, ExactSubject, ExternalWorkIdentity, FactoryArtifactReference, FactoryConfigurationId,
  FactoryConfigurationVersion, FactoryDigest, FactoryError, FactoryKey, FactoryMetadata, FactoryRunId, FactoryRunState,
  FactoryRunVersion, FactoryText, ReportingAttemptId, ReportingAttemptNumber, ReportingState, RiskClass,
  WorkEnvelopeId, WorkPriority,
};

/// Maximum specification references carried by one Work Envelope.
pub const MAX_WORK_SPECIFICATION_REFERENCES: usize = 64;

/// Bounded admission classification attached to one Work Envelope.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkClassification {
  priority: WorkPriority,
  risk: RiskClass,
  metadata: FactoryMetadata,
}

impl WorkClassification {
  /// Constructs one typed admission classification.
  #[must_use]
  pub const fn new(priority: WorkPriority, risk: RiskClass, metadata: FactoryMetadata) -> Self {
    Self {
      priority,
      risk,
      metadata,
    }
  }

  /// Returns the bounded priority.
  #[must_use]
  pub const fn priority(&self) -> WorkPriority {
    self.priority
  }

  /// Returns the declared risk class.
  #[must_use]
  pub const fn risk(&self) -> RiskClass {
    self.risk
  }

  /// Returns bounded provider metadata.
  #[must_use]
  pub const fn metadata(&self) -> &FactoryMetadata {
    &self.metadata
  }
}

/// Exact immutable Factory Configuration version bound at admission.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryConfigurationRef {
  id: FactoryConfigurationId,
  version: FactoryConfigurationVersion,
  project_id: ProjectId,
  definition_digest: FactoryDigest,
}

impl FactoryConfigurationRef {
  /// Constructs an exact configuration reference.
  #[must_use]
  pub const fn new(
    id: FactoryConfigurationId,
    version: FactoryConfigurationVersion,
    project_id: ProjectId,
    definition_digest: FactoryDigest,
  ) -> Self {
    Self {
      id,
      version,
      project_id,
      definition_digest,
    }
  }

  /// Returns the stable configuration identity.
  #[must_use]
  pub const fn id(&self) -> FactoryConfigurationId {
    self.id
  }

  /// Returns the immutable configuration version.
  #[must_use]
  pub const fn version(&self) -> FactoryConfigurationVersion {
    self.version
  }

  /// Returns the owning Project.
  #[must_use]
  pub const fn project_id(&self) -> ProjectId {
    self.project_id
  }

  /// Returns the canonical definition digest.
  #[must_use]
  pub const fn definition_digest(&self) -> FactoryDigest {
    self.definition_digest
  }
}

/// Immutable Artifact references carried by one Work Envelope.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkArtifacts {
  task: FactoryArtifactReference,
  acceptance: FactoryArtifactReference,
  specifications: Vec<FactoryArtifactReference>,
}

impl WorkArtifacts {
  /// Constructs distinct, bounded Work input references.
  pub fn new(
    task: FactoryArtifactReference,
    acceptance: FactoryArtifactReference,
    specifications: Vec<FactoryArtifactReference>,
  ) -> Result<Self, FactoryError> {
    if task.artifact_id() == acceptance.artifact_id() {
      return Err(FactoryError::InvalidReference {
        relationship: "task and acceptance artifacts",
      });
    }
    validate_unique_artifacts(
      &specifications,
      MAX_WORK_SPECIFICATION_REFERENCES,
      "specification references",
    )?;
    if specifications.iter().any(|artifact| {
      artifact.artifact_id() == task.artifact_id() || artifact.artifact_id() == acceptance.artifact_id()
    }) {
      return Err(FactoryError::InvalidReference {
        relationship: "work artifact",
      });
    }
    Ok(Self {
      task,
      acceptance,
      specifications,
    })
  }

  /// Returns the task Artifact.
  #[must_use]
  pub const fn task(&self) -> &FactoryArtifactReference {
    &self.task
  }

  /// Returns the acceptance-criteria Artifact.
  #[must_use]
  pub const fn acceptance(&self) -> &FactoryArtifactReference {
    &self.acceptance
  }

  /// Returns the bounded specification Artifacts.
  #[must_use]
  pub fn specifications(&self) -> &[FactoryArtifactReference] {
    &self.specifications
  }
}

/// Immutable normalized Work admitted to the Factory lifecycle.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkEnvelope {
  id: WorkEnvelopeId,
  configuration: FactoryConfigurationRef,
  external_identity: ExternalWorkIdentity,
  subject: ExactSubject,
  artifacts: WorkArtifacts,
  priority: WorkPriority,
  risk: RiskClass,
  metadata: FactoryMetadata,
}

impl WorkEnvelope {
  /// Constructs one bounded Work Envelope owned by its configuration Project.
  pub fn new(
    id: WorkEnvelopeId,
    configuration: FactoryConfigurationRef,
    external_identity: ExternalWorkIdentity,
    subject: ExactSubject,
    artifacts: WorkArtifacts,
    classification: WorkClassification,
  ) -> Result<Self, FactoryError> {
    if configuration.project_id != subject.project_id() {
      return Err(FactoryError::InconsistentSubject);
    }
    Ok(Self {
      id,
      configuration,
      external_identity,
      subject,
      artifacts,
      priority: classification.priority,
      risk: classification.risk,
      metadata: classification.metadata,
    })
  }

  /// Returns the Work identity.
  #[must_use]
  pub const fn id(&self) -> WorkEnvelopeId {
    self.id
  }

  /// Returns the exact configuration selected at admission.
  #[must_use]
  pub const fn configuration(&self) -> &FactoryConfigurationRef {
    &self.configuration
  }

  /// Returns the external idempotency identity.
  #[must_use]
  pub const fn external_identity(&self) -> &ExternalWorkIdentity {
    &self.external_identity
  }

  /// Returns the exact source subject.
  #[must_use]
  pub const fn subject(&self) -> &ExactSubject {
    &self.subject
  }

  /// Returns the Work input Artifacts.
  #[must_use]
  pub const fn artifacts(&self) -> &WorkArtifacts {
    &self.artifacts
  }

  /// Returns the bounded priority.
  #[must_use]
  pub const fn priority(&self) -> WorkPriority {
    self.priority
  }

  /// Returns the declared risk class.
  #[must_use]
  pub const fn risk(&self) -> RiskClass {
    self.risk
  }

  /// Returns bounded provider metadata.
  #[must_use]
  pub const fn metadata(&self) -> &FactoryMetadata {
    &self.metadata
  }
}

/// Durable Factory Run identity, exact input, state, and aggregate version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRun {
  id: FactoryRunId,
  configuration: FactoryConfigurationRef,
  work_id: WorkEnvelopeId,
  subject: ExactSubject,
  state: FactoryRunState,
  version: FactoryRunVersion,
}

impl FactoryRun {
  /// Starts a newly admitted Run from one immutable Work Envelope.
  #[must_use]
  pub fn admitted(id: FactoryRunId, work: &WorkEnvelope) -> Self {
    Self {
      id,
      configuration: work.configuration.clone(),
      work_id: work.id,
      subject: work.subject.clone(),
      state: FactoryRunState::Admitted,
      version: FactoryRunVersion::INITIAL,
    }
  }

  /// Restores a Run while validating redundant persisted identities.
  pub fn restore(
    id: FactoryRunId,
    configuration: FactoryConfigurationRef,
    work: &WorkEnvelope,
    subject: ExactSubject,
    state: FactoryRunState,
    version: FactoryRunVersion,
  ) -> Result<Self, FactoryError> {
    if configuration != work.configuration || subject != work.subject {
      return Err(FactoryError::InconsistentSubject);
    }
    Ok(Self {
      id,
      configuration,
      work_id: work.id,
      subject,
      state,
      version,
    })
  }

  /// Returns the Run identity.
  #[must_use]
  pub const fn id(&self) -> FactoryRunId {
    self.id
  }

  /// Returns the exact admitted configuration.
  #[must_use]
  pub const fn configuration(&self) -> &FactoryConfigurationRef {
    &self.configuration
  }

  /// Returns the admitted Work identity.
  #[must_use]
  pub const fn work_id(&self) -> WorkEnvelopeId {
    self.work_id
  }

  /// Returns the immutable base subject.
  #[must_use]
  pub const fn subject(&self) -> &ExactSubject {
    &self.subject
  }

  /// Returns the durable lifecycle state.
  #[must_use]
  pub const fn state(&self) -> FactoryRunState {
    self.state
  }

  /// Returns the optimistic aggregate version.
  #[must_use]
  pub const fn version(&self) -> FactoryRunVersion {
    self.version
  }
}

/// Immutable record that a Run requires human or deterministic-policy disposition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Escalation {
  id: EscalationId,
  run_id: FactoryRunId,
  subject: ExactSubject,
  decision_id: Option<DecisionId>,
  reason: FactoryText,
}

impl Escalation {
  /// Constructs an escalation tied to the exact Run subject.
  pub fn new(
    id: EscalationId,
    run: &FactoryRun,
    decision: Option<&Decision>,
    reason: FactoryText,
  ) -> Result<Self, FactoryError> {
    if decision.is_some_and(|value| value.subject().exact() != run.subject()) {
      return Err(FactoryError::InconsistentSubject);
    }
    if decision.is_some_and(|value| value.outcome() != DecisionOutcome::Escalate) {
      return Err(FactoryError::InvalidReference {
        relationship: "escalation decision",
      });
    }
    Ok(Self {
      id,
      run_id: run.id,
      subject: run.subject.clone(),
      decision_id: decision.map(Decision::id),
      reason,
    })
  }

  /// Returns the escalation identity.
  #[must_use]
  pub const fn id(&self) -> EscalationId {
    self.id
  }

  /// Returns the escalated Run identity.
  #[must_use]
  pub const fn run_id(&self) -> FactoryRunId {
    self.run_id
  }

  /// Returns the exact Run subject.
  #[must_use]
  pub const fn subject(&self) -> &ExactSubject {
    &self.subject
  }

  /// Returns the related Decision when escalation followed evaluation.
  #[must_use]
  pub const fn decision_id(&self) -> Option<DecisionId> {
    self.decision_id
  }

  /// Returns the bounded escalation reason.
  #[must_use]
  pub const fn reason(&self) -> &FactoryText {
    &self.reason
  }
}

/// Append-only delivery-for-review attempt for one candidate.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeliveryAttempt {
  id: DeliveryAttemptId,
  decision_id: DecisionId,
  subject: CandidateSubject,
  number: DeliveryAttemptNumber,
  state: DeliveryState,
  target: FactoryKey,
}

impl DeliveryAttempt {
  /// Constructs a delivery attempt from an exact Decision.
  pub fn new(
    id: DeliveryAttemptId,
    decision: &Decision,
    number: DeliveryAttemptNumber,
    state: DeliveryState,
    target: FactoryKey,
  ) -> Result<Self, FactoryError> {
    if decision.outcome() != DecisionOutcome::Accept {
      return Err(FactoryError::InvalidReference {
        relationship: "delivery decision",
      });
    }
    Ok(Self {
      id,
      decision_id: decision.id(),
      subject: decision.subject().clone(),
      number,
      state,
      target,
    })
  }

  /// Returns the delivery identity.
  #[must_use]
  pub const fn id(&self) -> DeliveryAttemptId {
    self.id
  }

  /// Returns the authorizing Decision identity.
  #[must_use]
  pub const fn decision_id(&self) -> DecisionId {
    self.decision_id
  }

  /// Returns the exact candidate.
  #[must_use]
  pub const fn subject(&self) -> &CandidateSubject {
    &self.subject
  }

  /// Returns the append-only attempt number.
  #[must_use]
  pub const fn number(&self) -> DeliveryAttemptNumber {
    self.number
  }

  /// Returns the terminal attempt classification.
  #[must_use]
  pub const fn state(&self) -> DeliveryState {
    self.state
  }

  /// Returns the selected delivery adapter target.
  #[must_use]
  pub const fn target(&self) -> &FactoryKey {
    &self.target
  }
}

/// Append-only attempt to project authoritative Run state to an external system.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReportingAttempt {
  id: ReportingAttemptId,
  run_id: FactoryRunId,
  subject: ExactSubject,
  number: ReportingAttemptNumber,
  state: ReportingState,
  reporter: FactoryKey,
}

impl ReportingAttempt {
  /// Constructs a reporter attempt from the authoritative Run.
  #[must_use]
  pub fn new(
    id: ReportingAttemptId,
    run: &FactoryRun,
    number: ReportingAttemptNumber,
    state: ReportingState,
    reporter: FactoryKey,
  ) -> Self {
    Self {
      id,
      run_id: run.id,
      subject: run.subject.clone(),
      number,
      state,
      reporter,
    }
  }

  /// Returns the reporting-attempt identity.
  #[must_use]
  pub const fn id(&self) -> ReportingAttemptId {
    self.id
  }

  /// Returns the reported Run identity.
  #[must_use]
  pub const fn run_id(&self) -> FactoryRunId {
    self.run_id
  }

  /// Returns the immutable Run subject.
  #[must_use]
  pub const fn subject(&self) -> &ExactSubject {
    &self.subject
  }

  /// Returns the append-only attempt number.
  #[must_use]
  pub const fn number(&self) -> ReportingAttemptNumber {
    self.number
  }

  /// Returns the terminal attempt classification.
  #[must_use]
  pub const fn state(&self) -> ReportingState {
    self.state
  }

  /// Returns the reporter adapter identity.
  #[must_use]
  pub const fn reporter(&self) -> &FactoryKey {
    &self.reporter
  }
}

fn validate_unique_artifacts(
  artifacts: &[FactoryArtifactReference],
  limit: usize,
  collection: &'static str,
) -> Result<(), FactoryError> {
  if artifacts.len() > limit {
    return Err(FactoryError::CollectionLimitExceeded { collection });
  }
  if artifacts
    .iter()
    .map(FactoryArtifactReference::artifact_id)
    .collect::<HashSet<_>>()
    .len()
    != artifacts.len()
  {
    return Err(FactoryError::InvalidReference {
      relationship: collection,
    });
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use octacity_server_domain::{ArtifactId, ImmutableRevision, RepositoryId};

  use super::*;

  fn digest(byte: u8) -> FactoryDigest {
    FactoryDigest::from_bytes([byte; 32])
  }

  fn artifact(byte: u8) -> FactoryArtifactReference {
    FactoryArtifactReference::new(ArtifactId::generate(), digest(byte), 1).expect("fixture artifact")
  }

  fn subject() -> ExactSubject {
    ExactSubject::new(
      ProjectId::generate(),
      RepositoryId::generate(),
      ImmutableRevision::new("base").expect("fixture revision"),
    )
  }

  fn work(subject: ExactSubject) -> WorkEnvelope {
    let configuration = FactoryConfigurationRef::new(
      FactoryConfigurationId::generate(),
      FactoryConfigurationVersion::INITIAL,
      subject.project_id(),
      digest(1),
    );
    WorkEnvelope::new(
      WorkEnvelopeId::generate(),
      configuration,
      ExternalWorkIdentity::new("tracker/work-1").expect("fixture identity"),
      subject,
      WorkArtifacts::new(artifact(90), artifact(91), vec![]).expect("fixture artifacts"),
      WorkClassification::new(
        WorkPriority::new(10).expect("fixture priority"),
        RiskClass::Medium,
        FactoryMetadata::default(),
      ),
    )
    .expect("fixture work")
  }

  #[test]
  fn work_artifacts_reject_duplicate_references_and_oversized_collections() {
    let duplicate = artifact(92);
    assert!(matches!(
      WorkArtifacts::new(duplicate.clone(), duplicate, vec![]),
      Err(FactoryError::InvalidReference { .. })
    ));
    assert!(matches!(
      WorkArtifacts::new(
        artifact(93),
        artifact(94),
        (0..=MAX_WORK_SPECIFICATION_REFERENCES).map(|_| artifact(95)).collect(),
      ),
      Err(FactoryError::CollectionLimitExceeded { .. })
    ));
  }

  #[test]
  fn run_restore_rejects_inconsistent_exact_subjects() {
    let admitted_work = work(subject());
    assert_eq!(
      FactoryRun::restore(
        FactoryRunId::generate(),
        admitted_work.configuration.clone(),
        &admitted_work,
        subject(),
        FactoryRunState::Implementing,
        FactoryRunVersion::INITIAL,
      ),
      Err(FactoryError::InconsistentSubject)
    );
  }
}
