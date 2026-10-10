use super::*;
use crate::{
  AdmittedFlow, BudgetUsage, DeterministicGateOutcome, ExactSubject, FactoryArtifactReference, FactoryDigest,
  FactoryError, WorkEnvelope,
};
use serde::{Deserialize, Deserializer, Serialize};

fn restore_evidence<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<AcceptedTriageEvidence>, D::Error> {
  #[derive(Deserialize)]
  #[serde(deny_unknown_fields)]
  struct Retained {
    subject: ExactSubject,
    input_digest: FactoryDigest,
    fact: TriageEvidenceFact,
    evidence: FactoryArtifactReference,
  }
  let rows = Vec::<Retained>::deserialize(deserializer)?;
  if rows.len() > MAX_TRIAGE_OBSERVATIONS + 1 {
    return Err(serde::de::Error::custom("triage evidence bound"));
  }
  rows
    .into_iter()
    .map(|row| {
      AcceptedTriageEvidence::new(
        row.subject,
        row.input_digest,
        row.fact,
        row.evidence,
        DeterministicGateOutcome::Passed,
      )
      .map_err(serde::de::Error::custom)
    })
    .collect()
}

/// Frozen accepted eligibility observations, trusted evidence, and code-owned decision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EligibilityReceipt {
  /// Original exact phase inputs, frozen before execution.
  pub input: EligibilityInput,
  /// Retained non-authoritative observation payload.
  pub result: EligibilityResult,
  /// Evidence accepted by the trusted validation port, never by the phase provider.
  #[serde(deserialize_with = "restore_evidence")]
  pub evidence: Vec<AcceptedTriageEvidence>,
  /// Aggregate usage at the deterministic eligibility decision.
  pub usage: BudgetUsage,
  /// Deterministically selected eligibility outcome.
  pub decision: EligibilityDecision,
}

/// Frozen classification observations and code-owned routing decision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClassificationReceipt {
  /// Complete accepted eligibility provenance.
  pub eligibility: EligibilityReceipt,
  /// Retained second-phase observations.
  pub result: TriageResult,
  /// Separately accepted trusted evidence.
  #[serde(deserialize_with = "restore_evidence")]
  pub evidence: Vec<AcceptedTriageEvidence>,
  /// Aggregate usage at the routing decision.
  pub usage: BudgetUsage,
  /// Deterministic finite route.
  pub decision: TriageDecision,
}

/// Append-only typed intake records retained alongside generic Flow history.
///
/// Only trusted persistence restores these records. Provider ports return the
/// observation contracts and cannot submit a receipt or an accepted decision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "phase", content = "record", deny_unknown_fields)]
pub enum TriageJournalRecord {
  /// Authorized discovery frozen before invoking eligibility.
  Prepared(Box<EligibilityInput>),
  /// First-phase result with its deterministic decision.
  Eligibility(Box<EligibilityReceipt>),
  /// Second-phase result with its deterministic route.
  Classification(Box<ClassificationReceipt>),
}

impl TriageJournalRecord {
  /// Returns the stable append-only intake phase ordinal.
  #[must_use]
  pub const fn phase(&self) -> u8 {
    match self {
      Self::Prepared(_) => 0,
      Self::Eligibility(_) => 1,
      Self::Classification(_) => 2,
    }
  }
  /// Returns the immutable record identity, including complete evidence and provenance.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    super::digest("octacity.factory.triage-journal.v1", self)
  }
  /// Returns the original frozen input from any phase.
  #[must_use]
  pub const fn input(&self) -> &EligibilityInput {
    match self {
      Self::Prepared(input) => input,
      Self::Eligibility(row) => &row.input,
      Self::Classification(row) => &row.eligibility.input,
    }
  }
  /// Recomputes every retained decision under the admitted immutable policy.
  pub fn validate(&self, work: &WorkEnvelope, admitted: &AdmittedFlow) -> Result<(), FactoryError> {
    let settings = admitted.triage().ok_or_else(|| invalid("triage admission"))?;
    let policy = TriagePolicy::for_flow(
      work.configuration().clone(),
      &admitted.validated()?,
      settings.definition,
      settings.policy.clone(),
    )?;
    if self.input()
      != &EligibilityInput::new(
        work,
        settings.project_goals.clone(),
        self.input().duplicate_candidates().to_vec(),
      )?
    {
      return Err(invalid("triage frozen Work"));
    }
    let eligibility: &EligibilityReceipt = match self {
      Self::Prepared(_) => return Ok(()),
      Self::Eligibility(row) => row,
      Self::Classification(row) => &row.eligibility,
    };
    if !settings.eligibility.matches(eligibility.result.provenance()) {
      return Err(invalid("eligibility execution profile"));
    }
    if eligibility.evidence.len() > MAX_TRIAGE_OBSERVATIONS + 1
      || policy.eligibility(
        &eligibility.input,
        &eligibility.result,
        &eligibility.evidence,
        eligibility.usage,
      )? != eligibility.decision
    {
      return Err(invalid("triage eligibility decision"));
    }
    if let Self::Classification(row) = self {
      if !settings.classification.matches(row.result.provenance()) {
        return Err(invalid("classification execution profile"));
      }
      let input = policy.classification_input(
        row.eligibility.input.clone(),
        row.eligibility.result.clone(),
        &row.eligibility.evidence,
        row.eligibility.usage,
      )?;
      if row.evidence.len() > MAX_TRIAGE_OBSERVATIONS + 1
        || policy.route(&input, &row.result, &row.eligibility.evidence, &row.evidence, row.usage)? != row.decision
      {
        return Err(invalid("triage routing decision"));
      }
    }
    Ok(())
  }
}

impl TriageJournalRecord {
  /// Returns every immutable Artifact referenced by this phase's new observations.
  #[must_use]
  pub fn artifacts(&self) -> Vec<&FactoryArtifactReference> {
    match self {
      Self::Prepared(input) => std::iter::once(input.project_goals())
        .chain(input.duplicate_candidates().iter().map(|row| &row.evidence))
        .collect(),
      Self::Eligibility(row) => std::iter::once(&row.result.provenance().result)
        .chain(std::iter::once(row.result.project_fit().evidence()))
        .chain(row.result.duplicates().iter().map(TriageObservation::evidence))
        .chain(row.evidence.iter().map(|row| &row.evidence))
        .collect(),
      Self::Classification(row) => {
        let c = row.result.classification();
        let mut artifacts = vec![
          &row.result.provenance().result,
          c.work_kind.evidence(),
          c.component.evidence(),
          c.severity.evidence(),
          c.size.evidence(),
          c.risk.evidence(),
          c.reproducibility.evidence(),
          c.recommended_route.evidence(),
        ];
        artifacts.extend(c.dependencies.iter().map(TriageObservation::evidence));
        artifacts.extend(row.evidence.iter().map(|row| &row.evidence));
        artifacts
      }
    }
  }
}
