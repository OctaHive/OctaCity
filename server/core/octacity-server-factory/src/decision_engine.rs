use std::collections::{BTreeMap, HashSet};

use crate::{
  Assessment, AssessmentOutcome, Decision, DecisionId, DecisionOutcome, DecisionPolicyVersion, DecisionReason,
  DeterministicGateOutcome, EvaluationPlan, EvidenceItem, EvidenceManifest, FactoryDigest, FactoryError, FactoryKey,
  FindingSeverity, IndeterminatePolicy, MAX_CRITERION_PACKS, MAX_DECISION_ASSESSMENTS, MAX_EVALUATORS,
  evaluation::DecisionBindings,
};

/// Unvalidated rules used to publish one immutable Decision policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecisionPolicyDefinition {
  /// Deterministic evidence kinds that must produce a passing gate.
  pub required_evidence: Vec<FactoryKey>,
  /// Evaluators whose immutable Assessment must be present.
  pub required_evaluators: Vec<FactoryKey>,
  /// Minimum number of distinct declared evaluator Assessments.
  pub quorum: u16,
  /// Lowest violation severity that blocks acceptance.
  pub severity_threshold: FindingSeverity,
  /// Whether optional indeterminate Assessments also block acceptance.
  pub indeterminate_policy: IndeterminatePolicy,
  /// Typed non-accepting outcome used when any policy rule fails.
  pub failure_outcome: DecisionOutcome,
}

/// Immutable versioned rules used by the pure Decision Engine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecisionPolicy {
  version: DecisionPolicyVersion,
  digest: FactoryDigest,
  required_evidence: Vec<FactoryKey>,
  required_evaluators: Vec<FactoryKey>,
  quorum: u16,
  severity_threshold: FindingSeverity,
  indeterminate_policy: IndeterminatePolicy,
  failure_outcome: DecisionOutcome,
}

impl DecisionPolicy {
  /// Validates and canonically orders one immutable Decision policy.
  pub fn try_new(
    version: DecisionPolicyVersion,
    mut definition: DecisionPolicyDefinition,
  ) -> Result<Self, FactoryError> {
    validate_policy_keys(&definition.required_evidence, MAX_CRITERION_PACKS, "required_evidence")?;
    validate_policy_keys(&definition.required_evaluators, MAX_EVALUATORS, "required_evaluators")?;
    if definition.quorum == 0 || usize::from(definition.quorum) > MAX_EVALUATORS {
      return Err(FactoryError::InvalidDecisionPolicy { field: "quorum" });
    }
    if definition.failure_outcome == DecisionOutcome::Accept {
      return Err(FactoryError::InvalidDecisionPolicy {
        field: "failure_outcome",
      });
    }
    definition.required_evidence.sort();
    definition.required_evaluators.sort();
    let digest = digest_policy(&definition);
    Ok(Self {
      version,
      digest,
      required_evidence: definition.required_evidence,
      required_evaluators: definition.required_evaluators,
      quorum: definition.quorum,
      severity_threshold: definition.severity_threshold,
      indeterminate_policy: definition.indeterminate_policy,
      failure_outcome: definition.failure_outcome,
    })
  }

  /// Returns the immutable policy version.
  #[must_use]
  pub const fn version(&self) -> DecisionPolicyVersion {
    self.version
  }

  /// Returns the canonical policy digest.
  #[must_use]
  pub const fn digest(&self) -> FactoryDigest {
    self.digest
  }
}

/// Complete immutable inputs to one deterministic Decision evaluation.
#[derive(Clone, Copy, Debug)]
pub struct DecisionEngineInput<'a> {
  evidence: &'a EvidenceManifest,
  plan: &'a EvaluationPlan,
  policy: &'a DecisionPolicy,
  assessments: &'a [Assessment],
}

impl<'a> DecisionEngineInput<'a> {
  /// Captures exact evidence, assessments, and policy for canonical evaluation.
  #[must_use]
  pub const fn new(
    evidence: &'a EvidenceManifest,
    plan: &'a EvaluationPlan,
    policy: &'a DecisionPolicy,
    assessments: &'a [Assessment],
  ) -> Self {
    Self {
      evidence,
      plan,
      policy,
      assessments,
    }
  }
}

/// Applies immutable policy to exact evidence and Assessments without side effects.
pub fn evaluate_decision(id: DecisionId, input: DecisionEngineInput<'_>) -> Result<Decision, FactoryError> {
  validate_input(&input)?;
  let input_digest = digest_decision_input(&input)?;
  let gates = input
    .evidence
    .items()
    .iter()
    .map(|item| (item.kind(), item))
    .collect::<BTreeMap<_, _>>();
  let assessments = input
    .assessments
    .iter()
    .map(|assessment| (assessment.evaluator(), assessment))
    .collect::<BTreeMap<_, _>>();
  let mut reasons = evaluate_required_evidence(input.policy, &gates);
  reasons.extend(evaluate_assessments(input.policy, &assessments));
  if input.assessments.len() < usize::from(input.policy.quorum) {
    reasons.push(DecisionReason::QuorumNotMet {
      required: input.policy.quorum,
      observed: u16::try_from(input.assessments.len()).expect("assessment count is bounded"),
    });
  }
  let outcome = if reasons.is_empty() {
    reasons.push(DecisionReason::PolicySatisfied);
    DecisionOutcome::Accept
  } else {
    input.policy.failure_outcome
  };
  let ordered_assessments = assessments
    .values()
    .map(|assessment| (*assessment).clone())
    .collect::<Vec<_>>();
  Decision::from_engine(
    id,
    input.plan,
    outcome,
    &ordered_assessments,
    reasons,
    DecisionBindings {
      input_digest,
      policy_version: input.policy.version,
      policy_digest: input.policy.digest,
    },
  )
}

fn digest_policy(definition: &DecisionPolicyDefinition) -> FactoryDigest {
  let quorum = definition.quorum.to_be_bytes();
  let mut owned_fields = definition
    .required_evidence
    .iter()
    .map(|key| format!("evidence\0{key}").into_bytes())
    .collect::<Vec<_>>();
  owned_fields.extend(
    definition
      .required_evaluators
      .iter()
      .map(|key| format!("evaluator\0{key}").into_bytes()),
  );
  let mut fields = vec![
    quorum.as_slice(),
    definition.severity_threshold.as_str().as_bytes(),
    definition.indeterminate_policy.as_str().as_bytes(),
    definition.failure_outcome.as_str().as_bytes(),
  ];
  fields.extend(owned_fields.iter().map(Vec::as_slice));
  FactoryDigest::sha256("octacity.decision-policy.v1", &fields)
}

fn digest_decision_input(input: &DecisionEngineInput<'_>) -> Result<FactoryDigest, FactoryError> {
  let evidence_id = input.evidence.id().as_uuid();
  let plan_id = input.plan.id().as_uuid();
  let candidate_digest = input.evidence.subject().changeset_digest().as_bytes();
  let policy_digest = input.policy.digest().as_bytes();
  let mut owned_fields = input
    .evidence
    .items()
    .iter()
    .map(encode_evidence_item)
    .collect::<Vec<_>>();
  let plan_digest = input.plan.digest()?.as_bytes();
  let mut assessments = input.assessments.iter().map(encode_assessment).collect::<Vec<_>>();
  assessments.sort();
  owned_fields.extend(assessments);
  let mut fields = vec![
    evidence_id.as_bytes().as_slice(),
    plan_id.as_bytes().as_slice(),
    candidate_digest.as_slice(),
    policy_digest.as_slice(),
    plan_digest.as_slice(),
  ];
  fields.extend(owned_fields.iter().map(Vec::as_slice));
  Ok(FactoryDigest::sha256("octacity.decision-engine.input.v2", &fields))
}

fn encode_evidence_item(item: &EvidenceItem) -> Vec<u8> {
  let producer = item.producer();
  let schema = item.schema();
  let tool = producer.tool();
  let plugin = producer.plugin();
  format!(
    "evidence\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}",
    item.kind(),
    item.output_kind().as_str(),
    item.artifact_id(),
    item.digest(),
    schema.identity(),
    schema.version(),
    schema.digest(),
    producer.build_id(),
    producer.attempt_id(),
    producer.job_id(),
    tool.identity(),
    tool.version(),
    tool.digest(),
    plugin.identity(),
    plugin.version(),
    plugin.digest(),
    item.published_at().unix_millis(),
    item.fresh_until().unix_millis(),
    item.outcome().as_str(),
  )
  .into_bytes()
}

fn encode_assessment(assessment: &Assessment) -> Vec<u8> {
  serde_json::to_vec(assessment).expect("validated Assessment serializes")
}

fn validate_input(input: &DecisionEngineInput<'_>) -> Result<(), FactoryError> {
  if input.plan.evidence_id() != input.evidence.id() || input.plan.subject() != input.evidence.subject() {
    return Err(FactoryError::InconsistentSubject);
  }
  if input.policy.quorum != input.plan.required_quorum()
    || usize::from(input.policy.quorum) > input.plan.evaluator_count()
    || input
      .policy
      .required_evaluators
      .iter()
      .any(|evaluator| !input.plan.has_evaluator_identity(evaluator))
  {
    return Err(FactoryError::InvalidDecisionPolicy {
      field: "evaluation_plan",
    });
  }
  if input.assessments.len() > MAX_DECISION_ASSESSMENTS {
    return Err(FactoryError::CollectionLimitExceeded {
      collection: "decision assessments",
    });
  }
  validate_assessments(input.plan, input.evidence, input.assessments)
}

fn validate_assessments(
  plan: &EvaluationPlan,
  evidence: &EvidenceManifest,
  assessments: &[Assessment],
) -> Result<(), FactoryError> {
  if assessments
    .iter()
    .any(|assessment| assessment.plan_id() != plan.id() || assessment.subject() != plan.subject())
  {
    return Err(FactoryError::InconsistentSubject);
  }
  if assessments.iter().map(Assessment::id).collect::<HashSet<_>>().len() != assessments.len()
    || assessments
      .iter()
      .map(Assessment::evaluator)
      .collect::<HashSet<_>>()
      .len()
      != assessments.len()
  {
    return Err(FactoryError::InvalidReference {
      relationship: "decision assessment",
    });
  }
  for assessment in assessments {
    assessment.validate_bindings(plan, evidence)?;
  }
  Ok(())
}

fn evaluate_required_evidence(
  policy: &DecisionPolicy,
  gates: &BTreeMap<&FactoryKey, &EvidenceItem>,
) -> Vec<DecisionReason> {
  policy
    .required_evidence
    .iter()
    .filter_map(|key| match gates.get(key).map(|item| item.outcome()) {
      None => Some(DecisionReason::RequiredEvidenceMissing(key.clone())),
      Some(DeterministicGateOutcome::Failed) => Some(DecisionReason::RequiredEvidenceFailed(key.clone())),
      Some(DeterministicGateOutcome::Indeterminate) => Some(DecisionReason::RequiredEvidenceIndeterminate(key.clone())),
      Some(DeterministicGateOutcome::Passed) => None,
    })
    .collect()
}

fn evaluate_assessments(
  policy: &DecisionPolicy,
  assessments: &BTreeMap<&FactoryKey, &Assessment>,
) -> Vec<DecisionReason> {
  let mut reasons = policy
    .required_evaluators
    .iter()
    .filter(|evaluator| !assessments.contains_key(evaluator))
    .cloned()
    .map(DecisionReason::RequiredAssessmentMissing)
    .collect::<Vec<_>>();

  for (evaluator, assessment) in assessments {
    match assessment.outcome() {
      AssessmentOutcome::Indeterminate
        if policy.required_evaluators.contains(evaluator)
          || policy.indeterminate_policy == IndeterminatePolicy::Any =>
      {
        reasons.push(DecisionReason::AssessmentIndeterminate((*evaluator).clone()));
      }
      AssessmentOutcome::Violated => {
        if let Some(severity) = assessment
          .findings()
          .iter()
          .filter_map(|finding| finding.severity())
          .max()
          && severity >= policy.severity_threshold
        {
          reasons.push(DecisionReason::SeverityThresholdExceeded {
            evaluator: (*evaluator).clone(),
            severity,
          });
        }
      }
      AssessmentOutcome::Satisfied | AssessmentOutcome::Indeterminate => {}
    }
  }
  reasons
}

fn validate_policy_keys(values: &[FactoryKey], maximum: usize, field: &'static str) -> Result<(), FactoryError> {
  if values.is_empty() || values.len() > maximum || values.iter().collect::<HashSet<_>>().len() != values.len() {
    return Err(FactoryError::InvalidDecisionPolicy { field });
  }
  Ok(())
}
