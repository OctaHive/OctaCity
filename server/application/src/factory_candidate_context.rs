//! Exact retained candidate inputs separated from declared textual model context.
use crate::{
  ApplicationError, FactoryArtifactReader, FactoryManagedCandidateProof, FactoryNodeBuildOutputDocument,
  ReadFactoryArtifact, restore_factory_node_change_set,
};
use octacity_artifact_store::{ArtifactByteReader, MAX_ARTIFACT_READ_BYTES};
use octacity_protocol::{CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_OUTPUT};
use octacity_server_domain::{ArtifactId, Timestamp};
use octacity_server_factory::{
  FactoryContextReference, FactoryDigest, FlowBuildExecution, FlowBuildIntent, FlowNodeInput, FlowNodeRecord,
  ImmutableReference, MAX_CONTEXT_MANIFEST_BYTES,
};
use octacity_server_store::{ArtifactRecord, ArtifactRecordStore, BuildQueryStore};
use std::{collections::BTreeMap, sync::Arc};

/// Exact selected candidate and its owner-retained producing facts.
/// Producer records, intent and execution must come from the trusted store;
/// model results and mutable lifecycle candidate pointers supply no authority.
pub struct ReadFactoryCandidateContext<'a> {
  /// Complete frozen input explicitly selecting the exact candidate artifacts.
  pub input: &'a FlowNodeInput,
  /// Owner-retained producer completion with independently verified outputs.
  pub producer_record: &'a FlowNodeRecord,
  /// Exact immutable producing intent retained alongside that completion.
  pub producer_intent: &'a FlowBuildIntent,
  /// Original ordinary Build, Attempt and Job identities retained by the owner.
  pub producer_execution: &'a FlowBuildExecution,
  /// Installed trusted capture implementation selected for that producer.
  pub capture_tool: &'a ImmutableReference,
  /// Trusted current time for publication and retention checks.
  pub observed_at: Timestamp,
  /// Exclusive end of the execution window that retention must cover.
  pub required_until: Timestamp,
}

/// Retained exact source proof and private declared model input bytes.
/// Candidate bundle and capture manifest remain source inputs, not prompt text.
pub struct FactoryCandidateContext {
  input_digest: FactoryDigest,
  candidate: FactoryManagedCandidateProof,
  bundle: ArtifactRecord,
  manifest: ArtifactRecord,
  model_context: BTreeMap<ArtifactId, Vec<u8>>,
  retained_until: Option<Timestamp>,
}
impl FactoryCandidateContext {
  /// Complete frozen input to which the proof and model context belong.
  pub const fn input_digest(&self) -> FactoryDigest {
    self.input_digest
  }
  /// Opaque proof accepted by the common ordinary Build assembler.
  pub const fn candidate(&self) -> &FactoryManagedCandidateProof {
    &self.candidate
  }
  /// Exact retained bundle publication; binary bytes have not been read here.
  pub const fn bundle(&self) -> &ArtifactRecord {
    &self.bundle
  }
  /// Exact retained trusted capture manifest publication.
  pub const fn manifest(&self) -> &ArtifactRecord {
    &self.manifest
  }
  /// Only remaining explicitly selected UTF-8 artifacts, never capture bytes.
  pub const fn model_context(&self) -> &BTreeMap<ArtifactId, Vec<u8>> {
    &self.model_context
  }
  pub(crate) fn is_retained_at(&self, observed_at: Timestamp) -> bool {
    self.retained_until.is_none_or(|deadline| observed_at < deadline)
  }
}

/// Trusted candidate recovery through the existing Project-owned Artifact reader.
pub struct FactoryCandidateContextSource<A, S, B> {
  reader: Arc<FactoryArtifactReader<A, S, B>>,
}
impl<A: ArtifactRecordStore, S: BuildQueryStore, B: ArtifactByteReader> FactoryCandidateContextSource<A, S, B> {
  /// Installs the same authoritative metadata, ownership and bounded byte gates.
  pub const fn new(reader: Arc<FactoryArtifactReader<A, S, B>>) -> Self {
    Self { reader }
  }

  /// Recovers only a candidate explicitly selected by this immutable context.
  /// The Agent later verifies bundle bytes against the signed protected identity.
  pub async fn read(
    &self,
    request: ReadFactoryCandidateContext<'_>,
  ) -> Result<FactoryCandidateContext, ApplicationError> {
    let invalid = ApplicationError::invalid;
    let context = request.input.context().ok_or_else(invalid)?;
    if context.subject().candidate().is_none() {
      return Err(invalid());
    }
    let provenance = request.producer_record.build_provenance().ok_or_else(invalid)?;
    let output = |name| {
      provenance
        .outputs()
        .iter()
        .find(|output| output.name.as_str() == name)
        .ok_or_else(invalid)
    };
    let bundle_output = output(CHANGE_SET_BUNDLE_OUTPUT)?;
    let manifest_output = output(CHANGE_SET_MANIFEST_OUTPUT)?;
    let mut selected = BTreeMap::new();
    for entry in context.entries() {
      let FactoryContextReference::Artifact(artifact) = entry.source() else {
        return Err(invalid());
      };
      selected.insert(artifact.artifact_id(), artifact);
    }
    for output in [bundle_output, manifest_output] {
      if selected.get(&output.artifact.artifact_id()).copied() != Some(&output.artifact) {
        return Err(invalid());
      }
    }
    let read = |artifact, maximum_bytes| ReadFactoryArtifact {
      project_id: request.input.subject().project_id(),
      artifact,
      maximum_bytes,
      observed_at: request.observed_at,
      required_until: request.required_until,
    };
    let bundle = self
      .reader
      .read_record(read(bundle_output.artifact.clone(), MAX_ARTIFACT_READ_BYTES))
      .await?;
    let retained = self
      .reader
      .read(read(manifest_output.artifact.clone(), MAX_ARTIFACT_READ_BYTES))
      .await?;
    let manifest_document = FactoryNodeBuildOutputDocument {
      record: retained.record().clone(),
      bytes: retained.bytes().to_vec(),
      schema: manifest_output.schema.clone(),
      tool: manifest_output.producer.tool().clone(),
      plugin: manifest_output.producer.plugin().clone(),
      data: None,
    };
    let candidate = restore_factory_node_change_set(
      request.producer_record,
      request.producer_intent,
      request.producer_execution,
      &manifest_document,
      request.capture_tool,
    )
    .map_err(|_| invalid())?;
    let proof = FactoryManagedCandidateProof::verify(
      request.input,
      candidate,
      bundle.clone(),
      retained.record().clone(),
      request.observed_at,
    )
    .map_err(|_| invalid())?;
    let mut model_context = BTreeMap::new();
    let mut retained_until = [bundle.identity(), retained.record().identity()]
      .into_iter()
      .filter_map(|identity| identity.retention.delete_after())
      .min();
    let mut remaining = MAX_CONTEXT_MANIFEST_BYTES;
    for artifact in selected.into_values().filter(|artifact| {
      artifact.artifact_id() != bundle_output.artifact.artifact_id()
        && artifact.artifact_id() != manifest_output.artifact.artifact_id()
    }) {
      let retained = self
        .reader
        .read(read(artifact.clone(), remaining.min(MAX_ARTIFACT_READ_BYTES)))
        .await?;
      std::str::from_utf8(retained.bytes()).map_err(|_| invalid())?;
      retained_until = retained_until
        .into_iter()
        .chain(retained.record().identity().retention.delete_after())
        .min();
      remaining = remaining.checked_sub(artifact.encoded_size()).ok_or_else(invalid)?;
      model_context.insert(artifact.artifact_id(), retained.bytes().to_vec());
    }
    Ok(FactoryCandidateContext {
      input_digest: request.input.digest().map_err(|_| invalid())?,
      candidate: proof,
      bundle,
      manifest: manifest_document.record,
      model_context,
      retained_until,
    })
  }
}
