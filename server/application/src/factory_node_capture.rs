//! Trusted candidate verification over a generic node's frozen input and execution.
//! This capability reads no lifecycle checkpoint and grants no Flow transition.

use crate::FactoryNodeBuildOutputs;
use crate::factory_candidate_deliverables::verify_deliverables;
use octacity_protocol::{
  CHANGE_SET_BUNDLE_MEDIA_TYPE, CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_MEDIA_TYPE, CHANGE_SET_MANIFEST_OUTPUT,
  CHANGE_SET_PATCH_MEDIA_TYPE, CHANGE_SET_PATCH_OUTPUT, CapturedChangeSetFileV1, CapturedChangeSetManifestV1,
};
use octacity_server_artifacts::{ArtifactIdentity, ArtifactRecord, ArtifactState, ArtifactType};
use octacity_server_domain::{ArtifactId, ImmutableRevision, Timestamp};
use octacity_server_factory::{
  CandidateSubject, ChangeSet, ChangeSetId, EvidenceOutputKind, FactoryContextReference, FactoryDigest,
  FlowBuildExecution, FlowBuildIntent, FlowNodeInput, FlowNodeRecord, ImmutableReference,
};
use octacity_server_store::StoreError;
use thiserror::Error;
use uuid::Uuid;

/// Exact published Artifact bytes verified against their immutable identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedFactoryArtifact {
  pub(super) identity: ArtifactIdentity,
  pub(super) bytes: Vec<u8>,
}

impl VerifiedFactoryArtifact {
  /// Verifies bounded bytes against already-published logical metadata.
  pub fn new(identity: ArtifactIdentity, bytes: Vec<u8>) -> Result<Self, FactoryChangeSetError> {
    if identity.size_bytes != u64::try_from(bytes.len()).unwrap_or(u64::MAX)
      || identity.digest.as_bytes() != FactoryDigest::content_sha256(&bytes).as_bytes()
    {
      return Err(FactoryChangeSetError::Integrity);
    }
    Ok(Self { identity, bytes })
  }
}

/// Stable failures from trusted candidate acceptance.
#[derive(Debug, Error)]
pub enum FactoryChangeSetError {
  /// Frozen node input, ordinary execution, names or manifest are inconsistent.
  #[error("Factory ChangeSet capture evidence is invalid")]
  InvalidEvidence,
  /// Published bytes differ from their immutable Artifact identities.
  #[error("Factory ChangeSet Artifact integrity verification failed")]
  Integrity,
  /// The authoritative store rejected or could not complete the operation.
  #[error("Factory ChangeSet store operation failed")]
  Store(#[from] StoreError),
}

/// Supplies protected candidate inputs selected by a node's immutable context.
/// The caller reads these records from the Artifact store; the ordinary Job
/// subsequently verifies downloaded bytes against the signed exact references.
/// No implicit predecessor, stage role or aggregate candidate pointer is read.
pub fn factory_node_candidate_materialization(
  input: &FlowNodeInput,
  candidate: &ChangeSet,
  bundle: &ArtifactRecord,
  manifest: &ArtifactRecord,
  at: Timestamp,
) -> Result<crate::FactoryCandidateMaterialization, FactoryChangeSetError> {
  let invalid = || FactoryChangeSetError::InvalidEvidence;
  let context = input.context().ok_or_else(invalid)?;
  if context.subject().candidate() != Some(candidate.subject())
    || candidate.subject().exact() != input.subject()
    || bundle.identity().artifact_id != candidate.bundle_artifact()
    || manifest.identity().artifact_id != candidate.manifest_artifact()
    || bundle.identity().job_id != manifest.identity().job_id
    || bundle.identity().lease_id != manifest.identity().lease_id
  {
    return Err(invalid());
  }
  for (record, name, media) in [
    (bundle, CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_BUNDLE_MEDIA_TYPE),
    (manifest, CHANGE_SET_MANIFEST_OUTPUT, CHANGE_SET_MANIFEST_MEDIA_TYPE),
  ] {
    let identity = record.identity();
    if record.state() != ArtifactState::Published
      || record.published_at().is_none_or(|published| published > at)
      || identity.retention.delete_after().is_some_and(|deadline| deadline <= at)
      || identity.logical_name.as_str() != name
      || !artifact_type(identity, media)
      || !context.entries().iter().any(|entry| {
        matches!(entry.source(), FactoryContextReference::Artifact(reference)
        if reference.artifact_id() == identity.artifact_id
          && reference.content_digest().as_bytes() == identity.digest.as_bytes()
          && reference.encoded_size() == identity.size_bytes)
      })
    {
      return Err(invalid());
    }
  }
  Ok(crate::FactoryCandidateMaterialization {
    change_set_id: candidate.id(),
    base_revision: input.subject().base_revision().clone(),
    candidate_revision: candidate.subject().candidate_revision().clone(),
    bundle: bundle.identity().clone(),
    manifest: manifest.identity().clone(),
  })
}

/// Verifies an ordinary Build's trusted capture under its exact frozen node intent.
/// The capture base comes from the explicitly selected context subject: the Work
/// base or a retained candidate. The installed capability selects `capture_tool`;
/// no logical node name, stage role or provider field supplies authority.
/// The common owner later retains this result and its completion atomically.
pub fn verify_factory_node_change_set(
  intent: &FlowBuildIntent,
  execution: &FlowBuildExecution,
  outputs: &FactoryNodeBuildOutputs,
  capture_tool: &ImmutableReference,
  observed_at: Timestamp,
) -> Result<ChangeSet, FactoryChangeSetError> {
  let checked = FlowBuildExecution::new(
    intent,
    execution.build_id(),
    execution.attempt_id(),
    execution.jobs().to_vec(),
  )
  .map_err(|_| FactoryChangeSetError::InvalidEvidence)?;
  if checked != *execution {
    return Err(FactoryChangeSetError::InvalidEvidence);
  }
  crate::factory_node_build::validate_execution_facts(intent, outputs)
    .map_err(|_| FactoryChangeSetError::InvalidEvidence)?;
  crate::factory_node_build::validate_outputs(
    outputs,
    intent.budget(),
    intent.node(),
    &crate::factory_node_build::ObservedBuild {
      build_id: execution.build_id(),
      attempt_id: execution.attempt_id(),
      jobs: execution.jobs().to_vec(),
    },
    observed_at,
  )
  .map_err(|_| FactoryChangeSetError::InvalidEvidence)?;
  let published = outputs
    .documents
    .iter()
    .map(|document| document.record.identity().clone())
    .collect::<Vec<_>>();
  let documents = outputs
    .documents
    .iter()
    .filter(|document| {
      matches!(
        document.record.identity().logical_name.as_str(),
        CHANGE_SET_BUNDLE_OUTPUT | CHANGE_SET_MANIFEST_OUTPUT | CHANGE_SET_PATCH_OUTPUT
      )
    })
    .map(|document| {
      if document.tool != *capture_tool || document.plugin != intent.profile().plugin {
        return Err(FactoryChangeSetError::InvalidEvidence);
      }
      VerifiedFactoryArtifact::new(document.record.identity().clone(), document.bytes.clone())
    })
    .collect::<Result<Vec<_>, _>>()?;
  let output = CaptureOutputs::load(&published, documents)?;
  let manifest = parse_manifest(&output.manifest)?;
  if manifest.capture_tool.identity != capture_tool.identity().as_str()
    || manifest.capture_tool.version != capture_tool.version().as_str()
    || manifest.capture_tool.sha256 != capture_tool.digest().to_string()
  {
    return Err(FactoryChangeSetError::InvalidEvidence);
  }
  let subject = intent
    .input()
    .context()
    .ok_or(FactoryChangeSetError::InvalidEvidence)?
    .subject();
  let base = subject
    .candidate()
    .map_or(subject.exact().base_revision(), |candidate| {
      candidate.candidate_revision()
    });
  validate_manifest(&manifest, intent.node().id().to_string(), base.as_str(), &output)?;
  verify_deliverables(intent, &manifest)?;
  captured_candidate(
    intent,
    execution,
    &manifest,
    (
      output.bundle.identity.artifact_id,
      FactoryDigest::from_bytes(output.bundle.identity.digest.as_bytes()),
    ),
    (
      output.manifest.identity.artifact_id,
      FactoryDigest::from_bytes(output.manifest.identity.digest.as_bytes()),
    ),
  )
}

/// Recovers a candidate from an owner-retained capture result after restart.
/// This reads previously verified provenance; materializing the bundle still
/// requires fetching and verifying the referenced immutable Artifact bytes.
/// No current-stage pointer or lifecycle projection participates in recovery.
pub fn factory_node_change_set(
  record: &FlowNodeRecord,
  intent: &FlowBuildIntent,
  execution: &FlowBuildExecution,
  capture_tool: &ImmutableReference,
) -> Result<ChangeSet, FactoryChangeSetError> {
  if intent.profile().result_tool() != capture_tool
    || intent.profile().result_output.as_str() != CHANGE_SET_MANIFEST_OUTPUT
    || intent.profile().result_output_kind != EvidenceOutputKind::Artifact
  {
    return Err(FactoryChangeSetError::InvalidEvidence);
  }
  let manifest: CapturedChangeSetManifestV1 =
    serde_json::from_value(record.payload().value().clone()).map_err(|_| FactoryChangeSetError::InvalidEvidence)?;
  let mut bytes = serde_json::to_vec(&manifest).map_err(|_| FactoryChangeSetError::InvalidEvidence)?;
  bytes.push(b'\n');
  recover_candidate(
    record,
    intent,
    execution,
    capture_tool,
    &bytes,
    record.payload().schema(),
  )
}

/// Restores capture selected as additional private evidence beside a public result.
/// Actual fetched bytes must match the owner-retained provenance; no result prose
/// or mutable lifecycle pointer can provide a replacement candidate.
pub fn restore_factory_node_change_set(
  record: &FlowNodeRecord,
  intent: &FlowBuildIntent,
  execution: &FlowBuildExecution,
  manifest: &crate::FactoryNodeBuildOutputDocument,
  capture_tool: &ImmutableReference,
) -> Result<ChangeSet, FactoryChangeSetError> {
  let invalid = || FactoryChangeSetError::InvalidEvidence;
  let proof = record
    .build_provenance()
    .and_then(|proof| {
      proof
        .outputs()
        .iter()
        .find(|output| output.name.as_str() == CHANGE_SET_MANIFEST_OUTPUT)
    })
    .ok_or_else(invalid)?;
  if manifest.record.state() != ArtifactState::Published
    || manifest.record.published_at().is_none_or(|at| at > proof.verified_at)
    || manifest.record.identity().logical_name.as_str() != CHANGE_SET_MANIFEST_OUTPUT
    || !artifact_type(manifest.record.identity(), CHANGE_SET_MANIFEST_MEDIA_TYPE)
    || crate::factory_node_build::artifact_reference(manifest).map_err(|_| invalid())? != proof.artifact
    || crate::factory_node_build::document_producer(manifest) != proof.producer
    || manifest.tool != *capture_tool
    || manifest.plugin != intent.profile().plugin
    || manifest.schema != proof.schema
  {
    return Err(invalid());
  }
  VerifiedFactoryArtifact::new(manifest.record.identity().clone(), manifest.bytes.clone())?;
  recover_candidate(
    record,
    intent,
    execution,
    capture_tool,
    &manifest.bytes,
    &manifest.schema,
  )
}

fn recover_candidate(
  record: &FlowNodeRecord,
  intent: &FlowBuildIntent,
  execution: &FlowBuildExecution,
  capture_tool: &ImmutableReference,
  bytes: &[u8],
  schema: &ImmutableReference,
) -> Result<ChangeSet, FactoryChangeSetError> {
  let invalid = || FactoryChangeSetError::InvalidEvidence;
  let provenance = record.build_provenance().ok_or_else(invalid)?;
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
  {
    return Err(invalid());
  }
  let output = |name: &str| {
    provenance
      .outputs()
      .iter()
      .find(|output| output.name.as_str() == name)
      .ok_or_else(invalid)
  };
  let manifest_output = output(CHANGE_SET_MANIFEST_OUTPUT)?;
  let bundle = output(CHANGE_SET_BUNDLE_OUTPUT)?;
  let manifest: CapturedChangeSetManifestV1 = serde_json::from_slice(bytes).map_err(|_| invalid())?;
  manifest.validate().map_err(|_| invalid())?;
  let subject = intent.input().context().ok_or_else(invalid)?.subject();
  let base = subject
    .candidate()
    .map_or(subject.exact().base_revision(), |candidate| {
      candidate.candidate_revision()
    });
  let file_matches = |file: &CapturedChangeSetFileV1| {
    output(&file.name).is_ok_and(|output| {
      file.size_bytes == output.artifact.encoded_size() && file.sha256 == output.artifact.content_digest().to_string()
    })
  };
  if manifest.node_attempt_id != intent.node().id().to_string()
    || manifest.base_revision != base.as_str()
    || manifest.capture_tool.identity != capture_tool.identity().as_str()
    || manifest.capture_tool.version != capture_tool.version().as_str()
    || manifest.capture_tool.sha256 != capture_tool.digest().to_string()
    || manifest_output.artifact.encoded_size() != bytes.len() as u64
    || manifest_output.artifact.content_digest() != FactoryDigest::content_sha256(bytes)
    || manifest_output.schema != *schema
    || !file_matches(&manifest.bundle)
    || match &manifest.patch {
      Some(patch) => !file_matches(patch),
      None => output(CHANGE_SET_PATCH_OUTPUT).is_ok(),
    }
    || provenance.outputs().iter().any(|output| {
      matches!(
        output.name.as_str(),
        CHANGE_SET_BUNDLE_OUTPUT | CHANGE_SET_MANIFEST_OUTPUT | CHANGE_SET_PATCH_OUTPUT
      ) && (output.output_kind != EvidenceOutputKind::Artifact
        || output.producer.build_id() != execution.build_id()
        || output.producer.attempt_id() != execution.attempt_id()
        || !execution.jobs().contains(&output.producer.job_id())
        || output.producer.job_id() != manifest_output.producer.job_id()
        || output.producer.tool() != capture_tool
        || output.producer.plugin() != &intent.profile().plugin
        || output.verified_at > record.observed_at())
    })
  {
    return Err(invalid());
  }
  verify_deliverables(intent, &manifest)?;
  captured_candidate(
    intent,
    execution,
    &manifest,
    (bundle.artifact.artifact_id(), bundle.artifact.content_digest()),
    (
      manifest_output.artifact.artifact_id(),
      manifest_output.artifact.content_digest(),
    ),
  )
}

fn captured_candidate(
  intent: &FlowBuildIntent,
  execution: &FlowBuildExecution,
  manifest: &CapturedChangeSetManifestV1,
  bundle: (ArtifactId, FactoryDigest),
  document: (ArtifactId, FactoryDigest),
) -> Result<ChangeSet, FactoryChangeSetError> {
  let digest = FactoryDigest::sha256(
    "octacity.factory.changeset.v1",
    &[
      manifest.base_revision.as_bytes(),
      manifest.candidate_revision.as_bytes(),
      &bundle.1.as_bytes(),
      &document.1.as_bytes(),
    ],
  );
  let subject = CandidateSubject::new(
    intent.input().subject().clone(),
    ImmutableRevision::new(manifest.candidate_revision.clone()).map_err(|_| FactoryChangeSetError::InvalidEvidence)?,
    digest,
  );
  let id = ChangeSetId::from_uuid(Uuid::new_v5(
    &intent.node().id().as_uuid(),
    execution.operation_id().as_bytes().as_slice(),
  ))
  .map_err(|_| FactoryChangeSetError::InvalidEvidence)?;
  ChangeSet::new(id, intent.input(), intent.node(), subject, bundle.0, document.0)
    .map_err(|_| FactoryChangeSetError::InvalidEvidence)
}

pub(super) struct CaptureOutputs {
  pub(super) bundle: VerifiedFactoryArtifact,
  pub(super) manifest: VerifiedFactoryArtifact,
  pub(super) patch: Option<VerifiedFactoryArtifact>,
}

impl CaptureOutputs {
  pub(super) fn load(
    published: &[ArtifactIdentity],
    documents: Vec<VerifiedFactoryArtifact>,
  ) -> Result<Self, FactoryChangeSetError> {
    let published_components = published
      .iter()
      .filter(|identity| {
        matches!(
          identity.logical_name.as_str(),
          CHANGE_SET_BUNDLE_OUTPUT | CHANGE_SET_MANIFEST_OUTPUT | CHANGE_SET_PATCH_OUTPUT
        )
      })
      .count();
    if documents.len() != published_components {
      return Err(FactoryChangeSetError::InvalidEvidence);
    }
    for document in &documents {
      if !published.contains(&document.identity) {
        return Err(FactoryChangeSetError::InvalidEvidence);
      }
    }
    let mut bundle = None;
    let mut manifest = None;
    let mut patch = None;
    for document in documents {
      let slot = match document.identity.logical_name.as_str() {
        CHANGE_SET_BUNDLE_OUTPUT => &mut bundle,
        CHANGE_SET_MANIFEST_OUTPUT => &mut manifest,
        CHANGE_SET_PATCH_OUTPUT => &mut patch,
        _ => return Err(FactoryChangeSetError::InvalidEvidence),
      };
      if slot.replace(document).is_some() {
        return Err(FactoryChangeSetError::InvalidEvidence);
      }
    }
    let output = Self {
      bundle: bundle.ok_or(FactoryChangeSetError::InvalidEvidence)?,
      manifest: manifest.ok_or(FactoryChangeSetError::InvalidEvidence)?,
      patch,
    };
    if !artifact_type(&output.bundle.identity, CHANGE_SET_BUNDLE_MEDIA_TYPE)
      || !artifact_type(&output.manifest.identity, CHANGE_SET_MANIFEST_MEDIA_TYPE)
      || output.bundle.identity.job_id != output.manifest.identity.job_id
      || output.bundle.identity.lease_id != output.manifest.identity.lease_id
      || output.patch.as_ref().is_some_and(|patch| {
        !artifact_type(&patch.identity, CHANGE_SET_PATCH_MEDIA_TYPE)
          || patch.identity.job_id != output.bundle.identity.job_id
          || patch.identity.lease_id != output.bundle.identity.lease_id
      })
    {
      return Err(FactoryChangeSetError::InvalidEvidence);
    }
    Ok(output)
  }
}

fn artifact_type(identity: &ArtifactIdentity, media_type: &str) -> bool {
  matches!(identity.artifact_type, ArtifactType::Artifact) && identity.media_type.as_str() == media_type
}

pub(super) fn parse_manifest(
  document: &VerifiedFactoryArtifact,
) -> Result<CapturedChangeSetManifestV1, FactoryChangeSetError> {
  CapturedChangeSetManifestV1::decode_canonical(&document.bytes).map_err(|_| FactoryChangeSetError::InvalidEvidence)
}

pub(super) fn validate_manifest(
  manifest: &CapturedChangeSetManifestV1,
  node_attempt_id: String,
  base_revision: &str,
  output: &CaptureOutputs,
) -> Result<(), FactoryChangeSetError> {
  if manifest.node_attempt_id != node_attempt_id
    || manifest.base_revision != base_revision
    || !file_matches(&manifest.bundle, &output.bundle)
  {
    return Err(FactoryChangeSetError::InvalidEvidence);
  }
  match (&manifest.patch, &output.patch) {
    (None, None) => Ok(()),
    (Some(expected), Some(actual)) if expected.name == CHANGE_SET_PATCH_OUTPUT && file_matches(expected, actual) => {
      Ok(())
    }
    _ => Err(FactoryChangeSetError::InvalidEvidence),
  }
}

fn file_matches(expected: &CapturedChangeSetFileV1, actual: &VerifiedFactoryArtifact) -> bool {
  expected.size_bytes == actual.identity.size_bytes
    && expected.sha256 == actual.identity.digest.to_string()
    && expected.name == actual.identity.logical_name.as_str()
}
