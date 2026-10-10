//! Immutable research observations and code-owned successor policy over ordinary Flows.

mod evidence;
mod execution;
mod handoff;
mod input;
mod policy;
mod reproduction;
mod result;
#[cfg(test)]
mod tests;
mod vocabulary;

pub use evidence::{AcceptedResearchEvidence, ResearchEvidenceFact, ResearchEvidenceKind, ResearchEvidenceRecord};
pub use execution::ResearchBuildIntent;
pub use handoff::{ResearchAcceptance, ResearchStageHandoff};
pub use input::{ResearchDetails, ResearchInput, ResearchSourceReference};
pub use policy::{
  ResearchAttemptDisposition, ResearchDecision, ResearchPolicy, ResearchPolicySettings, ResearchReason,
};
pub use reproduction::{DefectReproductionCheck, DefectReproductionObservation, DefectReproductionReport};
pub use result::{
  DefectResearchResult, FeatureResearchProposal, FeatureResearchResult, ResearchObservations, ResearchProvenance,
  ResearchResult,
};
pub use vocabulary::{
  DefectResearchOutcome, MAX_RESEARCH_ATTEMPTS, MAX_RESEARCH_CONTRACT_BYTES, MAX_RESEARCH_ITEMS,
  MAX_RESEARCH_PROPOSAL_BYTES, RESEARCH_CONTRACT_VERSION, ResearchOutcome, ResearchRoute, ResearchSchema,
};

use crate::{FactoryDigest, FactoryError};
use serde::Serialize;

fn invalid(field: &'static str) -> FactoryError {
  FactoryError::InvalidConfiguration { field }
}

fn digest(domain: &str, value: &impl Serialize) -> Result<FactoryDigest, FactoryError> {
  let bytes = serde_json::to_vec(value).map_err(|_| invalid("research serialization"))?;
  Ok(FactoryDigest::sha256(domain, &[&bytes]))
}

fn canonicalize<T: Ord>(values: &mut [T], minimum: usize) -> Result<(), FactoryError> {
  if values.len() < minimum || values.len() > MAX_RESEARCH_ITEMS {
    return Err(invalid("research collection bound"));
  }
  values.sort();
  if values.windows(2).any(|pair| pair[0] == pair[1]) {
    return Err(invalid("duplicate research item"));
  }
  Ok(())
}
