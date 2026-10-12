//! Safe owner-supplied facts retained in the common fenced result journal.
//! These facts describe verified domain results; they never select a Flow transition.
use crate::*;
use serde::{Deserialize, Serialize};

/// Typed read projection authored by a trusted adapter, never by provider payload inspection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct FlowDiscoveryFacts(DiscoveryFact);
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum DiscoveryFact {
  Candidate {
    subject: CandidateSubject,
  },
  Decision {
    subject: CandidateSubject,
    id: DecisionId,
    outcome: DecisionOutcome,
  },
  Delivery {
    subject: CandidateSubject,
    id: DeliveryAttemptId,
    decision_id: DecisionId,
    state: DeliveryState,
  },
  Escalation {
    subject: ExactSubject,
    id: EscalationId,
  },
}
impl FlowDiscoveryFacts {
  /// Projects an exact candidate verified by the trusted capture/publication owner.
  #[must_use]
  pub fn candidate(subject: CandidateSubject) -> Self {
    Self(DiscoveryFact::Candidate { subject })
  }
  /// Projects the authoritative Decision, omitting assessments, reasons and findings.
  #[must_use]
  pub fn decision(decision: &Decision) -> Self {
    Self(DiscoveryFact::Decision {
      subject: decision.subject().clone(),
      id: decision.id(),
      outcome: decision.outcome(),
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
      DiscoveryFact::Candidate { subject } => Some(subject),
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
  /// Returns the exact base to which the owner must bind the retained observation.
  #[must_use]
  pub fn exact_subject(&self) -> &ExactSubject {
    match &self.0 {
      DiscoveryFact::Escalation { subject, .. } => subject,
      DiscoveryFact::Candidate { subject }
      | DiscoveryFact::Decision { subject, .. }
      | DiscoveryFact::Delivery { subject, .. } => subject.exact(),
    }
  }
  /// Returns the candidate when the verified domain result has one.
  #[must_use]
  pub fn candidate_subject(&self) -> Option<&CandidateSubject> {
    match &self.0 {
      DiscoveryFact::Candidate { subject }
      | DiscoveryFact::Decision { subject, .. }
      | DiscoveryFact::Delivery { subject, .. } => Some(subject),
      DiscoveryFact::Escalation { .. } => None,
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
