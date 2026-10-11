//! Trusted deterministic validation over two explicitly selected immutable inputs.
use crate::{
  ApplicationError, FactoryCandidateMaterialization, factory_node_candidate_materialization,
  factory_protected_test_materialization,
};
use octacity_protocol::{ChangeSetMaterializationV3, ProtectedInputManifestV3, ProtectedInputV3};
use octacity_server_artifacts::ArtifactRecord;
use octacity_server_domain::Timestamp;
use octacity_server_factory::{
  CandidateSubject, ChangeSet, ChangeSetId, DeterministicGateOutcome, FactoryArtifactReference, FactoryDigest,
  FactoryKey, FlowBuildIntent, FlowNodeInput, FlowNodeKind, MountMode, ProtectedTestBundle,
};
use serde::{Deserialize, Serialize};

/// Authoritative inputs selected by a trusted ordinary-Job compiler.
pub struct FactoryProtectedValidationRequest<'a> {
  /// Frozen command execution, including its exact selected Context.
  pub intent: &'a FlowBuildIntent,
  /// Previously verified implementation candidate.
  pub candidate: &'a ChangeSet,
  /// Published candidate Git bundle record.
  pub candidate_bundle: &'a ArtifactRecord,
  /// Published candidate capture-manifest record.
  pub candidate_manifest: &'a ArtifactRecord,
  /// Previously verified protected test bundle.
  pub tests: &'a ProtectedTestBundle,
  /// Published private test inventory record.
  pub test_manifest: &'a ArtifactRecord,
  /// Exact protected source records, read from the Artifact store.
  pub test_files: &'a [ArtifactRecord],
  /// Trusted current time used to enforce original retention.
  pub observed_at: Timestamp,
}
/// Validated signed-ready materialization; private fields prevent input replacement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryProtectedValidationMaterialization {
  input: FlowNodeInput,
  candidate: ChangeSet,
  capture: FactoryCandidateMaterialization,
  tests: ProtectedTestBundle,
  inputs: Vec<ProtectedInputV3>,
}
/// One bounded class outcome; diagnostic bodies and expected values stay private.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryProtectedValidationClassResult {
  /// Arbitrary class declared by the pinned authoring plan.
  pub class: FactoryKey,
  /// Independently verified deterministic outcome.
  pub outcome: DeterministicGateOutcome,
}
/// Closed aggregate report emitted by the selected trusted validation command.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryProtectedValidationReport {
  /// Exact accepted implementation ChangeSet.
  pub changeset_id: ChangeSetId,
  /// Exact implementation revision and content identity.
  pub candidate: CandidateSubject,
  /// Exact private test inventory identity.
  pub tests: FactoryArtifactReference,
  /// Digest of both complete materialized inputs, including every test source.
  pub input_digest: FactoryDigest,
  /// Canonical complete declared-class outcomes, with no unbounded diagnostics.
  pub classes: Vec<FactoryProtectedValidationClassResult>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidationFacts {
  accepted: bool,
  input_digest: FactoryDigest,
  report_digest: FactoryDigest,
  candidate_revision: octacity_server_domain::ImmutableRevision,
  candidate_digest: FactoryDigest,
  protected_inventory: FactoryArtifactReference,
  covered_classes: Vec<FactoryKey>,
  fresh_until: Timestamp,
}
/// Combines inputs only for a command node with explicit candidate and test selections.
/// No model invocation or predecessor pointer can implicitly acquire these files.
pub fn factory_protected_validation_materialization(
  request: FactoryProtectedValidationRequest<'_>,
) -> Result<FactoryProtectedValidationMaterialization, ApplicationError> {
  let invalid = ApplicationError::invalid;
  if request.intent.node().node_kind() != FlowNodeKind::BuildCommand
    || request.intent.permissions().mounts().any(|mount| {
      mount.mode() != MountMode::ReadOnly
        && [
          octacity_protocol::FACTORY_SOURCE_ROOT,
          octacity_protocol::PROTECTED_INPUT_ROOT,
        ]
        .iter()
        .any(|protected| {
          let root = mount.root().as_str();
          root == "/"
            || root == *protected
            || protected.strip_prefix(root).is_some_and(|rest| rest.starts_with('/'))
            || root.strip_prefix(protected).is_some_and(|rest| rest.starts_with('/'))
        })
    })
  {
    return Err(invalid());
  }
  let capture = factory_node_candidate_materialization(
    request.intent.input(),
    request.candidate,
    request.candidate_bundle,
    request.candidate_manifest,
    request.observed_at,
  )
  .map_err(|_| invalid())?;
  let mut inputs = factory_protected_test_materialization(
    request.intent.input(),
    request.tests,
    request.test_manifest,
    request.test_files,
    request.observed_at,
  )?;
  inputs.extend(capture.wire_inputs().1);
  inputs.sort_by(|a, b| a.artifact_id.cmp(&b.artifact_id));
  ProtectedInputManifestV3 { inputs: inputs.clone() }
    .validate()
    .map_err(|_| invalid())?;
  let materialized = FactoryProtectedValidationMaterialization {
    input: request.intent.input().clone(),
    candidate: request.candidate.clone(),
    capture,
    tests: request.tests.clone(),
    inputs,
  };
  materialized.wire_inputs(&[])?;
  Ok(materialized)
}
impl FactoryProtectedValidationMaterialization {
  /// Assembles candidate/test inputs with the compiler's actual managed documents.
  /// All non-source inputs must fit the pinned plan's reserved slots before signing;
  /// the complete manifest is also checked for count, byte and destination collisions.
  pub fn wire_inputs(
    &self,
    managed_inputs: &[ProtectedInputV3],
  ) -> Result<(ChangeSetMaterializationV3, Vec<ProtectedInputV3>), ApplicationError> {
    let mut inputs = self.inputs.clone();
    inputs.extend_from_slice(managed_inputs);
    if inputs.len().saturating_sub(self.tests.files().len()) > usize::from(self.tests.plan().reserved_inputs()) {
      return Err(ApplicationError::invalid());
    }
    inputs.sort_by(|a, b| a.artifact_id.cmp(&b.artifact_id));
    ProtectedInputManifestV3 { inputs: inputs.clone() }
      .validate()
      .map_err(|_| ApplicationError::invalid())?;
    Ok((self.capture.wire_inputs().0, inputs))
  }
  /// Binds the candidate, complete immutable test plan and every downloaded file.
  /// Excludes transport capabilities, Job identities and observation clocks.
  pub fn input_digest(&self) -> Result<FactoryDigest, ApplicationError> {
    let value = serde_json::json!({"candidate":self.candidate,"tests":self.tests.digest().map_err(|_|ApplicationError::invalid())?,"inputs":self.inputs});
    Ok(FactoryDigest::content_sha256(
      &serde_json::to_vec(&value).map_err(|_| ApplicationError::invalid())?,
    ))
  }
  /// Validates exact subject and class completeness before accepting any outcome.
  pub fn verify_report(
    &self,
    report: &FactoryProtectedValidationReport,
  ) -> Result<DeterministicGateOutcome, ApplicationError> {
    if report.changeset_id != self.candidate.id()
      || report.candidate != *self.candidate.subject()
      || report.tests != *self.tests.artifact()
      || report.input_digest != self.input_digest()?
      || !report
        .classes
        .iter()
        .map(|item| &item.class)
        .eq(self.tests.plan().classes())
    {
      return Err(ApplicationError::invalid());
    }
    Ok(
      if report
        .classes
        .iter()
        .any(|item| item.outcome == DeterministicGateOutcome::Failed)
      {
        DeterministicGateOutcome::Failed
      } else if report
        .classes
        .iter()
        .any(|item| item.outcome == DeterministicGateOutcome::Indeterminate)
      {
        DeterministicGateOutcome::Indeterminate
      } else {
        DeterministicGateOutcome::Passed
      },
    )
  }
  pub(super) fn verify_facts(
    &self,
    report: &FactoryProtectedValidationReport,
    report_digest: FactoryDigest,
    value: &serde_json::Value,
    fresh_until: Timestamp,
    at: Timestamp,
  ) -> Result<Timestamp, ApplicationError> {
    let facts: ValidationFacts = serde_json::from_value(value.clone()).map_err(|_| ApplicationError::invalid())?;
    if !facts.accepted
      || facts.input_digest != self.input.digest().map_err(|_| ApplicationError::invalid())?
      || facts.report_digest != report_digest
      || facts.candidate_revision != *report.candidate.candidate_revision()
      || facts.candidate_digest != report.candidate.changeset_digest()
      || facts.protected_inventory != report.tests
      || !facts
        .covered_classes
        .iter()
        .eq(report.classes.iter().map(|item| &item.class))
      || facts.fresh_until <= at
      || facts.fresh_until > fresh_until
      || facts.fresh_until > self.retained_until()
    {
      return Err(ApplicationError::invalid());
    }
    Ok(facts.fresh_until)
  }
  pub(super) fn input(&self) -> &FlowNodeInput {
    &self.input
  }
  pub(super) fn candidate(&self) -> &ChangeSet {
    &self.candidate
  }
  pub(super) fn retained_until(&self) -> Timestamp {
    self
      .tests
      .retained_until()
      .min(
        self
          .capture
          .bundle
          .retention
          .delete_after()
          .unwrap_or(self.tests.retained_until()),
      )
      .min(
        self
          .capture
          .manifest
          .retention
          .delete_after()
          .unwrap_or(self.tests.retained_until()),
      )
  }
  pub(super) fn artifact_inputs(&self) -> Result<Vec<FactoryArtifactReference>, ApplicationError> {
    self
      .inputs
      .iter()
      .map(|input| {
        FactoryArtifactReference::new(
          input.artifact_id.parse().map_err(|_| ApplicationError::invalid())?,
          input.sha256.parse().map_err(|_| ApplicationError::invalid())?,
          input.size_bytes,
        )
        .map_err(|_| ApplicationError::invalid())
      })
      .collect()
  }
}

/// Publishes deterministic evidence only from an exact ordinary Build completion.
/// The selected closed report is checked against both complete materializations;
/// the Evidence Manifest retains every immutable Artifact input identity. The exact
/// qualification requirement is revalidated from retained facts and limits freshness.
pub fn construct_factory_protected_validation_evidence_manifest(
  id: octacity_server_factory::EvidenceManifestId,
  materialized: &FactoryProtectedValidationMaterialization,
  completed: &crate::FactoryNodeBuildCompletion,
  document: &crate::FactoryNodeBuildOutputDocument,
  requirement: &octacity_server_factory::EvidenceRequirement,
  facts_requirement: &octacity_server_factory::EvidenceRequirement,
  at: Timestamp,
) -> Result<octacity_server_factory::EvidenceManifest, ApplicationError> {
  let invalid = ApplicationError::invalid;
  if completed.record.input() != materialized.input()
    || requirement.output_kind() != octacity_server_factory::EvidenceOutputKind::Report
    || document.schema != *requirement.schema()
    || document.tool != *requirement.tool()
    || document.plugin != *requirement.plugin()
    || document.record.identity().logical_name.as_str() != requirement.kind().as_str()
  {
    return Err(invalid());
  }
  let report: FactoryProtectedValidationReport = serde_json::from_slice(&document.bytes).map_err(|_| invalid())?;
  let outcome = materialized.verify_report(&report)?;
  let proof = completed
    .record
    .build_provenance()
    .and_then(|proof| proof.outputs().iter().find(|output| output.name == *requirement.kind()))
    .ok_or_else(invalid)?;
  let facts = completed
    .record
    .build_provenance()
    .and_then(|provenance| {
      provenance
        .outputs()
        .iter()
        .find(|output| output.name == *facts_requirement.kind())
    })
    .ok_or_else(invalid)?;
  if facts_requirement.output_kind() != octacity_server_factory::EvidenceOutputKind::Report
    || facts.output_kind != facts_requirement.output_kind()
    || facts.schema != *facts_requirement.schema()
    || facts.producer.tool() != facts_requirement.tool()
    || facts.producer.plugin() != facts_requirement.plugin()
    || facts.verified_at > at
  {
    return Err(invalid());
  }
  let facts_value = facts.data.as_ref().ok_or_else(invalid)?.payload().value();
  let fresh_until = materialized.verify_facts(
    &report,
    FactoryDigest::content_sha256(&document.bytes),
    facts_value,
    proof.fresh_until.min(facts.fresh_until),
    at,
  )?;
  let attestation = crate::FactoryEvidenceAttestation::new(
    materialized.candidate().subject().clone(),
    requirement.kind().clone(),
    requirement.schema().clone(),
    requirement.tool().clone(),
    requirement.plugin().clone(),
    outcome,
    fresh_until,
  );
  let verified = crate::VerifiedFactoryEvidence::new(document.record.clone(), &document.bytes, attestation)
    .map_err(|_| invalid())?;
  crate::construct_factory_evidence_manifest(
    id,
    materialized.candidate(),
    completed,
    vec![requirement.clone()],
    vec![verified],
    at,
  )
  .map_err(|_| invalid())?
  .with_inputs(materialized.artifact_inputs()?)
  .map_err(|_| invalid())
}
