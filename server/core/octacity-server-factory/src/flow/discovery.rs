//! Safe owner-supplied facts retained in the common fenced result journal.
//! These facts describe verified domain results; they never select a Flow transition.
use crate::*;
use serde::{Deserialize, Serialize};

/// Closed technical classifications of safe owner observations, independent of logical nodes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FlowDiscoveryKind {
  /// Candidate publication.
  Candidate,
  /// Verified candidate capture.
  ChangeSet,
  /// Verified evidence manifest.
  Evidence,
  /// Frozen review plan.
  ReviewPlan,
  /// Evaluator assessment.
  Assessment,
  /// Advisory signal receipt.
  DecisionSignal,
  /// Authoritative disposition.
  Decision,
  /// Verified delivery attempt.
  Delivery,
  /// Bounded escalation.
  Escalation,
  /// Verified rollout observation.
  Promotion,
  /// Reporting attempt.
  Reporting,
}
impl FlowDiscoveryKind {
  /// Stable storage classification retained in the typed result envelope.
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Candidate => "candidate",
      Self::ChangeSet => "changeset",
      Self::Evidence => "evidence",
      Self::ReviewPlan => "review_plan",
      Self::Assessment => "assessment",
      Self::DecisionSignal => "decision_signal",
      Self::Decision => "decision",
      Self::Delivery => "delivery",
      Self::Escalation => "escalation",
      Self::Promotion => "promotion",
      Self::Reporting => "reporting",
    }
  }
}
/// Typed read projection authored by a trusted adapter, never by provider payload inspection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct FlowDiscoveryFacts(DiscoveryFact);
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum DiscoveryFact {
  Reporting {
    subject: crate::WorkSubject,
    id: ReportingAttemptId,
    number: ReportingAttemptNumber,
    state: ReportingState,
  },
  Promotion {
    subject: CandidateSubject,
    plan_digest: FactoryDigest,
    input_digest: FactoryDigest,
    environment_contract: ImmutableReference,
    deployment: ImmutableReference,
    outcome: DeterministicGateOutcome,
    started_at: octacity_server_domain::Timestamp,
    finished_at: octacity_server_domain::Timestamp,
  },
  #[serde(rename = "changeset")]
  ChangeSet {
    subject: CandidateSubject,
    id: ChangeSetId,
    node_attempt_id: NodeAttemptId,
  },
  Evidence {
    subject: CandidateSubject,
    id: EvidenceManifestId,
    changeset_id: ChangeSetId,
    digest: FactoryDigest,
    item_count: usize,
  },
  ReviewPlan {
    subject: CandidateSubject,
    id: EvaluationPlanId,
    evidence_id: EvidenceManifestId,
    digest: FactoryDigest,
    purpose: ReviewPurpose,
    required_quorum: u16,
    deadline: octacity_server_domain::Timestamp,
  },
  Assessment {
    subject: CandidateSubject,
    id: AssessmentId,
    plan_id: EvaluationPlanId,
    digest: FactoryDigest,
    evaluator: ImmutableReference,
    model: ImmutableReference,
    prompt_digest: FactoryDigest,
    outcome: AssessmentOutcome,
  },
  DecisionSignal {
    subject: WorkSubject,
    id: DecisionSignalReceiptId,
    request_id: DecisionSignalRequestId,
    receipt_digest: FactoryDigest,
    input_digest: FactoryDigest,
    frozen_input_digest: FactoryDigest,
    question_set: Box<ImmutableReference>,
    probability_semantics: Box<ImmutableReference>,
    provider: Box<ImmutableReference>,
    adapter: Box<ImmutableReference>,
    model: Box<ImmutableReference>,
    policy: Box<ImmutableReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    purpose: Option<DecisionSignalPurpose>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mode: Option<DecisionSignalMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fallback: Option<DecisionSignalFallback>,
    #[serde(
      default,
      skip_serializing_if = "Vec::is_empty",
      deserialize_with = "bounded_list::<_, _, 0, MAX_DECISION_SIGNAL_QUESTIONS>"
    )]
    answers: Vec<DecisionSignalAnswer>,
    state: DecisionSignalState,
    consumption: DecisionSignalConsumption,
  },
  Candidate {
    subject: CandidateSubject,
  },
  Decision {
    subject: CandidateSubject,
    id: DecisionId,
    outcome: DecisionOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    explanation: Option<Box<DecisionExplanation>>,
  },
  Delivery {
    subject: CandidateSubject,
    id: DeliveryAttemptId,
    decision_id: DecisionId,
    state: DeliveryState,
  },
  Escalation {
    subject: crate::WorkSubject,
    id: EscalationId,
  },
}

// Explicit privacy allowlist. Historical facts omit this extension so their
// retained bytes and producing record digests remain unchanged on restoration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct DecisionExplanation {
  plan_id: EvaluationPlanId,
  #[serde(deserialize_with = "bounded_list::<_, _, 0, MAX_DECISION_ASSESSMENTS>")]
  assessment_ids: Vec<AssessmentId>,
  #[serde(deserialize_with = "bounded_list::<_, _, 0, MAX_DECISION_SIGNAL_RECEIPTS>")]
  signal_receipt_ids: Vec<DecisionSignalReceiptId>,
  #[serde(deserialize_with = "bounded_list::<_, _, 1, MAX_DECISION_REASONS>")]
  reasons: Vec<DecisionReason>,
  input_digest: FactoryDigest,
  policy_version: DecisionPolicyVersion,
  policy_digest: FactoryDigest,
}

fn bounded_list<'de, D, T, const MIN: usize, const MAX: usize>(deserializer: D) -> Result<Vec<T>, D::Error>
where
  D: serde::Deserializer<'de>,
  T: Deserialize<'de>,
{
  let values = Vec::<T>::deserialize(deserializer)?;
  if !(MIN..=MAX).contains(&values.len()) {
    return Err(serde::de::Error::custom("decision explanation collection bound"));
  }
  Ok(values)
}
impl FlowDiscoveryFacts {
  /// Work dispositions retain correlation; candidate facts require the selected code resource.
  pub(super) fn matches_input(&self, input: &super::FlowNodeInput) -> bool {
    match &self.0 {
      DiscoveryFact::Escalation { subject, .. }
      | DiscoveryFact::Reporting { subject, .. }
      | DiscoveryFact::DecisionSignal { subject, .. } => subject == input.subject(),
      // Trusted capture/publication may produce a new candidate from the selected
      // base. Consumer facts below must instead describe the exact selected candidate.
      DiscoveryFact::Candidate { subject } | DiscoveryFact::ChangeSet { subject, .. } => {
        input.exact_resource().is_ok_and(|resource| resource == subject.exact())
      }
      _ => self
        .candidate_subject()
        .is_some_and(|subject| input.matches_candidate_resource(subject)),
    }
  }

  /// Returns a code-owned observation family without inspecting arbitrary payload JSON.
  pub const fn kind(&self) -> FlowDiscoveryKind {
    match self.0 {
      DiscoveryFact::Candidate { .. } => FlowDiscoveryKind::Candidate,
      DiscoveryFact::ChangeSet { .. } => FlowDiscoveryKind::ChangeSet,
      DiscoveryFact::Evidence { .. } => FlowDiscoveryKind::Evidence,
      DiscoveryFact::ReviewPlan { .. } => FlowDiscoveryKind::ReviewPlan,
      DiscoveryFact::Assessment { .. } => FlowDiscoveryKind::Assessment,
      DiscoveryFact::DecisionSignal { .. } => FlowDiscoveryKind::DecisionSignal,
      DiscoveryFact::Decision { .. } => FlowDiscoveryKind::Decision,
      DiscoveryFact::Delivery { .. } => FlowDiscoveryKind::Delivery,
      DiscoveryFact::Escalation { .. } => FlowDiscoveryKind::Escalation,
      DiscoveryFact::Promotion { .. } => FlowDiscoveryKind::Promotion,
      DiscoveryFact::Reporting { .. } => FlowDiscoveryKind::Reporting,
    }
  }

  /// Exact verified rollout receipt without measurements or private target locations.
  #[must_use]
  pub fn promotion(observation: &PromotionObservation) -> Self {
    Self(DiscoveryFact::Promotion {
      subject: observation.subject.clone(),
      plan_digest: observation.plan_digest,
      input_digest: observation.input_digest,
      environment_contract: observation.environment_contract.clone(),
      deployment: observation.deployment.clone(),
      outcome: observation.outcome,
      started_at: observation.started_at,
      finished_at: observation.finished_at,
    })
  }
  /// Bounded reporting outcome without connector identity or private source links.
  #[must_use]
  pub fn reporting(report: &ReportingAttempt) -> Self {
    Self(DiscoveryFact::Reporting {
      subject: report.subject().clone(),
      id: report.id(),
      number: report.number(),
      state: report.state(),
    })
  }

  /// Verified candidate identity, omitting retained bundles and private manifests.
  #[must_use]
  pub fn changeset(candidate: &ChangeSet) -> Self {
    Self(DiscoveryFact::ChangeSet {
      subject: candidate.subject().clone(),
      id: candidate.id(),
      node_attempt_id: candidate.node_attempt_id(),
    })
  }
  /// Content-bound manifest identity without protected inputs or evidence bytes.
  pub fn evidence(manifest: &EvidenceManifest) -> Result<Self, FactoryError> {
    Ok(Self(DiscoveryFact::Evidence {
      subject: manifest.subject().clone(),
      id: manifest.id(),
      changeset_id: manifest.changeset_id(),
      digest: manifest.digest()?,
      item_count: manifest.items().len(),
    }))
  }
  /// Frozen review authority without criterion-pack material or test references.
  pub fn review_plan(plan: &EvaluationPlan) -> Result<Self, FactoryError> {
    Ok(Self(DiscoveryFact::ReviewPlan {
      subject: plan.subject().clone(),
      id: plan.id(),
      evidence_id: plan.evidence_id(),
      digest: plan.digest()?,
      purpose: plan.purpose(),
      required_quorum: plan.required_quorum(),
      deadline: plan.deadline(),
    }))
  }
  /// Exact evaluator provenance and outcome, excluding its prose and findings.
  pub fn assessment(assessment: &Assessment) -> Result<Self, FactoryError> {
    let provenance = assessment.provenance();
    Ok(Self(DiscoveryFact::Assessment {
      subject: assessment.subject().clone(),
      id: assessment.id(),
      plan_id: assessment.plan_id(),
      digest: assessment.digest()?,
      evaluator: assessment.evaluator_reference().clone(),
      model: provenance.model().clone(),
      prompt_digest: provenance.prompt_digest(),
      outcome: assessment.outcome(),
    }))
  }
  /// Advisory observation and separately typed code-owned consumption; no provider payload.
  #[must_use]
  pub fn decision_signal(receipt: &DecisionSignalReceipt) -> Self {
    Self(DiscoveryFact::DecisionSignal {
      subject: receipt.subject().clone(),
      id: receipt.id(),
      request_id: receipt.request_id(),
      receipt_digest: receipt.receipt_digest(),
      input_digest: receipt.request().input().digest(),
      frozen_input_digest: receipt.request().request().frozen_input_digest(),
      question_set: Box::new(receipt.question_set().clone()),
      probability_semantics: Box::new(receipt.probability_semantics().clone()),
      provider: Box::new(receipt.provider().clone()),
      adapter: Box::new(receipt.adapter().clone()),
      model: Box::new(receipt.model().clone()),
      policy: Box::new(receipt.policy().clone()),
      purpose: Some(receipt.request().request().purpose()),
      mode: Some(receipt.mode()),
      fallback: Some(receipt.fallback()),
      answers: receipt
        .result()
        .map_or_else(Vec::new, |result| result.answers().to_vec()),
      state: receipt.state(),
      consumption: receipt.consumption().clone(),
    })
  }

  /// Projects an exact candidate verified by the trusted capture/publication owner.
  #[must_use]
  pub fn candidate(subject: CandidateSubject) -> Self {
    Self(DiscoveryFact::Candidate { subject })
  }
  /// Projects authoritative links and typed policy reasons, excluding private findings.
  #[must_use]
  pub fn decision(decision: &Decision) -> Self {
    Self(DiscoveryFact::Decision {
      subject: decision.subject().clone(),
      id: decision.id(),
      outcome: decision.outcome(),
      explanation: Some(Box::new(DecisionExplanation {
        plan_id: decision.plan_id(),
        assessment_ids: decision.assessment_ids().to_vec(),
        signal_receipt_ids: decision.signal_receipt_ids().to_vec(),
        reasons: decision.reasons().to_vec(),
        input_digest: decision.input_digest(),
        policy_version: decision.policy_version(),
        policy_digest: decision.policy_digest(),
      })),
    })
  }
  /// Projects a verified delivery attempt, omitting connector targets and private links.
  #[must_use]
  pub fn delivery(delivery: &DeliveryAttempt) -> Self {
    Self(DiscoveryFact::Delivery {
      subject: delivery.subject().clone(),
      id: delivery.id(),
      decision_id: delivery.decision_id(),
      state: delivery.state(),
    })
  }
  /// Projects escalation identity without human prose or private diagnostic details.
  #[must_use]
  pub fn escalation(escalation: &Escalation) -> Self {
    Self(DiscoveryFact::Escalation {
      subject: escalation.subject().clone(),
      id: escalation.id(),
    })
  }
  /// Returns an explicit candidate publication. A late Decision or delivery receipt
  /// describes its own subject and must not replace a newer published candidate.
  #[must_use]
  pub fn published_candidate(&self) -> Option<&CandidateSubject> {
    match &self.0 {
      DiscoveryFact::Candidate { subject } | DiscoveryFact::ChangeSet { subject, .. } => Some(subject),
      _ => None,
    }
  }
  /// An explicit owner publication takes precedence over a branch's frozen Context.
  /// Otherwise Context precedes receipt subjects, which may describe older work.
  #[must_use]
  pub fn select_current_candidate<'a>(&'a self, context: Option<&'a CandidateSubject>) -> Option<&'a CandidateSubject> {
    self
      .published_candidate()
      .or(context)
      .or_else(|| self.candidate_subject())
  }
  /// Returns repository provenance when present; Project/input facts have no VCS base.
  #[must_use]
  pub fn exact_subject(&self) -> Option<&ExactSubject> {
    match &self.0 {
      DiscoveryFact::Escalation { subject, .. }
      | DiscoveryFact::Reporting { subject, .. }
      | DiscoveryFact::DecisionSignal { subject, .. } => subject.repository().ok(),
      DiscoveryFact::Candidate { subject }
      | DiscoveryFact::ChangeSet { subject, .. }
      | DiscoveryFact::Evidence { subject, .. }
      | DiscoveryFact::ReviewPlan { subject, .. }
      | DiscoveryFact::Assessment { subject, .. }
      | DiscoveryFact::Promotion { subject, .. }
      | DiscoveryFact::Decision { subject, .. }
      | DiscoveryFact::Delivery { subject, .. } => Some(subject.exact()),
    }
  }
  /// Returns the candidate when the verified domain result has one.
  #[must_use]
  pub fn candidate_subject(&self) -> Option<&CandidateSubject> {
    match &self.0 {
      DiscoveryFact::Candidate { subject }
      | DiscoveryFact::ChangeSet { subject, .. }
      | DiscoveryFact::Evidence { subject, .. }
      | DiscoveryFact::ReviewPlan { subject, .. }
      | DiscoveryFact::Assessment { subject, .. }
      | DiscoveryFact::Promotion { subject, .. }
      | DiscoveryFact::Decision { subject, .. }
      | DiscoveryFact::Delivery { subject, .. } => Some(subject),
      DiscoveryFact::Escalation { .. } | DiscoveryFact::DecisionSignal { .. } | DiscoveryFact::Reporting { .. } => None,
    }
  }
  /// Returns a disposition independently of advisory Decision Signals.
  #[must_use]
  pub fn decision_summary(&self) -> Option<(DecisionId, DecisionOutcome)> {
    match self.0 {
      DiscoveryFact::Decision { id, outcome, .. } => Some((id, outcome)),
      _ => None,
    }
  }
  /// Returns the exact delivery state and its authoritative Decision correlation.
  #[must_use]
  pub fn delivery_summary(&self) -> Option<(DeliveryAttemptId, DecisionId, DeliveryState)> {
    match self.0 {
      DiscoveryFact::Delivery {
        id, decision_id, state, ..
      } => Some((id, decision_id, state)),
      _ => None,
    }
  }
  /// Returns the escalation identity without its reason text.
  #[must_use]
  pub fn escalation_id(&self) -> Option<EscalationId> {
    match self.0 {
      DiscoveryFact::Escalation { id, .. } => Some(id),
      _ => None,
    }
  }
}
