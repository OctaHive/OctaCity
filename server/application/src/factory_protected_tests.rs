//! Protected test capture over frozen inputs and actual ordinary Build outputs.
//! This technical capability selects no logical node or Flow transition.
use crate::factory_node_build::{
  ObservedBuild, artifact_reference, document_producer, selected_document, validate_execution_facts, validate_outputs,
};
use crate::{ApplicationError, FactoryNodeBuildOutputDocument, FactoryNodeBuildOutputs};
use octacity_server_artifacts::{ArtifactRecord, ArtifactState, ArtifactType};
use octacity_server_domain::Timestamp;
use octacity_server_factory::{
  EvidenceOutputKind, EvidenceRequirement, FactoryContextReference, FactoryDigest, FlowBuildExecution, FlowBuildIntent,
  FlowNodeInput, FlowNodeRecord, NodeAttemptId, ProtectedTestBundle, ProtectedTestFile, ProtectedTestPlan,
};
use serde::{Deserialize, Serialize};

/// Private inventory emitted by the installed capture tool, never a public handoff.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryProtectedTestInventory {
  /// Complete server-frozen coverage, subject, limits and retention identity.
  pub plan_digest: FactoryDigest,
  /// Exact ordinary execution input identity.
  pub input_digest: FactoryDigest,
  /// Exact producing attempt, preventing cross-run capture replay.
  pub node_attempt_id: NodeAttemptId,
  /// Individually retained protected source Artifacts.
  pub files: Vec<ProtectedTestFile>,
}

/// Expands one already-verified bundle selected explicitly by frozen Context.
/// The trusted compiler obtains these records from the Artifact store. Only its
/// signed Job receives the exact per-file grants and private read-only mounts;
/// a control dependency or a public handoff cannot supply this selection.
pub fn factory_protected_test_materialization(
  input: &FlowNodeInput,
  bundle: &ProtectedTestBundle,
  manifest: &ArtifactRecord,
  files: &[ArtifactRecord],
  at: Timestamp,
) -> Result<Vec<octacity_protocol::ProtectedInputV3>, ApplicationError> {
  let invalid = ApplicationError::invalid;
  if bundle.plan().subject().exact() != input.subject()
    || at >= bundle.retained_until()
    || input.context().is_none_or(|context| {
      !context.entries().iter().any(
        |entry| matches!(entry.source(),FactoryContextReference::Artifact(artifact) if artifact == bundle.artifact()),
      )
    })
    || files.len() != bundle.files().len()
  {
    return Err(invalid());
  }
  let mut selected = vec![(
    bundle.artifact(),
    manifest,
    "/octacity/protected/test-inventory.json".to_owned(),
  )];
  for file in bundle.files() {
    let record = files
      .iter()
      .find(|row| row.identity().artifact_id == file.artifact.artifact_id())
      .ok_or_else(invalid)?;
    if record.identity().job_id != bundle.producer().job_id()
      || record.identity().lease_id != manifest.identity().lease_id
    {
      return Err(invalid());
    }
    selected.push((
      &file.artifact,
      record,
      format!("/octacity/protected/tests/{}", file.path.as_str()),
    ));
  }
  let mut inputs = Vec::with_capacity(selected.len());
  for (reference, record, destination) in selected {
    let identity = record.identity();
    if record.state() != ArtifactState::Published
      || record.published_at().is_none_or(|published| published > at)
      || identity
        .retention
        .delete_after()
        .is_some_and(|deadline| deadline < bundle.retained_until() || deadline <= at)
      || identity.artifact_id != reference.artifact_id()
      || identity.digest.as_bytes() != reference.content_digest().as_bytes()
      || identity.size_bytes != reference.encoded_size()
      || identity.artifact_type != ArtifactType::Artifact
      || identity.build_id != bundle.producer().build_id()
      || identity.attempt_id != bundle.producer().attempt_id()
      || identity.job_id != bundle.producer().job_id()
    {
      return Err(invalid());
    }
    inputs.push(octacity_protocol::ProtectedInputV3 {
      artifact_id: identity.artifact_id.to_string(),
      size_bytes: identity.size_bytes,
      sha256: identity.digest.to_string(),
      media_type: identity.media_type.as_str().to_owned(),
      destination,
    });
  }
  inputs.sort_by(|a, b| a.artifact_id.cmp(&b.artifact_id));
  octacity_protocol::ProtectedInputManifestV3 { inputs: inputs.clone() }
    .validate()
    .map_err(|_| invalid())?;
  Ok(inputs)
}

/// Verifies coverage and every source byte before accepting protected metadata.
/// `plan` and `requirement` come from the trusted frozen capability configuration,
/// never from model-authored metadata. Ordinary output verification also fences
/// Job provenance, aggregate output budgets, publication and current retention.
pub fn verify_factory_protected_test_bundle(
  intent: &FlowBuildIntent,
  execution: &FlowBuildExecution,
  outputs: &FactoryNodeBuildOutputs,
  plan: &ProtectedTestPlan,
  requirement: &EvidenceRequirement,
  at: Timestamp,
) -> Result<ProtectedTestBundle, ApplicationError> {
  let invalid = ApplicationError::invalid;
  if intent
    .input()
    .context()
    .is_none_or(|context| context.subject() != plan.subject())
    || requirement.output_kind() != EvidenceOutputKind::Artifact
    || requirement.plugin() != &intent.profile().plugin
    || FlowBuildExecution::new(
      intent,
      execution.build_id(),
      execution.attempt_id(),
      execution.jobs().to_vec(),
    )
    .map_err(|_| invalid())?
      != *execution
  {
    return Err(invalid());
  }
  validate_execution_facts(intent, outputs)?;
  validate_outputs(
    outputs,
    intent.budget(),
    intent.node(),
    &ObservedBuild {
      build_id: execution.build_id(),
      attempt_id: execution.attempt_id(),
      jobs: execution.jobs().to_vec(),
    },
    at,
  )?;
  let manifest = selected_document(
    &outputs.documents,
    requirement.kind(),
    requirement.output_kind(),
    requirement.schema(),
    requirement.tool(),
    requirement.plugin(),
  )?;
  let inventory: FactoryProtectedTestInventory = serde_json::from_slice(&manifest.bytes).map_err(|_| invalid())?;
  if inventory.plan_digest != plan.digest().map_err(|_| invalid())?
    || inventory.input_digest != intent.input().digest().map_err(|_| invalid())?
    || inventory.node_attempt_id != intent.node().id()
  {
    return Err(invalid());
  }
  let mut retention = manifest
    .record
    .identity()
    .retention
    .delete_after()
    .unwrap_or(plan.retain_until());
  for file in &inventory.files {
    let document = outputs
      .documents
      .iter()
      .find(|doc| doc.record.identity().artifact_id == file.artifact.artifact_id())
      .ok_or_else(invalid)?;
    let identity = document.record.identity();
    if artifact_reference(document)? != file.artifact
      || identity.artifact_type != ArtifactType::Artifact
      || document.tool != *requirement.tool()
      || document.plugin != *requirement.plugin()
      || identity.job_id != manifest.record.identity().job_id
      || identity.lease_id != manifest.record.identity().lease_id
    {
      return Err(invalid());
    }
    retention = retention.min(identity.retention.delete_after().unwrap_or(plan.retain_until()));
  }
  if retention < plan.retain_until() {
    return Err(invalid());
  }
  ProtectedTestBundle::new(
    plan.clone(),
    artifact_reference(manifest)?,
    inventory.files,
    document_producer(manifest),
    outputs.verified_at,
    plan.retain_until(),
  )
  .map_err(|_| invalid())
}

/// Reconstructs accepted private metadata from the common journal and fetched bytes.
/// No process cache or current-stage projection participates. File bytes are
/// rechecked by signed protected-input staging before a validation Job starts.
pub fn restore_factory_protected_test_bundle(
  record: &FlowNodeRecord,
  intent: &FlowBuildIntent,
  execution: &FlowBuildExecution,
  manifest: &FactoryNodeBuildOutputDocument,
  plan: &ProtectedTestPlan,
  requirement: &EvidenceRequirement,
  at: Timestamp,
) -> Result<ProtectedTestBundle, ApplicationError> {
  let invalid = ApplicationError::invalid;
  let provenance = record.build_provenance().ok_or_else(invalid)?;
  let proof = provenance
    .outputs()
    .iter()
    .find(|output| output.name == *requirement.kind())
    .ok_or_else(invalid)?;
  let identity = manifest.record.identity();
  if record.input() != intent.input()
    || record.node_attempt_id() != intent.node().id()
    || provenance.profile() != intent.profile()
    || FlowBuildExecution::new(
      intent,
      execution.build_id(),
      execution.attempt_id(),
      execution.jobs().to_vec(),
    )
    .map_err(|_| invalid())?
      != *execution
    || proof.artifact != artifact_reference(manifest)?
    || proof.schema != *requirement.schema()
    || proof.output_kind != EvidenceOutputKind::Artifact
    || proof.producer != document_producer(manifest)
    || manifest.tool != *requirement.tool()
    || manifest.plugin != *requirement.plugin()
    || identity.build_id != execution.build_id()
    || identity.attempt_id != execution.attempt_id()
    || !execution.jobs().contains(&identity.job_id)
    || manifest.record.state() != ArtifactState::Published
    || manifest
      .record
      .published_at()
      .is_none_or(|published| published > proof.verified_at)
    || identity
      .retention
      .delete_after()
      .is_some_and(|deadline| deadline < plan.retain_until() || deadline <= at)
    || identity.size_bytes != manifest.bytes.len() as u64
    || identity.digest.as_bytes() != FactoryDigest::content_sha256(&manifest.bytes).as_bytes()
    || proof.verified_at > at
    || at >= plan.retain_until()
    || intent
      .input()
      .context()
      .is_none_or(|context| context.subject() != plan.subject())
  {
    return Err(invalid());
  }
  let inventory: FactoryProtectedTestInventory = serde_json::from_slice(&manifest.bytes).map_err(|_| invalid())?;
  if inventory.input_digest != intent.input().digest().map_err(|_| invalid())?
    || inventory.node_attempt_id != intent.node().id()
    || inventory.plan_digest != plan.digest().map_err(|_| invalid())?
  {
    return Err(invalid());
  }
  let bundle = ProtectedTestBundle::new(
    plan.clone(),
    proof.artifact.clone(),
    inventory.files,
    proof.producer.clone(),
    proof.verified_at,
    plan.retain_until(),
  )
  .map_err(|_| invalid())?;
  if record.payload().value() != &serde_json::to_value(bundle.public_handoff()).map_err(|_| invalid())? {
    return Err(invalid());
  }
  Ok(bundle)
}
