use super::{
  AcceptedResearchEvidence, DefectResearchOutcome, MAX_RESEARCH_ATTEMPTS, MAX_RESEARCH_PROPOSAL_BYTES,
  ResearchEvidenceFact, ResearchEvidenceKind, ResearchInput, ResearchObservations, ResearchOutcome, ResearchResult,
  ResearchRoute, ResearchSchema, digest, invalid,
};
use crate::{
  BudgetLimit, BudgetUsage, EvidenceRequirement, FactoryConfigurationRef, FactoryDigest, FactoryError, FactoryKey,
  FactoryPermissionSet, FlowDefinitionRef, FlowNodeKind, FlowTransitionTarget, ImmutableReference,
  PinnedFlowDefinitionClosure, RiskClass, ValidatedFlowDefinitionClosure, WorkKind, WorkSize, reserve_phase_budget,
};
use octacity_server_domain::Timestamp;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// Exact tool/model/task selection for one externally executed research node.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResearchNodeProfile {
  /// Exact definition containing the node.
  pub definition: FlowDefinitionRef,
  /// Stable node key in that definition.
  pub node: FactoryKey,
  /// Exact selected harness or deterministic tool.
  pub tool: ImmutableReference,
  /// Exact runner plugin.
  pub plugin: ImmutableReference,
  /// Exact model or command configuration.
  pub model_or_tool: ImmutableReference,
  /// Exact task/prompt contract.
  pub task_digest: FactoryDigest,
}

/// Operator-owned bounded research settings; constructing policy validates every field.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResearchPolicySettings {
  /// Aggregate attempts, time, tokens, cost, and output ceiling.
  pub budget: BudgetLimit,
  /// Reservation for one further externally executed attempt.
  pub attempt_budget: BudgetLimit,
  /// Immutable maximum research authority.
  pub permissions: FactoryPermissionSet,
  /// Maximum risk eligible for the small-Work protected-test path.
  pub small_work_max_risk: RiskClass,
  /// Maximum proposal Artifact bytes, within product and aggregate bounds.
  pub max_proposal_bytes: u64,
  /// Complete finite mapping for the configured Work kind.
  #[serde(serialize_with = "serialize_outcomes")]
  pub outcomes: BTreeMap<ResearchOutcome, ResearchRoute>,
  /// Declared terminal disposition when further attempts cannot fit.
  pub exhausted_route: ResearchRoute,
  /// Exact independently validated report requirements.
  pub evidence: BTreeMap<ResearchEvidenceKind, EvidenceRequirement>,
}

/// Immutable Flow-bound code-owned research policy with no provider deserializer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResearchPolicy {
  configuration: FactoryConfigurationRef,
  closure: PinnedFlowDefinitionClosure,
  kind: WorkKind,
  settings: ResearchPolicySettings,
  profiles: Vec<ResearchNodeProfile>,
  declared_routes: BTreeSet<ResearchRoute>,
}

/// Bounded authorization of one more attempt, or a declared terminal fallback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResearchAttemptDisposition {
  /// Dispatch only with this exact budget and narrowed authority.
  Dispatch {
    /// Next monotonic bounded attempt number.
    attempt: u32,
    /// Per-attempt reservation.
    budget: BudgetLimit,
    /// Authority narrowed to the immutable research ceiling.
    permissions: Box<FactoryPermissionSet>,
  },
  /// Further research cannot fit within the immutable budget.
  Exhausted(ResearchRoute),
}

/// Secret-safe reason for a code-owned research successor.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchReason {
  /// All required evidence accepted the declared observed outcome.
  AcceptedOutcome,
  /// Required deterministic evidence was absent or stale.
  MissingEvidence,
  /// The finite attempt budget was exhausted without reproduction.
  AttemptsExhausted,
  /// Missing inputs require human disposition.
  HumanInputRequired,
  /// Work size or risk requires authored requirements before development.
  RequirementsRequired,
  /// The required safe successor was not declared.
  UndeclaredSuccessor,
}

/// Auditable finite successor; this record grants no readiness or execution authority.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResearchDecision {
  /// Exact code-owned declared successor.
  pub route: ResearchRoute,
  /// Stable deterministic reason.
  pub reason: ResearchReason,
  /// Complete immutable research input.
  pub input_digest: FactoryDigest,
  /// Complete retained observations.
  pub result_digest: FactoryDigest,
  /// Exact immutable policy and definition closure.
  pub policy_digest: FactoryDigest,
  /// Canonical complete trusted evidence set.
  pub evidence_digest: FactoryDigest,
}

impl ResearchPolicy {
  /// Binds a validated research closure, exact profiles, finite outcomes, and immutable limits.
  pub fn new(
    configuration: FactoryConfigurationRef,
    closure: &ValidatedFlowDefinitionClosure,
    kind: WorkKind,
    settings: ResearchPolicySettings,
    mut profiles: Vec<ResearchNodeProfile>,
  ) -> Result<Self, FactoryError> {
    validate_budget(settings.budget)?;
    validate_budget(settings.attempt_budget)?;
    if settings.budget.max_attempts() > MAX_RESEARCH_ATTEMPTS
      || settings.attempt_budget.max_attempts() != 1
      || !settings.attempt_budget.fits_within(settings.budget)
      || !settings.budget.fits_within(closure.limits().budget())
      || !settings.permissions.is_no_broader_than(closure.limits().permissions())
      || settings.max_proposal_bytes == 0
      || settings.max_proposal_bytes > MAX_RESEARCH_PROPOSAL_BYTES
      || settings.max_proposal_bytes > settings.budget.max_output_bytes()
      || settings.max_proposal_bytes > settings.attempt_budget.max_output_bytes()
      || !matches!(
        settings.exhausted_route,
        ResearchRoute::Escalation | ResearchRoute::Rejection
      )
    {
      return Err(invalid("research policy bounds"));
    }
    let root = closure
      .closure()
      .definition(closure.closure().root())
      .ok_or_else(|| invalid("research root"))?;
    if root.input_schema() != Some(&ResearchSchema::Input.reference()?) {
      return Err(invalid("research input schema"));
    }
    let mut routes = BTreeSet::new();
    for terminal in root.terminals() {
      let route = ResearchRoute::ALL
        .into_iter()
        .find(|route| route.as_str() == terminal.key().as_str())
        .ok_or_else(|| invalid("research finite successor"))?;
      if terminal.schema() != &ResearchSchema::Decision.reference()? {
        return Err(invalid("research decision schema"));
      }
      routes.insert(route);
    }
    if !routes.contains(&ResearchRoute::Escalation)
      || !routes.contains(&settings.exhausted_route)
      || settings.outcomes.values().any(|route| !routes.contains(route))
    {
      return Err(invalid("research undeclared policy successor"));
    }
    let expected: BTreeSet<_> = match kind {
      WorkKind::Defect => [
        DefectResearchOutcome::Reproduced,
        DefectResearchOutcome::Intermittent,
        DefectResearchOutcome::EnvironmentSpecific,
        DefectResearchOutcome::CannotReproduce,
        DefectResearchOutcome::NeedsHumanInput,
      ]
      .map(ResearchOutcome::Defect)
      .into_iter()
      .collect(),
      WorkKind::FeatureRequest => [ResearchOutcome::FeatureProposal].into_iter().collect(),
    };
    if settings.outcomes.keys().copied().collect::<BTreeSet<_>>() != expected
      || kind == WorkKind::Defect
        && settings
          .outcomes
          .get(&ResearchOutcome::Defect(DefectResearchOutcome::NeedsHumanInput))
          != Some(&ResearchRoute::Escalation)
    {
      return Err(invalid("research outcome coverage"));
    }
    // Only a deterministic gate may export a policy successor from research.
    for edge in root.transitions() {
      if let FlowTransitionTarget::Terminal(target) = edge.target() {
        let gate = root
          .node(edge.predecessor())
          .ok_or_else(|| invalid("research policy gate"))?;
        if gate.kind() != FlowNodeKind::DeterministicGate
          || edge.outcome() != target
          || gate.input_schema() != Some(&ResearchSchema::result(kind).reference()?)
        {
          return Err(invalid("research route authority"));
        }
      }
    }
    profiles.sort_by(|a, b| (a.definition, &a.node).cmp(&(b.definition, &b.node)));
    let selected = profiles
      .iter()
      .map(|row| (row.definition, row.node.clone()))
      .collect::<BTreeSet<_>>();
    let mut required = BTreeSet::new();
    for definition in closure.closure().definitions() {
      for node in definition.nodes() {
        if !node.permissions().is_no_broader_than(&settings.permissions) || !node.budget().fits_within(settings.budget)
        {
          return Err(invalid("research node authority or budget"));
        }
        if matches!(node.kind(), FlowNodeKind::Reasoning | FlowNodeKind::BuildCommand) {
          if !node.budget().fits_within(settings.attempt_budget) {
            return Err(invalid("research per-node attempt budget"));
          }
          required.insert((definition.reference(), node.key().clone()));
        }
      }
    }
    if required.is_empty() || required != selected || selected.len() != profiles.len() {
      return Err(invalid("research exact node profiles"));
    }
    let primary = if kind == WorkKind::Defect {
      ResearchEvidenceKind::Reproduction
    } else {
      ResearchEvidenceKind::Proposal
    };
    if !settings.evidence.contains_key(&primary) || settings.evidence.len() > 4 {
      return Err(invalid("research required evidence policy"));
    }
    for (route, evidence) in [
      (ResearchRoute::Verification, ResearchEvidenceKind::Verification),
      (
        ResearchRoute::TerminalResolution,
        ResearchEvidenceKind::TerminalResolution,
      ),
    ] {
      if settings.outcomes.values().any(|selected| selected == &route) && !settings.evidence.contains_key(&evidence) {
        return Err(invalid("research resolution evidence policy"));
      }
    }
    Ok(Self {
      configuration,
      closure: closure.closure().clone(),
      kind,
      settings,
      profiles,
      declared_routes: routes,
    })
  }

  /// Authorizes one additional bounded attempt without granting broader permissions.
  pub fn next_attempt(
    &self,
    input: &ResearchInput,
    usage: BudgetUsage,
    requested: &FactoryPermissionSet,
  ) -> Result<ResearchAttemptDisposition, FactoryError> {
    self.validate_admission(input)?;
    usage.validate(self.settings.budget)?;
    if !requested.is_no_broader_than(&self.settings.permissions) {
      return Err(invalid("research authority widening"));
    }
    let budget = self.settings.attempt_budget;
    let reserve = BudgetUsage {
      attempts: 1,
      elapsed_millis: budget.max_elapsed_millis(),
      tokens: budget.max_tokens(),
      cost_micro_units: budget.max_cost_micro_units(),
      output_bytes: budget.max_output_bytes(),
    };
    if reserve_phase_budget(usage, reserve, self.settings.budget).is_none() {
      return Ok(ResearchAttemptDisposition::Exhausted(self.settings.exhausted_route));
    }
    Ok(ResearchAttemptDisposition::Dispatch {
      attempt: usage.attempts + 1,
      budget,
      permissions: Box::new(requested.intersection(&self.settings.permissions)),
    })
  }

  /// Selects only a declared safe successor from exact observations and independently accepted evidence.
  pub fn evaluate(
    &self,
    input: &ResearchInput,
    result: &ResearchResult,
    evidence: &[AcceptedResearchEvidence],
    usage: BudgetUsage,
    at: Timestamp,
  ) -> Result<ResearchDecision, FactoryError> {
    self.validate_admission(input)?;
    result.validate_input(input)?;
    usage.validate(self.settings.budget)?;
    let provenance = result.provenance();
    let profile = self
      .profiles
      .iter()
      .find(|row| row.definition == provenance.definition && row.node == provenance.node)
      .ok_or_else(|| invalid("research producer node"))?;
    if provenance.producer.tool() != &profile.tool
      || provenance.producer.plugin() != &profile.plugin
      || provenance.model_or_tool != profile.model_or_tool
      || provenance.task_digest != profile.task_digest
      || provenance.attempt.get() > u64::from(self.settings.budget.max_attempts())
      || provenance.attempt.get() > u64::from(usage.attempts)
      || at < provenance.observed_at
      || evidence.len() > super::MAX_RESEARCH_ITEMS
    {
      return Err(invalid("research execution profile or usage"));
    }
    if let ResearchObservations::Feature(row) = result.observations()
      && row.proposal.encoded_size() > self.settings.max_proposal_bytes
    {
      return Err(invalid("research proposal policy bound"));
    }
    let input_digest = input.digest()?;
    let result_digest = result.digest()?;
    let mut proofs = Vec::new();
    for proof in evidence {
      let record = proof.record();
      let requirement = self
        .settings
        .evidence
        .get(&record.fact.kind())
        .ok_or_else(|| invalid("research undeclared evidence kind"))?;
      if record.subject != *input.subject()
        || record.input_digest != input_digest
        || proof.result_digest != result_digest
        || &record.schema != requirement.schema()
        || record.output_kind != requirement.output_kind()
        || record.producer.tool() != requirement.tool()
        || record.producer.plugin() != requirement.plugin()
        || at < record.verified_at
      {
        return Err(invalid("research evidence policy binding"));
      }
      proofs.push(proof.digest()?);
    }
    proofs.sort();
    if proofs.windows(2).any(|pair| pair[0] == pair[1]) {
      return Err(invalid("duplicate research proof"));
    }
    let accepted = |fact| {
      evidence
        .iter()
        .any(|row| row.record.fact == fact && at < row.record.fresh_until)
    };
    let primary = match result.outcome() {
      ResearchOutcome::Defect(outcome) => ResearchEvidenceFact::Reproduction(outcome),
      ResearchOutcome::FeatureProposal => ResearchEvidenceFact::Proposal,
    };
    let (mut route, mut reason) = if !accepted(primary) {
      (ResearchRoute::Escalation, ResearchReason::MissingEvidence)
    } else if result.outcome() == ResearchOutcome::Defect(DefectResearchOutcome::NeedsHumanInput) {
      (ResearchRoute::Escalation, ResearchReason::HumanInputRequired)
    } else if result.outcome() == ResearchOutcome::Defect(DefectResearchOutcome::CannotReproduce)
      && usage.attempts >= self.settings.budget.max_attempts()
    {
      (self.settings.exhausted_route, ResearchReason::AttemptsExhausted)
    } else {
      (
        *self
          .settings
          .outcomes
          .get(&result.outcome())
          .ok_or_else(|| invalid("research observed outcome"))?,
        ResearchReason::AcceptedOutcome,
      )
    };
    if matches!(route, ResearchRoute::Verification | ResearchRoute::TerminalResolution) {
      let required = if route == ResearchRoute::Verification {
        ResearchEvidenceFact::Verification
      } else {
        ResearchEvidenceFact::TerminalResolution
      };
      if !accepted(required) {
        route = ResearchRoute::Escalation;
        reason = ResearchReason::MissingEvidence;
      }
    }
    if route == ResearchRoute::ProtectedTest
      && (*input.accepted_triage().result.classification().size.value() != WorkSize::Small
        || (*input.accepted_triage().result.classification().risk.value())
          .max(input.accepted_triage().eligibility.input.admission_risk())
          > self.settings.small_work_max_risk)
    {
      route = ResearchRoute::Requirements;
      reason = ResearchReason::RequirementsRequired;
    }
    if !self.declared_routes.contains(&route) {
      route = ResearchRoute::Escalation;
      reason = ResearchReason::UndeclaredSuccessor;
    }
    Ok(ResearchDecision {
      route,
      reason,
      input_digest,
      result_digest,
      policy_digest: self.digest()?,
      evidence_digest: digest("octacity.factory.research-evidence-set.v1", &proofs)?,
    })
  }
  /// Returns the exact immutable policy identity, including the complete pinned closure.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    digest("octacity.factory.research-policy.v1", self)
  }
  /// Returns the immutable resource and successor settings.
  #[must_use]
  pub const fn settings(&self) -> &ResearchPolicySettings {
    &self.settings
  }

  fn validate_admission(&self, input: &ResearchInput) -> Result<(), FactoryError> {
    if input.configuration() != &self.configuration
      || input.kind() != self.kind
      || input.policy_digest() != self.digest()?
      || !self.settings.budget.fits_within(input.admission_limits().budget())
      || !self
        .settings
        .permissions
        .is_no_broader_than(input.admission_limits().permissions())
    {
      return Err(invalid("research admitted authority or budget"));
    }
    Ok(())
  }
}

fn validate_budget(budget: BudgetLimit) -> Result<(), FactoryError> {
  BudgetLimit::new(
    budget.max_attempts(),
    budget.max_elapsed_millis(),
    budget.max_tokens(),
    budget.max_cost_micro_units(),
    budget.max_output_bytes(),
  )
  .map(|_| ())
}

fn serialize_outcomes<S: serde::Serializer>(
  outcomes: &BTreeMap<ResearchOutcome, ResearchRoute>,
  serializer: S,
) -> Result<S::Ok, S::Error> {
  outcomes.iter().collect::<Vec<_>>().serialize(serializer)
}
