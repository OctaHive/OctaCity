//! Trusted repository deliverables over exact capture bytes, without Flow names.
use crate::{FactoryChangeSetError, FactoryNodeBuildOutputs, verify_factory_node_change_set};
use octacity_protocol::{CHANGE_SET_MANIFEST_OUTPUT, CapturedChangeSetManifestV1};
use octacity_server_domain::Timestamp;
use octacity_server_factory::{
  CandidateDeliverableContract, CandidateFileObservation, ChangeSet, FactoryArtifactReference, FactoryRepositoryPath,
  FlowBuildExecution, FlowBuildIntent, ImmutableReference, ProtectedTestBundle,
};

/// Checked public deliverables for one exact candidate, retained independently of model results.
/// Only trusted capture verification can construct this value. Private tests are
/// inspected later inside validation; this object conveys no protected Artifact grant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedFactoryCandidateDeliverables {
  candidate: ChangeSet,
  manifest: FactoryArtifactReference,
  contract: CandidateDeliverableContract,
  files: Vec<CandidateFileObservation>,
  changes: Vec<CandidateFileObservation>,
}
impl VerifiedFactoryCandidateDeliverables {
  /// Returns the exact accepted Work, input and policy contract.
  #[must_use]
  pub const fn contract(&self) -> &CandidateDeliverableContract {
    &self.contract
  }
  /// Rejects protected source copied to any candidate path, including permitted public paths.
  /// The caller supplies only the already qualified bundle explicitly selected by validation.
  pub fn verify_protected(&self, tests: &ProtectedTestBundle) -> Result<(), crate::ApplicationError> {
    if self.contract.accepted_contract() != tests.plan().accepted_contract()
      || self.candidate.subject().exact()
        != tests
          .plan()
          .subject()
          .exact()
          .map_err(|_| crate::ApplicationError::invalid())?
    {
      return Err(crate::ApplicationError::invalid());
    }
    self
      .contract
      .verify_candidate(
        &self.changes,
        &self.files,
        &tests
          .files()
          .iter()
          .map(|file| file.artifact.content_digest())
          .collect::<Vec<_>>(),
      )
      .map_err(|_| crate::ApplicationError::invalid())
  }
  pub(super) fn verify_selection(
    &self,
    intent: &FlowBuildIntent,
    candidate: &ChangeSet,
    manifest: &octacity_server_artifacts::ArtifactRecord,
  ) -> Result<(), crate::ApplicationError> {
    let invalid = crate::ApplicationError::invalid;
    let expected = intent
      .profile()
      .capture_deliverables
      .as_ref()
      .ok_or_else(invalid)?
      .freeze(intent.input())
      .map_err(|_| invalid())?;
    if &self.candidate != candidate
      || self.manifest.artifact_id() != manifest.identity().artifact_id
      || self.manifest.content_digest().as_bytes() != manifest.identity().digest.as_bytes()
      || self.manifest.encoded_size() != manifest.identity().size_bytes
      || self.contract.accepted_contract() != expected.accepted_contract()
      || self.contract.policy() != expected.policy()
    {
      return Err(invalid());
    }
    Ok(())
  }
}
/// Qualifies mandatory public changes from actual trusted-capture output bytes.
/// Artifact/producer provenance, exact execution, capture identity and scope are
/// checked before the deliverable proof is available to a validator.
pub fn verify_factory_node_candidate_deliverables(
  intent: &FlowBuildIntent,
  execution: &FlowBuildExecution,
  outputs: &FactoryNodeBuildOutputs,
  capture_tool: &ImmutableReference,
  at: Timestamp,
) -> Result<VerifiedFactoryCandidateDeliverables, FactoryChangeSetError> {
  let invalid = || FactoryChangeSetError::InvalidEvidence;
  let candidate = verify_factory_node_change_set(intent, execution, outputs, capture_tool, at)?;
  let document = outputs
    .documents
    .iter()
    .find(|doc| doc.record.identity().logical_name.as_str() == CHANGE_SET_MANIFEST_OUTPUT)
    .ok_or_else(invalid)?;
  let manifest: CapturedChangeSetManifestV1 = serde_json::from_slice(&document.bytes).map_err(|_| invalid())?;
  let contract = intent
    .profile()
    .capture_deliverables
    .as_ref()
    .ok_or_else(invalid)?
    .freeze(intent.input())
    .map_err(|_| invalid())?;
  Ok(VerifiedFactoryCandidateDeliverables {
    candidate,
    manifest: crate::factory_node_build::artifact_reference(document).map_err(|_| invalid())?,
    contract,
    files: cumulative_files(intent, &manifest)?,
    changes: candidate_files(&manifest.changed_paths)?,
  })
}
pub(super) fn candidate_files(
  paths: &[octacity_protocol::CapturedChangeSetPathV1],
) -> Result<Vec<CandidateFileObservation>, FactoryChangeSetError> {
  let invalid = || FactoryChangeSetError::InvalidEvidence;
  paths
    .iter()
    .map(|file| {
      Ok(CandidateFileObservation {
        path: FactoryRepositoryPath::new(&file.path).map_err(|_| invalid())?,
        content_digest: if file.status == "D" {
          None
        } else {
          Some(
            file
              .content_sha256
              .as_ref()
              .ok_or_else(invalid)?
              .parse()
              .map_err(|_| invalid())?,
          )
        },
      })
    })
    .collect()
}
pub(super) fn verify_deliverables(
  intent: &FlowBuildIntent,
  manifest: &CapturedChangeSetManifestV1,
) -> Result<(), FactoryChangeSetError> {
  if let Some(binding) = &intent.profile().capture_deliverables {
    let contract = binding
      .freeze(intent.input())
      .map_err(|_| FactoryChangeSetError::InvalidEvidence)?;
    contract
      .verify_candidate(
        &candidate_files(&manifest.changed_paths)?,
        &cumulative_files(intent, manifest)?,
        &[],
      )
      .map_err(|_| FactoryChangeSetError::InvalidEvidence)?;
  }
  Ok(())
}
fn cumulative_files(
  intent: &FlowBuildIntent,
  manifest: &CapturedChangeSetManifestV1,
) -> Result<Vec<CandidateFileObservation>, FactoryChangeSetError> {
  let changes = manifest
    .cumulative_changes
    .as_ref()
    .ok_or(FactoryChangeSetError::InvalidEvidence)?;
  if changes.base_revision
    != intent
      .input()
      .exact_resource()
      .map_err(|_| FactoryChangeSetError::InvalidEvidence)?
      .base_revision()
      .as_str()
  {
    return Err(FactoryChangeSetError::InvalidEvidence);
  }
  candidate_files(&changes.changed_paths)
}

/// Restores mandatory deliverables from the exact retained capture bytes and original producer.
/// Qualification uses the same immutable policy and file digests as fresh capture.
pub(crate) fn restore_factory_node_candidate_deliverables(
  record: &octacity_server_factory::FlowNodeRecord,
  intent: &FlowBuildIntent,
  execution: &FlowBuildExecution,
  document: &crate::FactoryNodeBuildOutputDocument,
  capture_tool: &ImmutableReference,
) -> Result<VerifiedFactoryCandidateDeliverables, FactoryChangeSetError> {
  let invalid = || FactoryChangeSetError::InvalidEvidence;
  let candidate = crate::restore_factory_node_change_set(record, intent, execution, document, capture_tool)?;
  let manifest: CapturedChangeSetManifestV1 = serde_json::from_slice(&document.bytes).map_err(|_| invalid())?;
  let contract = intent
    .profile()
    .capture_deliverables
    .as_ref()
    .ok_or_else(invalid)?
    .freeze(intent.input())
    .map_err(|_| invalid())?;
  Ok(VerifiedFactoryCandidateDeliverables {
    candidate,
    manifest: crate::factory_node_build::artifact_reference(document).map_err(|_| invalid())?,
    contract,
    files: cumulative_files(intent, &manifest)?,
    changes: candidate_files(&manifest.changed_paths)?,
  })
}
