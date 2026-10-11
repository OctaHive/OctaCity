//! Verification qualification over common Build results and retained Evidence Manifests.
use crate::{
  FactoryEvidenceAttestation, FactoryEvidenceError, FactoryNodeBuildCompletion, FactoryNodeBuildOutputDocument,
  VerifiedFactoryEvidence, construct_factory_evidence_manifest,
};
use octacity_server_domain::Timestamp;
use octacity_server_factory::{
  ChangeSet, DeterministicGateOutcome, EvidenceManifest, EvidenceManifestId, EvidenceRequirement, FlowBuildIntent,
  MAX_FLOW_DATA_BYTES, VerificationCheckInput, VerificationObservation, VerificationPlan,
};
use std::collections::BTreeSet;

/// Independently qualified output; provider JSON cannot construct this proof.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedFactoryVerificationCheck {
  input: VerificationCheckInput,
  evidence: EvidenceManifest,
}
impl VerifiedFactoryVerificationCheck {
  /// Returns the exact frozen target, environment and node input used by this proof.
  #[must_use]
  pub const fn input(&self) -> &VerificationCheckInput {
    &self.input
  }
  /// Returns retained typed evidence with actual ordinary execution provenance.
  #[must_use]
  pub const fn evidence(&self) -> &EvidenceManifest {
    &self.evidence
  }
}

/// Qualifies an exact typed observation against its frozen plan and actual retained Build output.
pub fn verify_factory_verification_check(
  intent: &FlowBuildIntent,
  candidate: &ChangeSet,
  completed: &FactoryNodeBuildCompletion,
  document: &FactoryNodeBuildOutputDocument,
  at: Timestamp,
  fresh_until: Timestamp,
) -> Result<VerifiedFactoryVerificationCheck, FactoryEvidenceError> {
  let invalid = || FactoryEvidenceError::InvalidEvidence;
  let selected = intent.verification_check().ok_or_else(invalid)?;
  let plan = selected.plan();
  if plan.subject() != candidate.subject()
    || completed.record.input() != intent.input()
    || document.bytes.len() > MAX_FLOW_DATA_BYTES
    || completed
      .record
      .build_provenance()
      .is_none_or(|proof| proof.profile() != intent.profile())
  {
    return Err(invalid());
  }
  let observation: VerificationObservation = serde_json::from_slice(&document.bytes).map_err(|_| invalid())?;
  let outcome = observation.verify(selected)?;
  if &observation.check != intent.input().node()
    || serde_json::to_value(&observation).map_err(|_| invalid())? != *completed.record.payload().value()
  {
    return Err(invalid());
  }
  let provenance = completed.record.build_provenance().ok_or_else(invalid)?;
  let output = provenance
    .outputs()
    .iter()
    .find(|output| output.name == intent.profile().result_output)
    .ok_or_else(invalid)?;
  let attestation = FactoryEvidenceAttestation::new(
    candidate.subject().clone(),
    output.name.clone(),
    document.schema.clone(),
    document.tool.clone(),
    document.plugin.clone(),
    outcome,
    fresh_until,
  );
  let qualified = VerifiedFactoryEvidence::new(document.record.clone(), &document.bytes, attestation)?;
  let target = selected.target();
  let inputs = target.retained_inputs()?;
  let evidence = construct_factory_evidence_manifest(
    EvidenceManifestId::generate(),
    candidate,
    completed,
    vec![EvidenceRequirement::new(
      output.name.clone(),
      output.output_kind,
      output.schema.clone(),
      output.producer.tool().clone(),
      output.producer.plugin().clone(),
    )],
    vec![qualified],
    at,
  )?
  .with_inputs(inputs)?;
  Ok(VerifiedFactoryVerificationCheck {
    input: selected.clone(),
    evidence,
  })
}

/// Applies immutable required-check and quorum policy; summaries grant no acceptance authority.
/// Missing, stale or indeterminate required evidence cannot satisfy verification.
pub fn evaluate_factory_verification(
  plan: &VerificationPlan,
  proofs: &[VerifiedFactoryVerificationCheck],
  at: Timestamp,
) -> Result<DeterministicGateOutcome, FactoryEvidenceError> {
  let mut seen = BTreeSet::new();
  let mut passed = 0;
  let mut failed = false;
  let mut pending = false;
  let plan_digest = plan.digest()?;
  for proof in proofs {
    let check = plan
      .policy()
      .checks
      .get(proof.input.check())
      .ok_or(FactoryEvidenceError::InvalidEvidence)?;
    if proof.input.plan().digest()? != plan_digest
      || proof.evidence.subject() != plan.subject()
      || !seen.insert(proof.input.check())
    {
      return Err(FactoryEvidenceError::InvalidEvidence);
    }
    let item = proof
      .evidence
      .items()
      .first()
      .ok_or(FactoryEvidenceError::InvalidEvidence)?;
    if item.published_at() > at || at >= item.fresh_until() {
      pending |= check.required;
      continue;
    }
    match item.outcome() {
      DeterministicGateOutcome::Passed => passed += 1,
      DeterministicGateOutcome::Failed => failed |= check.required,
      DeterministicGateOutcome::Indeterminate => pending |= check.required,
    }
  }
  pending |= plan
    .policy()
    .checks
    .iter()
    .any(|(key, check)| check.required && !seen.contains(key));
  Ok(if failed {
    DeterministicGateOutcome::Failed
  } else if pending || passed < usize::from(plan.policy().required_quorum) {
    DeterministicGateOutcome::Indeterminate
  } else {
    DeterministicGateOutcome::Passed
  })
}
