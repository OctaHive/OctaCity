//! Typed observations and deterministic two-phase triage over ordinary nested Flows.

mod composition;
mod contract;
mod journal;
mod journey;
mod policy;
mod runtime;
#[cfg(test)]
pub(crate) mod tests;

pub use composition::{TriageNode, TriageSchema, compose_triage_flow};
pub use contract::{
  ClassificationInput, DuplicateAssessment, DuplicateCandidate, DuplicateStatus, EligibilityInput, EligibilityResult,
  MAX_TRIAGE_OBSERVATIONS, PreliminaryReproducibility, ProjectFit, TRIAGE_CONTRACT_VERSION, TriageClassification,
  TriageDependency, TriageObservation, TriageProvenance, TriageResult, TriageRoute, WorkKind, WorkSize,
};
pub use journal::{ClassificationReceipt, EligibilityReceipt, TriageJournalRecord};
pub use journey::{FactoryFlowConfiguration, FactoryTriageConfiguration, FactoryTriageExecutionProfile};
pub use policy::{
  AcceptedTriageEvidence, EligibilityDecision, EligibilityOutcome, TriageDecision, TriageDisposition,
  TriageEvidenceFact, TriagePolicy, TriagePolicySettings, TriageReason,
};
pub use runtime::validate_triage_phase_observation;

use crate::{FactoryDigest, FactoryError};
use serde::Serialize;

fn invalid(field: &'static str) -> FactoryError {
  FactoryError::InvalidConfiguration { field }
}

fn digest(domain: &str, value: &impl Serialize) -> Result<FactoryDigest, FactoryError> {
  let bytes = serde_json::to_vec(value).map_err(|_| invalid("triage serialization"))?;
  Ok(FactoryDigest::sha256(domain, &[&bytes]))
}

fn canonicalize<T, K: Ord>(values: &mut [T], key: impl Fn(&T) -> K) -> Result<(), FactoryError> {
  if values.len() > MAX_TRIAGE_OBSERVATIONS {
    return Err(invalid("triage observation bound"));
  }
  values.sort_by_key(&key);
  if values.windows(2).any(|pair| key(&pair[0]) == key(&pair[1])) {
    return Err(invalid("duplicate triage observation"));
  }
  Ok(())
}
