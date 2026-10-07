use std::collections::HashSet;

use octacity_server_domain::ArtifactId;

use crate::{
  AssessmentId, AssessmentOutcome, CandidateSubject, ChangeSetId, DecisionId, DecisionOutcome, EvaluationPlanId,
  EvidenceManifestId, FactoryDigest, FactoryError, FactoryKey, FactoryText, FindingSeverity, StageAttempt,
  StageAttemptId,
};

/// Maximum typed evidence items in one manifest.
pub const MAX_EVIDENCE_ITEMS: usize = 256;
/// Maximum criterion packs selected by one Evaluation Plan.
pub const MAX_CRITERION_PACKS: usize = 32;
/// Maximum evaluator branches selected by one Evaluation Plan.
pub const MAX_EVALUATORS: usize = 32;
/// Maximum findings returned by one Assessment.
pub const MAX_ASSESSMENT_FINDINGS: usize = 128;
/// Maximum Assessments consumed by one Decision.
pub const MAX_DECISION_ASSESSMENTS: usize = 64;
/// Maximum bounded reasons recorded by one Decision.
pub const MAX_DECISION_REASONS: usize = MAX_CRITERION_PACKS + MAX_EVALUATORS + 1;

/// Trusted immutable source candidate captured after a writable Stage Attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangeSet {
  id: ChangeSetId,
  stage_attempt_id: StageAttemptId,
  subject: CandidateSubject,
  bundle_artifact: ArtifactId,
  manifest_artifact: ArtifactId,
}

impl ChangeSet {
  /// Constructs a ChangeSet bound to the Stage Attempt's exact base subject.
  pub fn new(
    id: ChangeSetId,
    stage: &StageAttempt,
    subject: CandidateSubject,
    bundle_artifact: ArtifactId,
    manifest_artifact: ArtifactId,
  ) -> Result<Self, FactoryError> {
    if subject.exact() != stage.subject() {
      return Err(FactoryError::InconsistentSubject);
    }
    if !matches!(
      stage.kind(),
      crate::FactoryStageKind::Implementation | crate::FactoryStageKind::Rework
    ) {
      return Err(FactoryError::InvalidReference {
        relationship: "changeset producing stage",
      });
    }
    if bundle_artifact == manifest_artifact {
      return Err(FactoryError::InvalidReference {
        relationship: "changeset artifacts",
      });
    }
    Ok(Self {
      id,
      stage_attempt_id: stage.id(),
      subject,
      bundle_artifact,
      manifest_artifact,
    })
  }

  /// Returns the ChangeSet identity.
  #[must_use]
  pub const fn id(&self) -> ChangeSetId {
    self.id
  }

  /// Returns the producing Stage Attempt.
  #[must_use]
  pub const fn stage_attempt_id(&self) -> StageAttemptId {
    self.stage_attempt_id
  }

  /// Returns the exact candidate subject.
  #[must_use]
  pub const fn subject(&self) -> &CandidateSubject {
    &self.subject
  }

  /// Returns the immutable candidate bundle Artifact.
  #[must_use]
  pub const fn bundle_artifact(&self) -> ArtifactId {
    self.bundle_artifact
  }

  /// Returns the immutable capture-manifest Artifact.
  #[must_use]
  pub const fn manifest_artifact(&self) -> ArtifactId {
    self.manifest_artifact
  }
}

/// One typed, content-addressed report or Artifact in an Evidence Manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceItem {
  kind: FactoryKey,
  artifact_id: ArtifactId,
  digest: FactoryDigest,
}

impl EvidenceItem {
  /// Constructs one typed evidence reference.
  #[must_use]
  pub const fn new(kind: FactoryKey, artifact_id: ArtifactId, digest: FactoryDigest) -> Self {
    Self {
      kind,
      artifact_id,
      digest,
    }
  }

  /// Returns the stable evidence kind.
  #[must_use]
  pub const fn kind(&self) -> &FactoryKey {
    &self.kind
  }

  /// Returns the immutable evidence Artifact.
  #[must_use]
  pub const fn artifact_id(&self) -> ArtifactId {
    self.artifact_id
  }

  /// Returns the exact evidence content digest.
  #[must_use]
  pub const fn digest(&self) -> FactoryDigest {
    self.digest
  }
}

/// Immutable bounded deterministic evidence for one exact candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceManifest {
  id: EvidenceManifestId,
  changeset_id: ChangeSetId,
  subject: CandidateSubject,
  items: Vec<EvidenceItem>,
}

impl EvidenceManifest {
  /// Constructs a non-empty manifest for the exact ChangeSet candidate.
  pub fn new(
    id: EvidenceManifestId,
    changeset: &ChangeSet,
    subject: CandidateSubject,
    items: Vec<EvidenceItem>,
  ) -> Result<Self, FactoryError> {
    if subject != changeset.subject {
      return Err(FactoryError::InconsistentSubject);
    }
    validate_count(&items, 1, MAX_EVIDENCE_ITEMS, "evidence items")?;
    if items
      .iter()
      .map(EvidenceItem::artifact_id)
      .collect::<HashSet<_>>()
      .len()
      != items.len()
    {
      return Err(FactoryError::InvalidReference {
        relationship: "evidence artifact",
      });
    }
    Ok(Self {
      id,
      changeset_id: changeset.id,
      subject,
      items,
    })
  }

  /// Returns the Evidence Manifest identity.
  #[must_use]
  pub const fn id(&self) -> EvidenceManifestId {
    self.id
  }

  /// Returns the exact ChangeSet identity.
  #[must_use]
  pub const fn changeset_id(&self) -> ChangeSetId {
    self.changeset_id
  }

  /// Returns the exact candidate subject.
  #[must_use]
  pub const fn subject(&self) -> &CandidateSubject {
    &self.subject
  }

  /// Returns the bounded evidence items.
  #[must_use]
  pub fn items(&self) -> &[EvidenceItem] {
    &self.items
  }
}

/// Immutable selection of criterion packs and evaluator branches for one candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationPlan {
  id: EvaluationPlanId,
  evidence_id: EvidenceManifestId,
  subject: CandidateSubject,
  criterion_packs: Vec<FactoryKey>,
  evaluators: Vec<FactoryKey>,
}

impl EvaluationPlan {
  /// Constructs a bounded plan tied to one exact Evidence Manifest.
  pub fn new(
    id: EvaluationPlanId,
    evidence: &EvidenceManifest,
    subject: CandidateSubject,
    criterion_packs: Vec<FactoryKey>,
    evaluators: Vec<FactoryKey>,
  ) -> Result<Self, FactoryError> {
    if subject != evidence.subject {
      return Err(FactoryError::InconsistentSubject);
    }
    validate_unique_keys(&criterion_packs, MAX_CRITERION_PACKS, "criterion packs")?;
    validate_unique_keys(&evaluators, MAX_EVALUATORS, "evaluators")?;
    Ok(Self {
      id,
      evidence_id: evidence.id,
      subject,
      criterion_packs,
      evaluators,
    })
  }

  /// Returns the Evaluation Plan identity.
  #[must_use]
  pub const fn id(&self) -> EvaluationPlanId {
    self.id
  }

  /// Returns the exact Evidence Manifest identity.
  #[must_use]
  pub const fn evidence_id(&self) -> EvidenceManifestId {
    self.evidence_id
  }

  /// Returns the exact candidate subject.
  #[must_use]
  pub const fn subject(&self) -> &CandidateSubject {
    &self.subject
  }

  /// Returns the selected immutable criterion-pack keys.
  #[must_use]
  pub fn criterion_packs(&self) -> &[FactoryKey] {
    &self.criterion_packs
  }

  /// Returns the selected evaluator keys.
  #[must_use]
  pub fn evaluators(&self) -> &[FactoryKey] {
    &self.evaluators
  }
}

/// One schema-valid typed finding returned by an evaluator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssessmentFinding {
  /// A policy-relevant violation with an ordered severity.
  Violation {
    /// Severity interpreted only by deterministic policy.
    severity: FindingSeverity,
    /// Bounded operator-visible summary; it never selects an outcome directly.
    summary: FactoryText,
  },
  /// Missing or insufficient evidence prevented a determination.
  EvidenceGap {
    /// Bounded operator-visible description of the evidence gap.
    summary: FactoryText,
  },
}

impl AssessmentFinding {
  /// Returns violation severity, or `None` for an evidence gap.
  #[must_use]
  pub const fn severity(&self) -> Option<FindingSeverity> {
    match self {
      Self::Violation { severity, .. } => Some(*severity),
      Self::EvidenceGap { .. } => None,
    }
  }

  /// Returns the bounded operator-visible summary.
  #[must_use]
  pub const fn summary(&self) -> &FactoryText {
    match self {
      Self::Violation { summary, .. } | Self::EvidenceGap { summary } => summary,
    }
  }
}

/// Immutable schema-valid evaluator result for one exact Evaluation Plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Assessment {
  id: AssessmentId,
  plan_id: EvaluationPlanId,
  subject: CandidateSubject,
  evaluator: FactoryKey,
  outcome: AssessmentOutcome,
  findings: Vec<AssessmentFinding>,
}

impl Assessment {
  /// Constructs an Assessment from an evaluator declared by the plan.
  pub fn new(
    id: AssessmentId,
    plan: &EvaluationPlan,
    subject: CandidateSubject,
    evaluator: FactoryKey,
    outcome: AssessmentOutcome,
    findings: Vec<AssessmentFinding>,
  ) -> Result<Self, FactoryError> {
    if subject != plan.subject {
      return Err(FactoryError::InconsistentSubject);
    }
    if !plan.evaluators.contains(&evaluator) {
      return Err(FactoryError::InvalidReference {
        relationship: "assessment evaluator",
      });
    }
    if findings.len() > MAX_ASSESSMENT_FINDINGS {
      return Err(FactoryError::CollectionLimitExceeded {
        collection: "assessment findings",
      });
    }
    let findings_match_outcome = match outcome {
      AssessmentOutcome::Satisfied => findings.is_empty(),
      AssessmentOutcome::Violated => {
        !findings.is_empty() && findings.iter().all(|finding| finding.severity().is_some())
      }
      AssessmentOutcome::Indeterminate => {
        !findings.is_empty() && findings.iter().all(|finding| finding.severity().is_none())
      }
    };
    if !findings_match_outcome {
      return Err(FactoryError::InvalidReference {
        relationship: "assessment outcome findings",
      });
    }
    Ok(Self {
      id,
      plan_id: plan.id,
      subject,
      evaluator,
      outcome,
      findings,
    })
  }

  /// Returns the Assessment identity.
  #[must_use]
  pub const fn id(&self) -> AssessmentId {
    self.id
  }

  /// Returns the Evaluation Plan identity.
  #[must_use]
  pub const fn plan_id(&self) -> EvaluationPlanId {
    self.plan_id
  }

  /// Returns the exact candidate subject.
  #[must_use]
  pub const fn subject(&self) -> &CandidateSubject {
    &self.subject
  }

  /// Returns the selected evaluator identity.
  #[must_use]
  pub const fn evaluator(&self) -> &FactoryKey {
    &self.evaluator
  }

  /// Returns the schema-valid outcome.
  #[must_use]
  pub const fn outcome(&self) -> AssessmentOutcome {
    self.outcome
  }

  /// Returns bounded typed finding summaries.
  #[must_use]
  pub fn findings(&self) -> &[AssessmentFinding] {
    &self.findings
  }
}

/// Typed deterministic reason recorded by the Decision Engine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DecisionReason {
  /// Every mandatory gate and evaluator policy was satisfied.
  PolicySatisfied,
  /// Required deterministic evidence was not supplied.
  RequiredEvidenceMissing(FactoryKey),
  /// Required deterministic evidence recorded failure.
  RequiredEvidenceFailed(FactoryKey),
  /// Required deterministic evidence could not produce a result.
  RequiredEvidenceIndeterminate(FactoryKey),
  /// A required evaluator Assessment is absent.
  RequiredAssessmentMissing(FactoryKey),
  /// An evaluator Assessment is indeterminate under the immutable policy.
  AssessmentIndeterminate(FactoryKey),
  /// A typed violation met or exceeded the immutable severity threshold.
  SeverityThresholdExceeded {
    /// Evaluator whose immutable Assessment contained the violation.
    evaluator: FactoryKey,
    /// Highest policy-relevant severity reported by that evaluator.
    severity: FindingSeverity,
  },
  /// Too few distinct declared evaluator Assessments reached the join.
  QuorumNotMet {
    /// Immutable required number of Assessments.
    required: u16,
    /// Number of distinct valid Assessments supplied.
    observed: u16,
  },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DecisionBindings {
  pub(crate) input_digest: FactoryDigest,
  pub(crate) policy_version: crate::DecisionPolicyVersion,
  pub(crate) policy_digest: FactoryDigest,
}

/// Immutable deterministic candidate disposition input to later lifecycle policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Decision {
  id: DecisionId,
  plan_id: EvaluationPlanId,
  subject: CandidateSubject,
  outcome: DecisionOutcome,
  assessment_ids: Vec<AssessmentId>,
  reasons: Vec<DecisionReason>,
  input_digest: FactoryDigest,
  policy_version: crate::DecisionPolicyVersion,
  policy_digest: FactoryDigest,
}

impl Decision {
  pub(crate) fn from_engine(
    id: DecisionId,
    plan: &EvaluationPlan,
    outcome: DecisionOutcome,
    assessments: &[Assessment],
    reasons: Vec<DecisionReason>,
    bindings: DecisionBindings,
  ) -> Result<Self, FactoryError> {
    if assessments.len() > MAX_DECISION_ASSESSMENTS {
      return Err(FactoryError::CollectionLimitExceeded {
        collection: "decision assessments",
      });
    }
    validate_count(&reasons, 1, MAX_DECISION_REASONS, "decision reasons")?;
    if assessments
      .iter()
      .any(|assessment| assessment.plan_id != plan.id || assessment.subject != plan.subject)
    {
      return Err(FactoryError::InconsistentSubject);
    }
    let assessment_ids: Vec<_> = assessments.iter().map(Assessment::id).collect();
    if assessment_ids.iter().copied().collect::<HashSet<_>>().len() != assessment_ids.len() {
      return Err(FactoryError::InvalidReference {
        relationship: "decision assessment",
      });
    }
    Ok(Self {
      id,
      plan_id: plan.id,
      subject: plan.subject.clone(),
      outcome,
      assessment_ids,
      reasons,
      input_digest: bindings.input_digest,
      policy_version: bindings.policy_version,
      policy_digest: bindings.policy_digest,
    })
  }

  /// Returns the Decision identity.
  #[must_use]
  pub const fn id(&self) -> DecisionId {
    self.id
  }

  /// Returns the Evaluation Plan identity.
  #[must_use]
  pub const fn plan_id(&self) -> EvaluationPlanId {
    self.plan_id
  }

  /// Returns the exact candidate subject.
  #[must_use]
  pub const fn subject(&self) -> &CandidateSubject {
    &self.subject
  }

  /// Returns the deterministic outcome.
  #[must_use]
  pub const fn outcome(&self) -> DecisionOutcome {
    self.outcome
  }

  /// Returns the consumed Assessment identities.
  #[must_use]
  pub fn assessment_ids(&self) -> &[AssessmentId] {
    &self.assessment_ids
  }

  /// Returns the bounded decision reasons.
  #[must_use]
  pub fn reasons(&self) -> &[DecisionReason] {
    &self.reasons
  }

  /// Returns the canonical deterministic input digest.
  #[must_use]
  pub const fn input_digest(&self) -> FactoryDigest {
    self.input_digest
  }

  /// Returns the immutable Decision policy version.
  #[must_use]
  pub const fn policy_version(&self) -> crate::DecisionPolicyVersion {
    self.policy_version
  }

  /// Returns the exact immutable Decision policy digest.
  #[must_use]
  pub const fn policy_digest(&self) -> FactoryDigest {
    self.policy_digest
  }
}

fn validate_count<T>(
  values: &[T],
  minimum: usize,
  maximum: usize,
  collection: &'static str,
) -> Result<(), FactoryError> {
  if values.len() < minimum {
    return Err(FactoryError::InvalidReference {
      relationship: collection,
    });
  }
  if values.len() > maximum {
    return Err(FactoryError::CollectionLimitExceeded { collection });
  }
  Ok(())
}

fn validate_unique_keys(values: &[FactoryKey], maximum: usize, collection: &'static str) -> Result<(), FactoryError> {
  validate_count(values, 1, maximum, collection)?;
  if values.iter().collect::<HashSet<_>>().len() != values.len() {
    return Err(FactoryError::InvalidReference {
      relationship: collection,
    });
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use octacity_server_domain::{ArtifactId, ImmutableRevision, ProjectId, RepositoryId};

  use crate::{
    BudgetLimit, FactoryConfigurationId, FactoryConfigurationRef, FactoryConfigurationVersion, FactoryRun,
    FactoryRunId, FactoryStageKind, StageAttemptNumber, WorkArtifacts, WorkClassification, WorkEnvelope,
    WorkEnvelopeId, WorkPriority,
  };

  use super::*;

  fn digest(byte: u8) -> FactoryDigest {
    FactoryDigest::from_bytes([byte; 32])
  }

  fn stage() -> StageAttempt {
    let subject = crate::ExactSubject::new(
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
      crate::ExternalWorkIdentity::new("source/1").expect("fixture identity"),
      subject,
      WorkArtifacts::new(ArtifactId::generate(), ArtifactId::generate(), vec![]).expect("fixture artifacts"),
      WorkClassification::new(
        WorkPriority::new(1).expect("fixture priority"),
        crate::RiskClass::Low,
        crate::FactoryMetadata::default(),
      ),
    )
    .expect("fixture work");
    StageAttempt::new(
      crate::StageAttemptId::generate(),
      &FactoryRun::admitted(FactoryRunId::generate(), &work),
      StageAttemptNumber::INITIAL,
      FactoryStageKind::Implementation,
      BudgetLimit::new(1, 1, 1, 1, 1).expect("fixture budget"),
      digest(2),
      crate::FactoryClaimOwnership::new(
        crate::FactoryKey::new("worker").expect("fixture owner"),
        crate::FactoryClaim::new(
          crate::FactoryClaimFence::new(digest(9)),
          octacity_server_domain::Timestamp::from_unix_millis(1).expect("fixture claim start"),
          octacity_server_domain::Timestamp::from_unix_millis(2).expect("fixture claim deadline"),
        )
        .expect("fixture claim"),
      ),
    )
  }

  #[test]
  fn changeset_rejects_a_candidate_for_another_exact_subject() {
    let stage = stage();
    let other = crate::ExactSubject::new(
      ProjectId::generate(),
      RepositoryId::generate(),
      ImmutableRevision::new("other").expect("fixture revision"),
    );
    assert_eq!(
      ChangeSet::new(
        ChangeSetId::generate(),
        &stage,
        CandidateSubject::new(
          other,
          ImmutableRevision::new("candidate").expect("fixture revision"),
          digest(3),
        ),
        ArtifactId::generate(),
        ArtifactId::generate(),
      ),
      Err(FactoryError::InconsistentSubject)
    );
  }

  #[test]
  fn evaluation_plan_rejects_duplicate_and_empty_references() {
    let stage = stage();
    let candidate = CandidateSubject::new(
      stage.subject().clone(),
      ImmutableRevision::new("candidate").expect("fixture revision"),
      digest(3),
    );
    let changeset = ChangeSet::new(
      ChangeSetId::generate(),
      &stage,
      candidate.clone(),
      ArtifactId::generate(),
      ArtifactId::generate(),
    )
    .expect("fixture changeset");
    let evidence = EvidenceManifest::new(
      EvidenceManifestId::generate(),
      &changeset,
      candidate.clone(),
      vec![EvidenceItem::new(
        FactoryKey::new("tests").expect("fixture key"),
        ArtifactId::generate(),
        digest(4),
      )],
    )
    .expect("fixture evidence");
    let duplicate = FactoryKey::new("architecture").expect("fixture key");
    assert!(matches!(
      EvaluationPlan::new(
        EvaluationPlanId::generate(),
        &evidence,
        candidate,
        vec![duplicate.clone(), duplicate],
        vec![FactoryKey::new("evaluator").expect("fixture key")],
      ),
      Err(FactoryError::InvalidReference { .. })
    ));
  }
}
