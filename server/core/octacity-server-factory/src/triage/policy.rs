use serde::{Deserialize, Serialize};

use crate::{
  BudgetLimit, BudgetResource, BudgetUsage, DeterministicGateOutcome, ExactSubject, FactoryArtifactReference,
  FactoryConfigurationRef, FactoryDigest, FactoryError, FlowDefinitionRef, RiskClass, ValidatedFlowDefinitionClosure,
  WorkEnvelopeId,
};

use super::*;

/// Trusted evidence purpose; model recommendations cannot create accepted facts.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum TriageEvidenceFact {
  /// Acceptance of the observed fit against the frozen Project goals.
  ProjectFit(ProjectFit),
  /// Acceptance of one exact duplicate assessment.
  Duplicate(DuplicateAssessment),
  /// Deterministically accepted reproduction for the exact base.
  Reproduced,
  /// Existing behavior satisfies the frozen acceptance criteria.
  AlreadyFixed,
  /// An existing exact target may enter verification without implementation.
  VerificationOnly,
}

/// Trusted acceptance of exact evidence, constructed outside a reasoning result.
///
/// This type deliberately has no Deserialize implementation. The application
/// must reconstruct it from authoritative deterministic validation, not provider bytes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AcceptedTriageEvidence {
  subject: ExactSubject,
  input_digest: FactoryDigest,
  fact: TriageEvidenceFact,
  pub(super) evidence: FactoryArtifactReference,
}

impl AcceptedTriageEvidence {
  /// Accepts retained evidence only after a trusted gate passes for the exact inputs.
  pub fn new(
    subject: ExactSubject,
    input_digest: FactoryDigest,
    fact: TriageEvidenceFact,
    evidence: FactoryArtifactReference,
    outcome: DeterministicGateOutcome,
  ) -> Result<Self, FactoryError> {
    if outcome != DeterministicGateOutcome::Passed {
      return Err(invalid("unaccepted triage evidence"));
    }
    Ok(Self {
      subject,
      input_digest,
      fact,
      evidence,
    })
  }
}

/// Immutable deterministic triage settings, narrowed by the pinned Flow and configuration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TriagePolicySettings {
  /// Maximum risk allowed to proceed without human disposition.
  pub max_risk: RiskClass,
  /// Policy-approved small-Work entry, or `None` to require authored requirements.
  pub small_work_route: Option<TriageRoute>,
  /// Hard budget enclosing both triage phases and subsequent dispatch.
  pub budget: BudgetLimit,
}

/// Pure policy pinned to exact configuration, root, and replaceable phase definitions.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TriagePolicy {
  configuration: FactoryConfigurationRef,
  root: FlowDefinitionRef,
  eligibility_flow: FlowDefinitionRef,
  classification_flow: FlowDefinitionRef,
  // These sets are derived from the content-addressed phase roots. They are
  // implementation caches, not new policy inputs or a new serialized identity.
  #[serde(skip)]
  eligibility_producers: Vec<FlowDefinitionRef>,
  #[serde(skip)]
  classification_producers: Vec<FlowDefinitionRef>,
  settings: TriagePolicySettings,
  declared_routes: Vec<TriageRoute>,
}

/// First-phase disposition selected solely by deterministic policy.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EligibilityOutcome {
  /// Execute classification with the accepted first-phase provenance.
  Classify,
  /// Resolve the Work without dispatching classification.
  Rejection,
  /// Ask for human disposition without dispatching classification.
  Escalation,
}

impl EligibilityOutcome {
  /// Returns the finite outcome key declared by the deterministic eligibility gate.
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Classify => "classify",
      Self::Rejection => "rejection",
      Self::Escalation => "escalation",
    }
  }
}

/// Stable reason recorded alongside a deterministic triage disposition.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TriageReason {
  /// Eligibility passed with accepted observations.
  Eligible,
  /// A duplicate was accepted for the frozen candidate.
  Duplicate,
  /// The Work does not fit the frozen Project goals.
  OutOfScope,
  /// Observations or required evidence remain inconclusive.
  Inconclusive,
  /// The enclosing hard budget is exhausted.
  BudgetExhausted,
  /// Observed or admitted risk exceeds the pinned ceiling.
  RiskExceeded,
  /// Defect reproduction needs bounded research.
  ReproductionRequired,
  /// Large Work or policy requires authored requirements.
  RequirementsRequired,
  /// Small Work satisfies the configured bypass bounds.
  SmallWork,
  /// Trusted evidence satisfies a terminal or verification-only resolution.
  AcceptedEvidence,
  /// The recommendation is permitted by the deterministic policy.
  PermittedRecommendation,
  /// The required route is absent from the pinned definition.
  UndeclaredRoute,
}

/// Auditable first-phase decision, never a model-supplied authority record.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EligibilityDecision {
  /// Finite policy-selected outcome.
  pub outcome: EligibilityOutcome,
  /// Deterministic reason.
  pub reason: TriageReason,
  /// Exact duplicate resolved, when applicable.
  pub duplicate_id: Option<WorkEnvelopeId>,
  /// Complete immutable phase input.
  pub input_digest: FactoryDigest,
  /// Complete immutable observations.
  pub result_digest: FactoryDigest,
  /// Exact configuration and Flow-bound policy.
  pub policy_digest: FactoryDigest,
  /// Canonical identity of trusted evidence consumed by eligibility.
  pub evidence_digest: FactoryDigest,
}

/// Auditable routing disposition; committing and dispatch remain application responsibilities.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TriageDecision {
  /// Finite declared outcome selected by policy.
  pub route: TriageRoute,
  /// Deterministic reason.
  pub reason: TriageReason,
  /// Complete classification input, including accepted eligibility provenance.
  pub input_digest: FactoryDigest,
  /// Complete classification observations.
  pub result_digest: FactoryDigest,
  /// Exact configuration and Flow-bound policy.
  pub policy_digest: FactoryDigest,
  /// Exact trusted facts consumed by this decision.
  pub evidence_digest: FactoryDigest,
}

/// Common typed terminal payload exported by triage, with an explicit originating phase.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "phase", content = "decision")]
pub enum TriageDisposition {
  /// Eligibility terminated before classification.
  Eligibility(EligibilityDecision),
  /// Classification reached a policy-selected route.
  Classification(TriageDecision),
}

impl TriagePolicy {
  /// Binds immutable routing settings to a fully validated two-phase composition.
  pub fn new(
    configuration: FactoryConfigurationRef,
    closure: &ValidatedFlowDefinitionClosure,
    settings: TriagePolicySettings,
  ) -> Result<Self, FactoryError> {
    Self::for_flow(configuration, closure, closure.closure().root(), settings)
  }

  /// Binds triage policy to an exact nested triage definition inside a larger admitted closure.
  pub fn for_flow(
    configuration: FactoryConfigurationRef,
    closure: &ValidatedFlowDefinitionClosure,
    triage_definition: FlowDefinitionRef,
    settings: TriagePolicySettings,
  ) -> Result<Self, FactoryError> {
    let root = closure
      .closure()
      .definition(triage_definition)
      .ok_or_else(|| invalid("triage root"))?;
    super::composition::validate_composition(root)?;
    if settings
      .small_work_route
      .is_some_and(|route| !matches!(route, TriageRoute::ProtectedTest | TriageRoute::Development))
      || !settings.budget.fits_within(closure.limits().budget())
      || !root.execution().budget().fits_within(settings.budget)
    {
      return Err(invalid("triage policy bounds"));
    }
    let phase_root = |role: TriageNode| {
      root
        .node(&role.key())
        .and_then(|node| node.subflow_definition())
        .ok_or_else(|| invalid("triage phase"))
    };
    let phase = |phase: FlowDefinitionRef| {
      let mut selected = std::collections::BTreeSet::from([phase]);
      let mut pending = vec![phase];
      while let Some(reference) = pending.pop() {
        let definition = closure
          .closure()
          .definition(reference)
          .ok_or_else(|| invalid("triage phase closure"))?;
        for child in definition
          .nodes()
          .iter()
          .filter_map(crate::FlowNodeDefinition::subflow_definition)
        {
          if selected.insert(child) {
            pending.push(child);
          }
        }
      }
      Ok::<_, FactoryError>(selected.into_iter().collect())
    };
    let declared_routes = TriageRoute::ALL
      .into_iter()
      .filter(|route| {
        root
          .terminals()
          .iter()
          .any(|terminal| terminal.key().as_str() == route.as_str())
      })
      .collect();
    Ok(Self {
      configuration,
      root: root.reference(),
      eligibility_flow: phase_root(TriageNode::Eligibility)?,
      classification_flow: phase_root(TriageNode::Classification)?,
      eligibility_producers: phase(phase_root(TriageNode::Eligibility)?)?,
      classification_producers: phase(phase_root(TriageNode::Classification)?)?,
      settings,
      declared_routes,
    })
  }

  /// Returns the exact selected policy identity, including configuration and nested versions.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    super::digest("octacity.factory.triage-policy.v1", self)
  }

  /// Selects rejection, escalation, or classification from accepted eligibility evidence.
  pub fn eligibility(
    &self,
    input: &EligibilityInput,
    result: &EligibilityResult,
    evidence: &[AcceptedTriageEvidence],
    usage: BudgetUsage,
  ) -> Result<EligibilityDecision, FactoryError> {
    result.validate_input(input)?;
    if input.configuration() != &self.configuration
      || !self.eligibility_producers.contains(&result.provenance().definition)
    {
      return Err(invalid("eligibility pinned configuration or Flow"));
    }
    self.validate_evidence(input.subject(), input.digest()?, evidence)?;
    let mut decision = EligibilityDecision {
      outcome: EligibilityOutcome::Classify,
      reason: TriageReason::Eligible,
      duplicate_id: None,
      input_digest: input.digest()?,
      result_digest: result.digest()?,
      policy_digest: self.digest()?,
      evidence_digest: evidence_digest(evidence)?,
    };
    let fit = *result.project_fit().value();
    if self.exhausted(usage) {
      decision.outcome = EligibilityOutcome::Escalation;
      decision.reason = TriageReason::BudgetExhausted;
    } else if fit == ProjectFit::Inconclusive
      || !accepted(
        evidence,
        &TriageEvidenceFact::ProjectFit(fit),
        result.project_fit().evidence(),
      )
    {
      decision.outcome = EligibilityOutcome::Escalation;
      decision.reason = TriageReason::Inconclusive;
    } else if fit == ProjectFit::OutOfScope {
      decision.outcome = EligibilityOutcome::Rejection;
      decision.reason = TriageReason::OutOfScope;
    } else {
      for duplicate in result.duplicates() {
        if duplicate.value().status == DuplicateStatus::Inconclusive
          || !accepted(
            evidence,
            &TriageEvidenceFact::Duplicate(duplicate.value().clone()),
            duplicate.evidence(),
          )
        {
          decision.outcome = EligibilityOutcome::Escalation;
          decision.reason = TriageReason::Inconclusive;
        } else if duplicate.value().status == DuplicateStatus::Duplicate {
          decision.outcome = EligibilityOutcome::Rejection;
          decision.reason = TriageReason::Duplicate;
          decision.duplicate_id = Some(duplicate.value().candidate_id);
          break;
        }
      }
    }
    Ok(decision)
  }

  /// Freezes classification input only when this exact policy selects classification.
  pub fn classification_input(
    &self,
    input: EligibilityInput,
    result: EligibilityResult,
    evidence: &[AcceptedTriageEvidence],
    usage: BudgetUsage,
  ) -> Result<ClassificationInput, FactoryError> {
    if self.eligibility(&input, &result, evidence, usage)?.outcome != EligibilityOutcome::Classify {
      return Err(invalid("terminal eligibility cannot dispatch classification"));
    }
    ClassificationInput::accepted(input, result, self.digest()?)
  }

  /// Selects only a finite declared route, with hard evidence/risk/budget rules taking precedence.
  pub fn route(
    &self,
    input: &ClassificationInput,
    result: &TriageResult,
    eligibility_evidence: &[AcceptedTriageEvidence],
    evidence: &[AcceptedTriageEvidence],
    usage: BudgetUsage,
  ) -> Result<TriageDecision, FactoryError> {
    result.validate_input(input)?;
    if input.policy_digest() != self.digest()?
      || input.eligibility_input().configuration() != &self.configuration
      || !self
        .eligibility_producers
        .contains(&input.eligibility_result().provenance().definition)
      || !self.classification_producers.contains(&result.provenance().definition)
    {
      return Err(invalid("classification pinned policy or Flow"));
    }
    // A deserialized second-phase input does not self-attest eligibility authority.
    // Recheck its original observations against trusted first-phase validation.
    let eligibility = self.eligibility(
      input.eligibility_input(),
      input.eligibility_result(),
      eligibility_evidence,
      BudgetUsage::default(),
    )?;
    if eligibility.outcome != EligibilityOutcome::Classify {
      return Err(invalid("unaccepted classification eligibility"));
    }
    self.validate_evidence(input.eligibility_input().subject(), input.digest()?, evidence)?;
    let (route, reason) = self.select_route(input, result, evidence, usage);
    let (route, reason) = if self.declared_routes.contains(&route) {
      (route, reason)
    } else {
      (TriageRoute::Escalation, TriageReason::UndeclaredRoute)
    };
    Ok(TriageDecision {
      route,
      reason,
      input_digest: input.digest()?,
      result_digest: result.digest()?,
      policy_digest: self.digest()?,
      evidence_digest: super::digest(
        "octacity.factory.triage-routing-evidence.v1",
        &(eligibility.evidence_digest, evidence_digest(evidence)?),
      )?,
    })
  }

  fn select_route(
    &self,
    input: &ClassificationInput,
    result: &TriageResult,
    evidence: &[AcceptedTriageEvidence],
    usage: BudgetUsage,
  ) -> (TriageRoute, TriageReason) {
    let classification = result.classification();
    let recommended = *classification.recommended_route.value();
    if self.exhausted(usage) {
      return (TriageRoute::Escalation, TriageReason::BudgetExhausted);
    }
    if (*classification.risk.value()).max(input.eligibility_input().admission_risk()) > self.settings.max_risk {
      return (TriageRoute::Escalation, TriageReason::RiskExceeded);
    }
    if !self.declared_routes.contains(&recommended) {
      return (TriageRoute::Escalation, TriageReason::UndeclaredRoute);
    }
    if matches!(recommended, TriageRoute::Rejection | TriageRoute::Escalation) {
      return (recommended, TriageReason::PermittedRecommendation);
    }
    if matches!(recommended, TriageRoute::AlreadyFixed | TriageRoute::VerificationOnly) {
      let fact = if recommended == TriageRoute::AlreadyFixed {
        TriageEvidenceFact::AlreadyFixed
      } else {
        TriageEvidenceFact::VerificationOnly
      };
      return if accepted(evidence, &fact, classification.recommended_route.evidence()) {
        (recommended, TriageReason::AcceptedEvidence)
      } else {
        (TriageRoute::Escalation, TriageReason::Inconclusive)
      };
    }
    if *classification.work_kind.value() == WorkKind::Defect {
      match classification.reproducibility.value() {
        PreliminaryReproducibility::NeedsHumanInput => return (TriageRoute::Escalation, TriageReason::Inconclusive),
        PreliminaryReproducibility::Reproduced
          if accepted(
            evidence,
            &TriageEvidenceFact::Reproduced,
            classification.reproducibility.evidence(),
          ) => {}
        _ => return (TriageRoute::Research, TriageReason::ReproductionRequired),
      }
    }
    if recommended == TriageRoute::Research {
      return (recommended, TriageReason::PermittedRecommendation);
    }
    if *classification.size.value() != WorkSize::Small
      || self.settings.small_work_route.is_none()
      || recommended == TriageRoute::Requirements
    {
      return (TriageRoute::Requirements, TriageReason::RequirementsRequired);
    }
    (
      self.settings.small_work_route.unwrap_or(TriageRoute::Requirements),
      TriageReason::SmallWork,
    )
  }

  fn exhausted(&self, usage: BudgetUsage) -> bool {
    [
      BudgetResource::Attempts,
      BudgetResource::ElapsedTime,
      BudgetResource::Tokens,
      BudgetResource::Cost,
      BudgetResource::OutputBytes,
    ]
    .into_iter()
    .any(|resource| usage.is_exhausted(self.settings.budget, resource))
  }
  fn validate_evidence(
    &self,
    subject: &ExactSubject,
    input_digest: FactoryDigest,
    evidence: &[AcceptedTriageEvidence],
  ) -> Result<(), FactoryError> {
    if evidence.len() > MAX_TRIAGE_OBSERVATIONS + 1
      || evidence
        .iter()
        .any(|record| record.subject != *subject || record.input_digest != input_digest)
    {
      return Err(invalid("triage accepted evidence binding"));
    }
    Ok(())
  }
}

fn accepted(
  evidence: &[AcceptedTriageEvidence],
  fact: &TriageEvidenceFact,
  artifact: &FactoryArtifactReference,
) -> bool {
  evidence
    .iter()
    .any(|record| &record.fact == fact && &record.evidence == artifact)
}

fn evidence_digest(evidence: &[AcceptedTriageEvidence]) -> Result<FactoryDigest, FactoryError> {
  let mut records = evidence
    .iter()
    .map(serde_json::to_vec)
    .collect::<Result<Vec<_>, _>>()
    .map_err(|_| invalid("triage evidence serialization"))?;
  records.sort();
  records.dedup();
  super::digest("octacity.factory.triage-accepted-evidence.v1", &records)
}

impl TriageDisposition {
  /// Canonical typed successor input, retaining the complete deterministic decision.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    super::digest("octacity.factory.triage-disposition.v1", self)
  }
}
