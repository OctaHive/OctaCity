use octacity_server_domain::Timestamp;
use serde::{Deserialize, Serialize};

use crate::{
  ExactSubject, FactoryArtifactReference, FactoryConfigurationRef, FactoryDigest, FactoryError, FactoryKey,
  FindingSeverity, FlowDefinitionRef, ImmutableReference, NodeAttemptId, RiskClass, WorkEnvelope, WorkEnvelopeId,
};

use super::{canonicalize, digest, invalid};

/// Maximum duplicate candidates or dependencies retained by one triage phase.
pub const MAX_TRIAGE_OBSERVATIONS: usize = 64;
/// Version of the provider-neutral two-phase triage contracts.
pub const TRIAGE_CONTRACT_VERSION: u16 = 1;

macro_rules! triage_enum {
  ($name:ident, $doc:literal, {$($variant:ident => $variant_doc:literal),+ $(,)?}) => {
    #[doc = $doc]
    #[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
    #[serde(rename_all = "snake_case")]
    pub enum $name {
      $(#[doc = $variant_doc] $variant,)+
    }
  };
}

triage_enum!(WorkKind, "Work purpose observed by classification.", {
  Defect => "A defect in existing behavior.",
  FeatureRequest => "A requested new capability.",
});
triage_enum!(ProjectFit, "Observed fit with the exact Project goals.", {
  InScope => "The Work fits the Project goals.",
  OutOfScope => "The Work falls outside the Project goals.",
  Inconclusive => "Fit needs human clarification.",
});
triage_enum!(DuplicateStatus, "Observed relationship to one frozen duplicate candidate.", {
  Distinct => "The candidate is distinct.",
  Duplicate => "The candidate describes the same Work.",
  Inconclusive => "The relationship is unresolved.",
});
triage_enum!(WorkSize, "Finite ordered classification size.", {
  Small => "Bounded Work that may satisfy specification-bypass policy.",
  Medium => "Work that requires authored requirements.",
  Large => "Work that requires authored requirements and broader planning.",
});
triage_enum!(PreliminaryReproducibility, "Preliminary defect reproduction observation, never implementation authority.", {
  Reproduced => "A deterministic reproduction was observed.",
  Intermittent => "Reproduction is intermittent.",
  EnvironmentSpecific => "Reproduction depends on an environment contract.",
  CannotReproduce => "The available attempt could not reproduce the defect.",
  Inconclusive => "Reproduction has not been established.",
  NeedsHumanInput => "Required reproduction inputs need human clarification.",
  NotApplicable => "Feature-request classification has no defect reproduction claim.",
});
triage_enum!(TriageRoute, "Finite recommendation and deterministic triage route vocabulary.", {
  Research => "Gather bounded defect or feature research.",
  Requirements => "Author and review requirements.",
  ProtectedTest => "Create protected tests through the declared gates.",
  Development => "Enter the declared development flow.",
  VerificationOnly => "Verify existing behavior without implementation.",
  AlreadyFixed => "Resolve Work satisfied by an existing change.",
  Escalation => "Request human disposition.",
  Rejection => "Reject the Work with retained evidence.",
});

impl TriageRoute {
  /// Every supported route, in a stable order.
  pub const ALL: [Self; 8] = [
    Self::Research,
    Self::Requirements,
    Self::ProtectedTest,
    Self::Development,
    Self::VerificationOnly,
    Self::AlreadyFixed,
    Self::Escalation,
    Self::Rejection,
  ];

  /// Returns the stable Flow outcome key.
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Research => "research",
      Self::Requirements => "requirements",
      Self::ProtectedTest => "protected_test",
      Self::Development => "development",
      Self::VerificationOnly => "verification_only",
      Self::AlreadyFixed => "already_fixed",
      Self::Escalation => "escalation",
      Self::Rejection => "rejection",
    }
  }
}

/// One typed, non-authoritative observation with retained exact evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TriageObservation<T> {
  value: T,
  evidence: FactoryArtifactReference,
}

impl<T> TriageObservation<T> {
  /// Binds a typed observation to its immutable source Artifact.
  #[must_use]
  pub const fn new(value: T, evidence: FactoryArtifactReference) -> Self {
    Self { value, evidence }
  }

  /// Borrows the observed value.
  #[must_use]
  pub const fn value(&self) -> &T {
    &self.value
  }

  /// Returns the exact evidence retained for this observation.
  #[must_use]
  pub const fn evidence(&self) -> &FactoryArtifactReference {
    &self.evidence
  }
}

/// Exact producer and input provenance shared by all observations in a phase result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TriageProvenance {
  /// Digest of the complete canonical phase input.
  pub input_digest: FactoryDigest,
  /// Pinned nested definition executed by the phase.
  pub definition: FlowDefinitionRef,
  /// Immutable Node Attempt that produced the observations.
  pub node_attempt_id: NodeAttemptId,
  /// Exact selected harness, tool, or producer identity.
  pub producer: ImmutableReference,
  /// Exact selected model or deterministic tool identity.
  pub model_or_tool: ImmutableReference,
  /// Exact task/prompt policy digest.
  pub task_digest: FactoryDigest,
  /// Retained raw typed result, separate from its canonical domain record.
  pub result: FactoryArtifactReference,
  /// Timestamp of the recorded observation.
  pub observed_at: Timestamp,
}

/// One Project-owned immutable Work candidate frozen before eligibility execution.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DuplicateCandidate {
  /// Exact candidate Work identity.
  pub work_id: WorkEnvelopeId,
  /// Project, Repository, and revision of the candidate.
  pub subject: ExactSubject,
  /// Retained candidate facts used by duplicate assessment.
  pub evidence: FactoryArtifactReference,
}

/// Typed immutable inputs to the first triage phase.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "EligibilityInputWire")]
pub struct EligibilityInput {
  work_id: WorkEnvelopeId,
  subject: ExactSubject,
  configuration: FactoryConfigurationRef,
  work_digest: FactoryDigest,
  admission_risk: RiskClass,
  task: FactoryArtifactReference,
  acceptance: FactoryArtifactReference,
  project_goals: FactoryArtifactReference,
  duplicate_candidates: Vec<DuplicateCandidate>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EligibilityInputWire {
  work_id: WorkEnvelopeId,
  subject: ExactSubject,
  configuration: FactoryConfigurationRef,
  work_digest: FactoryDigest,
  admission_risk: RiskClass,
  task: FactoryArtifactReference,
  acceptance: FactoryArtifactReference,
  project_goals: FactoryArtifactReference,
  duplicate_candidates: Vec<DuplicateCandidate>,
}

impl TryFrom<EligibilityInputWire> for EligibilityInput {
  type Error = FactoryError;

  fn try_from(mut wire: EligibilityInputWire) -> Result<Self, Self::Error> {
    canonicalize(&mut wire.duplicate_candidates, |candidate| candidate.work_id)?;
    if wire.configuration.project_id() != wire.subject.project_id()
      || wire.duplicate_candidates.iter().any(|candidate| {
        candidate.work_id == wire.work_id || candidate.subject.project_id() != wire.subject.project_id()
      })
    {
      return Err(FactoryError::InconsistentSubject);
    }
    if wire.task.artifact_id() == wire.acceptance.artifact_id() {
      return Err(invalid("triage task and acceptance"));
    }
    Ok(Self {
      work_id: wire.work_id,
      subject: wire.subject,
      configuration: wire.configuration,
      work_digest: wire.work_digest,
      admission_risk: wire.admission_risk,
      task: wire.task,
      acceptance: wire.acceptance,
      project_goals: wire.project_goals,
      duplicate_candidates: wire.duplicate_candidates,
    })
  }
}

impl EligibilityInput {
  /// Freezes authorized candidate discovery and Project goals beside admitted Work.
  pub fn new(
    work: &WorkEnvelope,
    project_goals: FactoryArtifactReference,
    duplicate_candidates: Vec<DuplicateCandidate>,
  ) -> Result<Self, FactoryError> {
    Self::try_from(EligibilityInputWire {
      work_id: work.id(),
      subject: work.subject().clone(),
      configuration: work.configuration().clone(),
      work_digest: digest("octacity.factory.triage-work.v1", work)?,
      admission_risk: work.risk(),
      task: work.artifacts().task().clone(),
      acceptance: work.artifacts().acceptance().clone(),
      project_goals,
      duplicate_candidates,
    })
  }

  /// Returns the exact admitted Work identity.
  #[must_use]
  pub const fn work_id(&self) -> WorkEnvelopeId {
    self.work_id
  }
  /// Returns the Project-owned immutable source subject.
  #[must_use]
  pub const fn subject(&self) -> &ExactSubject {
    &self.subject
  }
  /// Returns the pinned Factory Configuration.
  #[must_use]
  pub const fn configuration(&self) -> &FactoryConfigurationRef {
    &self.configuration
  }
  /// Returns the canonical identity of the complete admitted Work Envelope.
  #[must_use]
  pub const fn work_digest(&self) -> FactoryDigest {
    self.work_digest
  }
  /// Returns the frozen task Artifact.
  #[must_use]
  pub const fn task(&self) -> &FactoryArtifactReference {
    &self.task
  }
  /// Returns the frozen acceptance-criteria Artifact.
  #[must_use]
  pub const fn acceptance(&self) -> &FactoryArtifactReference {
    &self.acceptance
  }
  /// Returns the exact Project goals used for eligibility.
  #[must_use]
  pub const fn project_goals(&self) -> &FactoryArtifactReference {
    &self.project_goals
  }
  /// Returns the risk ceiling recorded at admission.
  #[must_use]
  pub const fn admission_risk(&self) -> RiskClass {
    self.admission_risk
  }
  /// Returns the canonical duplicate candidates.
  #[must_use]
  pub fn duplicate_candidates(&self) -> &[DuplicateCandidate] {
    &self.duplicate_candidates
  }
  /// Returns the content identity of the complete frozen eligibility input.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    digest("octacity.factory.eligibility-input.v1", self)
  }
}

/// Frozen second-phase inputs, created only after deterministic eligibility acceptance.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "ClassificationInputWire")]
pub struct ClassificationInput {
  eligibility_input: EligibilityInput,
  eligibility_result: EligibilityResult,
  policy_digest: FactoryDigest,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClassificationInputWire {
  eligibility_input: EligibilityInput,
  eligibility_result: EligibilityResult,
  policy_digest: FactoryDigest,
}

impl TryFrom<ClassificationInputWire> for ClassificationInput {
  type Error = FactoryError;
  fn try_from(wire: ClassificationInputWire) -> Result<Self, Self::Error> {
    wire.eligibility_result.validate_input(&wire.eligibility_input)?;
    if *wire.eligibility_result.project_fit.value() != ProjectFit::InScope
      || wire
        .eligibility_result
        .duplicates
        .iter()
        .any(|record| record.value.status != DuplicateStatus::Distinct)
    {
      return Err(invalid("terminal eligibility cannot enter classification"));
    }
    Ok(Self {
      eligibility_input: wire.eligibility_input,
      eligibility_result: wire.eligibility_result,
      policy_digest: wire.policy_digest,
    })
  }
}

impl ClassificationInput {
  pub(super) fn accepted(
    input: EligibilityInput,
    result: EligibilityResult,
    policy_digest: FactoryDigest,
  ) -> Result<Self, FactoryError> {
    Self::try_from(ClassificationInputWire {
      eligibility_input: input,
      eligibility_result: result,
      policy_digest,
    })
  }
  /// Returns the original exact Work, Project goals, and duplicate discovery.
  #[must_use]
  pub const fn eligibility_input(&self) -> &EligibilityInput {
    &self.eligibility_input
  }
  /// Returns the accepted first-phase observations and producer provenance.
  #[must_use]
  pub const fn eligibility_result(&self) -> &EligibilityResult {
    &self.eligibility_result
  }
  /// Returns the immutable deterministic eligibility policy digest.
  #[must_use]
  pub const fn policy_digest(&self) -> FactoryDigest {
    self.policy_digest
  }
  /// Returns the complete second-phase input identity, including eligibility provenance.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    digest("octacity.factory.classification-input.v1", self)
  }
}

/// One dependency observed during classification; readiness is selected separately by pool policy.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TriageDependency {
  /// Exact Work that must precede this Work.
  pub work_id: WorkEnvelopeId,
  /// Exact retained version of the dependency facts.
  pub work_digest: FactoryDigest,
}

/// Caller-owned bounded classification observations, each with retained evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TriageClassification {
  /// Defect or feature-request purpose.
  pub work_kind: TriageObservation<WorkKind>,
  /// Bounded Project component key.
  pub component: TriageObservation<FactoryKey>,
  /// Finite severity assessment.
  pub severity: TriageObservation<FindingSeverity>,
  /// Finite size assessment.
  pub size: TriageObservation<WorkSize>,
  /// Observed risk; deterministic policy also preserves admitted risk.
  pub risk: TriageObservation<RiskClass>,
  /// Exact dependency observations, canonicalized by Work identity.
  pub dependencies: Vec<TriageObservation<TriageDependency>>,
  /// Preliminary reproduction; feature requests must use `not_applicable`.
  pub reproducibility: TriageObservation<PreliminaryReproducibility>,
  /// Finite advisory recommendation, never a dispatch instruction.
  pub recommended_route: TriageObservation<TriageRoute>,
}

/// Immutable typed second-phase result with every observation and exact provenance retained.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "TriageResultWire")]
pub struct TriageResult {
  schema_version: u16,
  provenance: TriageProvenance,
  classification: TriageClassification,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TriageResultWire {
  schema_version: u16,
  provenance: TriageProvenance,
  classification: TriageClassification,
}

impl TryFrom<TriageResultWire> for TriageResult {
  type Error = FactoryError;
  fn try_from(mut wire: TriageResultWire) -> Result<Self, Self::Error> {
    if wire.schema_version != TRIAGE_CONTRACT_VERSION {
      return Err(invalid("triage schema version"));
    }
    canonicalize(&mut wire.classification.dependencies, |record| record.value.work_id)?;
    let is_feature = *wire.classification.work_kind.value() == WorkKind::FeatureRequest;
    if is_feature != (*wire.classification.reproducibility.value() == PreliminaryReproducibility::NotApplicable) {
      return Err(invalid("Work kind and preliminary reproducibility"));
    }
    Ok(Self {
      schema_version: wire.schema_version,
      provenance: wire.provenance,
      classification: wire.classification,
    })
  }
}

impl TriageResult {
  /// Validates complete observations against accepted first-phase input and Work identity.
  pub fn new(
    input: &ClassificationInput,
    provenance: TriageProvenance,
    classification: TriageClassification,
  ) -> Result<Self, FactoryError> {
    let result = Self::try_from(TriageResultWire {
      schema_version: TRIAGE_CONTRACT_VERSION,
      provenance,
      classification,
    })?;
    result.validate_input(input)?;
    Ok(result)
  }
  pub(super) fn validate_input(&self, input: &ClassificationInput) -> Result<(), FactoryError> {
    if self.provenance.input_digest != input.digest()?
      || self
        .classification
        .dependencies
        .iter()
        .any(|record| record.value.work_id == input.eligibility_input.work_id)
    {
      return Err(invalid("classification input binding"));
    }
    Ok(())
  }
  /// Returns all non-authoritative observations with their exact evidence.
  #[must_use]
  pub const fn classification(&self) -> &TriageClassification {
    &self.classification
  }
  /// Returns the selected producer, input, nested Flow, and Node Attempt provenance.
  #[must_use]
  pub const fn provenance(&self) -> &TriageProvenance {
    &self.provenance
  }
  /// Returns the exact immutable classification-result identity.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    digest("octacity.factory.triage-result.v1", self)
  }
}

/// Typed duplicate assessment; the candidate must be present in the frozen input.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DuplicateAssessment {
  /// Exact candidate assessed by this observation.
  pub candidate_id: WorkEnvelopeId,
  /// Observed relationship, consumed only by deterministic policy.
  pub status: DuplicateStatus,
}

/// Immutable observations from eligibility; it contains no dispatch instruction.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "EligibilityResultWire")]
pub struct EligibilityResult {
  schema_version: u16,
  provenance: TriageProvenance,
  project_fit: TriageObservation<ProjectFit>,
  duplicates: Vec<TriageObservation<DuplicateAssessment>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EligibilityResultWire {
  schema_version: u16,
  provenance: TriageProvenance,
  project_fit: TriageObservation<ProjectFit>,
  duplicates: Vec<TriageObservation<DuplicateAssessment>>,
}

impl TryFrom<EligibilityResultWire> for EligibilityResult {
  type Error = FactoryError;

  fn try_from(mut wire: EligibilityResultWire) -> Result<Self, Self::Error> {
    if wire.schema_version != TRIAGE_CONTRACT_VERSION {
      return Err(invalid("triage schema version"));
    }
    canonicalize(&mut wire.duplicates, |record| record.value.candidate_id)?;
    Ok(Self {
      schema_version: wire.schema_version,
      provenance: wire.provenance,
      project_fit: wire.project_fit,
      duplicates: wire.duplicates,
    })
  }
}

impl EligibilityResult {
  /// Validates complete observations against the exact frozen eligibility inputs.
  pub fn new(
    input: &EligibilityInput,
    provenance: TriageProvenance,
    project_fit: TriageObservation<ProjectFit>,
    duplicates: Vec<TriageObservation<DuplicateAssessment>>,
  ) -> Result<Self, FactoryError> {
    let result = Self::try_from(EligibilityResultWire {
      schema_version: TRIAGE_CONTRACT_VERSION,
      provenance,
      project_fit,
      duplicates,
    })?;
    result.validate_input(input)?;
    Ok(result)
  }

  pub(super) fn validate_input(&self, input: &EligibilityInput) -> Result<(), FactoryError> {
    if self.provenance.input_digest != input.digest()?
      || self
        .duplicates
        .iter()
        .map(|record| record.value.candidate_id)
        .ne(input.duplicate_candidates.iter().map(|candidate| candidate.work_id))
    {
      return Err(invalid("eligibility input binding"));
    }
    Ok(())
  }

  /// Returns exact phase producer and input provenance.
  #[must_use]
  pub const fn provenance(&self) -> &TriageProvenance {
    &self.provenance
  }
  /// Returns the Project-fit observation with its exact evidence.
  #[must_use]
  pub const fn project_fit(&self) -> &TriageObservation<ProjectFit> {
    &self.project_fit
  }
  /// Returns all duplicate observations in stable candidate order.
  #[must_use]
  pub fn duplicates(&self) -> &[TriageObservation<DuplicateAssessment>] {
    &self.duplicates
  }
  /// Returns the exact immutable eligibility-result identity.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    digest("octacity.factory.eligibility-result.v1", self)
  }
}
