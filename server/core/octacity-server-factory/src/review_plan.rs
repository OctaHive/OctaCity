//! Immutable review planning over exact evidence, criteria, capabilities, and bounds.

use std::collections::HashSet;

use octacity_server_domain::{ProjectId, Timestamp};
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use crate::{
  BudgetLimit, CandidateSubject, EvaluationPlanId, EvaluationPolicy, EvidenceManifest, EvidenceManifestId,
  FactoryArtifactReference, FactoryDigest, FactoryError, FactoryKey, ImmutableReference,
};

/// Maximum criterion packs selected by one Evaluation Plan.
pub const MAX_CRITERION_PACKS: usize = 32;
/// Maximum evaluator branches selected by one Evaluation Plan.
pub const MAX_EVALUATORS: usize = 32;

/// Factory phase whose candidate is being reviewed.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewPurpose {
  /// Review of requirements or a specification candidate.
  Requirements,
  /// Review of an implementation candidate.
  Implementation,
  /// Review of source changes for maintainability and correctness.
  CodeReview,
  /// Independent verification of a candidate.
  Verification,
  /// Review of a deployment or promotion candidate.
  Deployment,
}

/// Project-owned immutable criteria consumed by evaluators.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CriterionPack {
  project_id: ProjectId,
  reference: ImmutableReference,
  schema: ImmutableReference,
  artifact: FactoryArtifactReference,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CriterionPackWire {
  project_id: ProjectId,
  reference: ImmutableReference,
  schema: ImmutableReference,
  artifact: FactoryArtifactReference,
}

impl<'de> Deserialize<'de> for CriterionPack {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let wire = CriterionPackWire::deserialize(deserializer)?;
    Self::new(wire.project_id, wire.reference, wire.schema, wire.artifact).map_err(D::Error::custom)
  }
}

impl CriterionPack {
  /// Constructs a criterion pack whose immutable identity matches its retained bytes.
  pub fn new(
    project_id: ProjectId,
    reference: ImmutableReference,
    schema: ImmutableReference,
    artifact: FactoryArtifactReference,
  ) -> Result<Self, FactoryError> {
    if reference.digest() != artifact.content_digest() {
      return Err(FactoryError::InvalidReference {
        relationship: "criterion pack artifact digest",
      });
    }
    Ok(Self {
      project_id,
      reference,
      schema,
      artifact,
    })
  }

  /// Returns the owning Project.
  #[must_use]
  pub const fn project_id(&self) -> ProjectId {
    self.project_id
  }

  /// Returns the exact versioned criterion-pack identity.
  #[must_use]
  pub const fn reference(&self) -> &ImmutableReference {
    &self.reference
  }

  /// Returns the exact schema used to validate the pack.
  #[must_use]
  pub const fn schema(&self) -> &ImmutableReference {
    &self.schema
  }

  /// Returns the retained criterion-pack artifact.
  #[must_use]
  pub const fn artifact(&self) -> &FactoryArtifactReference {
    &self.artifact
  }
}

/// One independently evaluated branch in a review plan.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewBranch {
  key: FactoryKey,
  evaluator: ImmutableReference,
  required: bool,
}

impl ReviewBranch {
  /// Constructs a branch bound to one exact evaluator capability.
  #[must_use]
  pub const fn new(key: FactoryKey, evaluator: ImmutableReference, required: bool) -> Self {
    Self {
      key,
      evaluator,
      required,
    }
  }

  /// Returns the stable branch key.
  #[must_use]
  pub const fn key(&self) -> &FactoryKey {
    &self.key
  }

  /// Returns the exact evaluator identity.
  #[must_use]
  pub const fn evaluator(&self) -> &ImmutableReference {
    &self.evaluator
  }

  /// Reports whether the branch must be available and complete.
  #[must_use]
  pub const fn is_required(&self) -> bool {
    self.required
  }
}

/// Advertised bounds of one exact evaluator connector.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReviewEvaluatorCapability {
  evaluator: ImmutableReference,
  purposes: Vec<ReviewPurpose>,
  data_handling: Vec<ImmutableReference>,
  budget_ceiling: BudgetLimit,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewEvaluatorCapabilityWire {
  evaluator: ImmutableReference,
  purposes: Vec<ReviewPurpose>,
  data_handling: Vec<ImmutableReference>,
  budget_ceiling: BudgetLimit,
}

impl<'de> Deserialize<'de> for ReviewEvaluatorCapability {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let wire = ReviewEvaluatorCapabilityWire::deserialize(deserializer)?;
    Self::try_new(wire.evaluator, wire.purposes, wire.data_handling, wire.budget_ceiling).map_err(D::Error::custom)
  }
}

impl ReviewEvaluatorCapability {
  /// Constructs a canonical non-empty evaluator capability declaration.
  pub fn try_new(
    evaluator: ImmutableReference,
    mut purposes: Vec<ReviewPurpose>,
    mut data_handling: Vec<ImmutableReference>,
    budget_ceiling: BudgetLimit,
  ) -> Result<Self, FactoryError> {
    canonicalize_unique(&mut purposes, "evaluator purposes")?;
    canonicalize_unique(&mut data_handling, "evaluator data handling")?;
    Ok(Self {
      evaluator,
      purposes,
      data_handling,
      budget_ceiling,
    })
  }

  /// Returns the exact evaluator identity.
  #[must_use]
  pub const fn evaluator(&self) -> &ImmutableReference {
    &self.evaluator
  }

  fn supports(&self, purpose: ReviewPurpose, data_handling: &ImmutableReference, budget: BudgetLimit) -> bool {
    self.purposes.contains(&purpose)
      && self.data_handling.contains(data_handling)
      && budget.fits_within(self.budget_ceiling)
  }
}

/// Operator request used to freeze one immutable Evaluation Plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationPlanDefinition {
  /// New Evaluation Plan identity.
  pub id: EvaluationPlanId,
  /// Factory phase being reviewed.
  pub purpose: ReviewPurpose,
  /// Exact candidate subject.
  pub subject: CandidateSubject,
  /// Exact Project-owned criterion packs.
  pub criterion_packs: Vec<CriterionPack>,
  /// Independent evaluator branches.
  pub branches: Vec<ReviewBranch>,
  /// Hard aggregate review budget.
  pub budget: BudgetLimit,
  /// Time at which the immutable plan was frozen.
  pub created_at: Timestamp,
  /// Absolute review deadline.
  pub deadline: Timestamp,
  /// Exact data-handling policy applied before any protected input is exposed.
  pub data_handling: ImmutableReference,
}

/// Immutable selection of candidate, evidence, criteria, evaluator branches, and bounds.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EvaluationPlan {
  id: EvaluationPlanId,
  evidence_id: EvidenceManifestId,
  purpose: ReviewPurpose,
  subject: CandidateSubject,
  criterion_packs: Vec<CriterionPack>,
  branches: Vec<ReviewBranch>,
  required_quorum: u16,
  budget: BudgetLimit,
  created_at: Timestamp,
  deadline: Timestamp,
  data_handling: ImmutableReference,
}

impl EvaluationPlan {
  fn from_parts(wire: EvaluationPlanWire) -> Result<Self, FactoryError> {
    validate_plan_shape(&wire)?;
    let mut criterion_packs = wire.criterion_packs;
    let mut branches = wire.branches;
    criterion_packs.sort_by(|left, right| left.reference().cmp(right.reference()));
    branches.sort_by(|left, right| left.key().cmp(right.key()));
    Ok(Self {
      id: wire.id,
      evidence_id: wire.evidence_id,
      purpose: wire.purpose,
      subject: wire.subject,
      criterion_packs,
      branches,
      required_quorum: wire.required_quorum,
      budget: wire.budget,
      created_at: wire.created_at,
      deadline: wire.deadline,
      data_handling: wire.data_handling,
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

  /// Returns the reviewed Factory phase.
  #[must_use]
  pub const fn purpose(&self) -> ReviewPurpose {
    self.purpose
  }

  /// Returns the exact candidate subject.
  #[must_use]
  pub const fn subject(&self) -> &CandidateSubject {
    &self.subject
  }

  /// Returns the canonical immutable criterion packs.
  #[must_use]
  pub fn criterion_packs(&self) -> &[CriterionPack] {
    &self.criterion_packs
  }

  /// Returns the canonical evaluator branches.
  #[must_use]
  pub fn branches(&self) -> &[ReviewBranch] {
    &self.branches
  }

  /// Returns the number of successful branches required by deterministic policy.
  #[must_use]
  pub const fn required_quorum(&self) -> u16 {
    self.required_quorum
  }

  /// Returns the immutable aggregate budget.
  #[must_use]
  pub const fn budget(&self) -> BudgetLimit {
    self.budget
  }

  /// Returns the plan creation time.
  #[must_use]
  pub const fn created_at(&self) -> Timestamp {
    self.created_at
  }

  /// Returns the absolute plan deadline.
  #[must_use]
  pub const fn deadline(&self) -> Timestamp {
    self.deadline
  }

  /// Returns the exact data-handling policy.
  #[must_use]
  pub const fn data_handling(&self) -> &ImmutableReference {
    &self.data_handling
  }

  /// Reports whether an exact evaluator is selected by the plan.
  #[must_use]
  pub fn has_evaluator(&self, evaluator: &ImmutableReference) -> bool {
    self.branches.iter().any(|branch| branch.evaluator() == evaluator)
  }

  /// Reports whether an evaluator logical identity is selected by the plan.
  #[must_use]
  pub fn has_evaluator_identity(&self, evaluator: &FactoryKey) -> bool {
    self
      .branches
      .iter()
      .any(|branch| branch.evaluator().identity() == evaluator)
  }

  /// Returns the bounded number of selected evaluator branches.
  #[must_use]
  pub fn evaluator_count(&self) -> usize {
    self.branches.len()
  }

  /// Computes the stable content digest of the complete frozen plan.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    let encoded = serde_json::to_vec(self).map_err(|_| FactoryError::InvalidReference {
      relationship: "evaluation plan serialization",
    })?;
    Ok(FactoryDigest::sha256(
      "octacity.factory.evaluation-plan.v2",
      &[&encoded],
    ))
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EvaluationPlanWire {
  id: EvaluationPlanId,
  evidence_id: EvidenceManifestId,
  purpose: ReviewPurpose,
  subject: CandidateSubject,
  criterion_packs: Vec<CriterionPack>,
  branches: Vec<ReviewBranch>,
  required_quorum: u16,
  budget: BudgetLimit,
  created_at: Timestamp,
  deadline: Timestamp,
  data_handling: ImmutableReference,
}

impl<'de> Deserialize<'de> for EvaluationPlan {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    Self::from_parts(EvaluationPlanWire::deserialize(deserializer)?).map_err(D::Error::custom)
  }
}

/// Safe result of preparing a review before protected inputs are dispatched.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReviewPlanPreparation {
  /// Every required capability is present and the immutable plan may be persisted.
  Ready(Box<EvaluationPlan>),
  /// Required capability is unavailable; execution must escalate without exposing inputs.
  Escalate(ReviewPlanEscalation),
}

/// Typed fail-closed reason preventing review dispatch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReviewPlanEscalation {
  /// A required evaluator branch cannot satisfy the frozen plan.
  RequiredCapabilityUnavailable {
    /// Stable branch key safe to expose in diagnostics.
    branch: FactoryKey,
  },
  /// Available evaluator capabilities cannot satisfy the configured quorum.
  QuorumCapabilityUnavailable {
    /// Required evaluator count.
    required: u16,
    /// Compatible evaluator count.
    available: u16,
  },
}

/// Validates metadata-only capability declarations and freezes an immutable Evaluation Plan.
///
/// The function accepts references and capability metadata, never protected artifact bytes. A
/// required-capability failure therefore becomes an escalation before any evaluator dispatch.
pub fn prepare_evaluation_plan(
  mut definition: EvaluationPlanDefinition,
  evidence: &EvidenceManifest,
  policy: &EvaluationPolicy,
  capabilities: &[ReviewEvaluatorCapability],
) -> Result<ReviewPlanPreparation, FactoryError> {
  validate_definition(&definition, evidence, policy)?;
  validate_unique_by(
    capabilities.iter().map(|capability| capability.evaluator().identity()),
    "review capability identities",
  )?;
  definition
    .criterion_packs
    .sort_by(|left, right| left.reference().cmp(right.reference()));
  definition.branches.sort_by(|left, right| left.key().cmp(right.key()));

  let compatible = definition
    .branches
    .iter()
    .filter(|branch| {
      capabilities.iter().any(|capability| {
        capability.evaluator() == branch.evaluator()
          && capability.supports(definition.purpose, &definition.data_handling, definition.budget)
      })
    })
    .map(ReviewBranch::key)
    .collect::<HashSet<_>>();
  if let Some(branch) = definition
    .branches
    .iter()
    .find(|branch| branch.is_required() && !compatible.contains(branch.key()))
  {
    return Ok(ReviewPlanPreparation::Escalate(
      ReviewPlanEscalation::RequiredCapabilityUnavailable {
        branch: branch.key().clone(),
      },
    ));
  }
  let available = u16::try_from(compatible.len()).map_err(|_| FactoryError::CollectionLimitExceeded {
    collection: "review capabilities",
  })?;
  if available < policy.required_quorum() {
    return Ok(ReviewPlanPreparation::Escalate(
      ReviewPlanEscalation::QuorumCapabilityUnavailable {
        required: policy.required_quorum(),
        available,
      },
    ));
  }

  Ok(ReviewPlanPreparation::Ready(Box::new(EvaluationPlan::from_parts(
    EvaluationPlanWire {
      id: definition.id,
      evidence_id: evidence.id(),
      purpose: definition.purpose,
      subject: definition.subject,
      criterion_packs: definition.criterion_packs,
      branches: definition.branches,
      required_quorum: policy.required_quorum(),
      budget: definition.budget,
      created_at: definition.created_at,
      deadline: definition.deadline,
      data_handling: definition.data_handling,
    },
  )?)))
}

fn validate_plan_shape(plan: &EvaluationPlanWire) -> Result<(), FactoryError> {
  if plan
    .criterion_packs
    .iter()
    .any(|pack| pack.project_id() != plan.subject.exact().project_id())
  {
    return Err(FactoryError::InconsistentSubject);
  }
  let elapsed = plan
    .deadline
    .unix_millis()
    .checked_sub(plan.created_at.unix_millis())
    .and_then(|value| u64::try_from(value).ok());
  if elapsed.is_none_or(|value| value == 0 || value > plan.budget.max_elapsed_millis()) {
    return Err(FactoryError::InvalidReference {
      relationship: "evaluation deadline",
    });
  }
  validate_count(plan.criterion_packs.len(), MAX_CRITERION_PACKS, "criterion packs")?;
  validate_count(plan.branches.len(), MAX_EVALUATORS, "review branches")?;
  let required = u16::try_from(plan.branches.iter().filter(|branch| branch.is_required()).count()).map_err(|_| {
    FactoryError::CollectionLimitExceeded {
      collection: "review branches",
    }
  })?;
  let branch_count = u16::try_from(plan.branches.len()).map_err(|_| FactoryError::CollectionLimitExceeded {
    collection: "review branches",
  })?;
  if plan.required_quorum == 0 || plan.required_quorum > branch_count || required > plan.required_quorum {
    return Err(FactoryError::InvalidReference {
      relationship: "evaluation quorum",
    });
  }
  validate_unique_by(
    plan.criterion_packs.iter().map(CriterionPack::reference),
    "criterion packs",
  )?;
  validate_unique_by(
    plan.criterion_packs.iter().map(|pack| pack.reference().identity()),
    "criterion pack identities",
  )?;
  validate_unique_by(plan.branches.iter().map(ReviewBranch::key), "review branches")?;
  validate_unique_by(plan.branches.iter().map(ReviewBranch::evaluator), "review evaluators")?;
  validate_unique_by(
    plan.branches.iter().map(|branch| branch.evaluator().identity()),
    "review evaluator identities",
  )
}

fn validate_definition(
  definition: &EvaluationPlanDefinition,
  evidence: &EvidenceManifest,
  policy: &EvaluationPolicy,
) -> Result<(), FactoryError> {
  if &definition.subject != evidence.subject()
    || definition
      .criterion_packs
      .iter()
      .any(|pack| pack.project_id() != definition.subject.exact().project_id())
  {
    return Err(FactoryError::InconsistentSubject);
  }
  if definition.budget != policy.budget() {
    return Err(FactoryError::InvalidReference {
      relationship: "evaluation budget",
    });
  }
  let elapsed = definition
    .deadline
    .unix_millis()
    .checked_sub(definition.created_at.unix_millis());
  if elapsed
    .and_then(|value| u64::try_from(value).ok())
    .is_none_or(|value| value == 0 || value > definition.budget.max_elapsed_millis())
  {
    return Err(FactoryError::InvalidReference {
      relationship: "evaluation deadline",
    });
  }
  validate_count(definition.criterion_packs.len(), MAX_CRITERION_PACKS, "criterion packs")?;
  validate_count(definition.branches.len(), MAX_EVALUATORS, "review branches")?;
  validate_unique_by(
    definition.criterion_packs.iter().map(CriterionPack::reference),
    "criterion packs",
  )?;
  validate_unique_by(
    definition
      .criterion_packs
      .iter()
      .map(|pack| pack.reference().identity()),
    "criterion pack identities",
  )?;
  validate_unique_by(definition.branches.iter().map(ReviewBranch::key), "review branches")?;
  validate_unique_by(
    definition.branches.iter().map(ReviewBranch::evaluator),
    "review evaluators",
  )?;
  validate_unique_by(
    definition.branches.iter().map(|branch| branch.evaluator().identity()),
    "review evaluator identities",
  )?;

  let mut selected_packs = definition
    .criterion_packs
    .iter()
    .map(CriterionPack::reference)
    .collect::<Vec<_>>();
  selected_packs.sort();
  let mut configured_packs = policy.criterion_packs().iter().collect::<Vec<_>>();
  configured_packs.sort();
  let mut selected_evaluators = definition
    .branches
    .iter()
    .map(ReviewBranch::evaluator)
    .collect::<Vec<_>>();
  selected_evaluators.sort();
  let mut configured_evaluators = policy.evaluators().iter().collect::<Vec<_>>();
  configured_evaluators.sort();
  if selected_packs != configured_packs || selected_evaluators != configured_evaluators {
    return Err(FactoryError::InvalidReference {
      relationship: "evaluation policy selection",
    });
  }
  Ok(())
}

fn canonicalize_unique<T: Ord>(values: &mut [T], collection: &'static str) -> Result<(), FactoryError> {
  if values.is_empty() {
    return Err(FactoryError::InvalidReference {
      relationship: collection,
    });
  }
  values.sort();
  if values.windows(2).any(|pair| pair[0] == pair[1]) {
    return Err(FactoryError::InvalidReference {
      relationship: collection,
    });
  }
  Ok(())
}

fn validate_count(count: usize, maximum: usize, collection: &'static str) -> Result<(), FactoryError> {
  if count == 0 {
    return Err(FactoryError::InvalidReference {
      relationship: collection,
    });
  }
  if count > maximum {
    return Err(FactoryError::CollectionLimitExceeded { collection });
  }
  Ok(())
}

fn validate_unique_by<'a, T: Eq + std::hash::Hash + ?Sized + 'a>(
  mut values: impl Iterator<Item = &'a T>,
  relationship: &'static str,
) -> Result<(), FactoryError> {
  let mut seen = HashSet::new();
  if values.any(|value| !seen.insert(value)) {
    return Err(FactoryError::InvalidReference { relationship });
  }
  Ok(())
}
