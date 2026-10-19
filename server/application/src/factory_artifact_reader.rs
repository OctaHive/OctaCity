//! Trusted retained-input reads from Build, Source and Project publication authorities.
use crate::{ApplicationError, VerifiedFactoryArtifact};
use octacity_artifact_store::{ArtifactByteReader, ArtifactObject, MAX_ARTIFACT_READ_BYTES};
use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::{FactoryArtifactReference, FactoryDigest};
use octacity_server_store::{
  ArtifactRecord, ArtifactRecordStore, ArtifactUploadRecord, BuildQueryStore, SourceArtifactQueryStore,
};
use std::sync::Arc;

/// Explicit immutable selection; control dependencies do not grant Artifact access.
#[derive(Clone)]
pub struct ReadFactoryArtifact {
  /// Project that must own the producing ordinary Build.
  pub project_id: ProjectId,
  /// Exact logical identity, content digest and byte size.
  pub artifact: FactoryArtifactReference,
  /// Remaining caller-owned input byte ceiling, within the hard reader bound.
  pub maximum_bytes: u64,
  /// Trusted time used to reject expired or future publications.
  pub observed_at: Timestamp,
  /// Exclusive end of the requested read window that retention must cover.
  pub required_until: Timestamp,
}
/// Published metadata and verified bytes; construction remains inside the reader.
pub struct FactoryRetainedArtifact {
  record: ArtifactRecord,
  verified: VerifiedFactoryArtifact,
}
impl FactoryRetainedArtifact {
  /// Immutable publication metadata used for provenance and retention checks.
  pub const fn record(&self) -> &ArtifactRecord {
    &self.record
  }
  /// Exact bounded bytes. Callers must never include them in diagnostics.
  pub fn bytes(&self) -> &[u8] {
    &self.verified.bytes
  }
}
/// Private context input with verified bytes and exact publication provenance.
pub struct FactoryRetainedInput {
  bytes: Vec<u8>,
  provenance_digest: FactoryDigest,
}
impl FactoryRetainedInput {
  /// Exact private bytes, never diagnostics or routing authority.
  pub fn bytes(&self) -> &[u8] {
    &self.bytes
  }
  /// Original publication identity, distinct for source preparation and Build output.
  pub const fn provenance_digest(&self) -> FactoryDigest {
    self.provenance_digest
  }
}
/// Reads selected inputs without presigned URLs, provider payloads or repository workspaces.
pub struct FactoryArtifactReader<A, S, B> {
  artifacts: Arc<A>,
  builds: Arc<S>,
  bytes: Arc<B>,
  source_inputs: Option<Arc<dyn SourceArtifactQueryStore>>,
  project_inputs: Option<Arc<dyn octacity_server_store::ProjectInputQueryStore>>,
}
impl<A: ArtifactRecordStore, S: BuildQueryStore, B: ArtifactByteReader> FactoryArtifactReader<A, S, B> {
  /// Uses authoritative logical publication, ordinary Build ownership and bounded byte ports.
  pub fn new(artifacts: Arc<A>, builds: Arc<S>, bytes: Arc<B>) -> Self {
    Self {
      artifacts,
      builds,
      bytes,
      source_inputs: None,
      project_inputs: None,
    }
  }
  /// Installs private source input reads without granting produced-output provenance.
  pub fn with_source_inputs(mut self, store: Arc<dyn SourceArtifactQueryStore>) -> Self {
    self.source_inputs = Some(store);
    self
  }
  /// Installs Project-owned private text without granting produced-output provenance.
  pub fn with_project_inputs(mut self, store: Arc<dyn octacity_server_store::ProjectInputQueryStore>) -> Self {
    self.project_inputs = Some(store);
    self
  }
  /// Reads explicit context data from its authoritative publication owner.
  pub async fn read_input(&self, request: ReadFactoryArtifact) -> Result<FactoryRetainedInput, ApplicationError> {
    if let Some(store) = &self.source_inputs
      && let Some(source) = store
        .source_artifact(request.project_id, request.artifact.artifact_id())
        .await?
    {
      let provenance = serde_json::to_vec(&source).map_err(|_| ApplicationError::invalid())?;
      let digest = FactoryDigest::sha256("octacity.factory.source-input-provenance.v1", &[&provenance]);
      return self
        .read_private_input(
          &request,
          source.project_id,
          &source.object,
          source.published_at,
          source.expires_at,
          digest,
        )
        .await;
    }
    if let Some(store) = &self.project_inputs
      && let Some(input) = store
        .project_input_artifact(request.project_id, request.artifact.artifact_id())
        .await?
    {
      if input.object.media_type.as_str() != "text/plain" {
        return Err(ApplicationError::invalid());
      }
      let provenance = serde_json::to_vec(&input).map_err(|_| ApplicationError::invalid())?;
      let digest = FactoryDigest::sha256("octacity.factory.project-input-provenance.v1", &[&provenance]);
      return self
        .read_private_input(
          &request,
          input.project_id,
          &input.object,
          input.published_at,
          input.expires_at,
          digest,
        )
        .await;
    }
    let retained = self.read(request).await?;
    let provenance = serde_json::to_vec(retained.record().identity()).map_err(|_| ApplicationError::invalid())?;
    Ok(FactoryRetainedInput {
      bytes: retained.bytes().to_vec(),
      provenance_digest: FactoryDigest::sha256("octacity.factory.artifact-context-provenance.v1", &[&provenance]),
    })
  }
  // Every non-Build input uses the same byte, Project and retention checks.
  async fn read_private_input(
    &self,
    request: &ReadFactoryArtifact,
    project: ProjectId,
    object: &octacity_server_store::FactoryInputObject,
    published_at: Timestamp,
    expires_at: Timestamp,
    provenance_digest: FactoryDigest,
  ) -> Result<FactoryRetainedInput, ApplicationError> {
    if project != request.project_id
      || object.artifact != request.artifact
      || request.artifact.encoded_size() > request.maximum_bytes
      || request.maximum_bytes > MAX_ARTIFACT_READ_BYTES
      || published_at > request.observed_at
      || request.observed_at >= expires_at
      || request.required_until <= request.observed_at
      || request.required_until > expires_at
    {
      return Err(ApplicationError::invalid());
    }
    let description = ArtifactObject::new(
      object.artifact.artifact_id(),
      object.upload_id,
      object.artifact.encoded_size(),
      object.artifact.content_digest().to_string(),
      object.media_type.as_str(),
    )
    .map_err(|_| ApplicationError::invalid())?;
    let bytes = self
      .bytes
      .read_published(&description, request.maximum_bytes)
      .await
      .map_err(|_| ApplicationError::unavailable())?;
    if bytes.len() as u64 != object.artifact.encoded_size()
      || FactoryDigest::content_sha256(&bytes) != object.artifact.content_digest()
    {
      return Err(ApplicationError::unavailable());
    }
    Ok(FactoryRetainedInput {
      bytes,
      provenance_digest,
    })
  }
  /// Verifies selection and retention before any protected bytes are read.
  pub async fn read(&self, request: ReadFactoryArtifact) -> Result<FactoryRetainedArtifact, ApplicationError> {
    let upload = self.retained_upload(&request).await?;
    let record = upload.artifact;
    let identity = record.identity();
    let object = ArtifactObject::new(
      identity.artifact_id,
      upload.upload_id,
      identity.size_bytes,
      identity.digest.to_string(),
      upload.transport_media_type.as_str(),
    )
    .map_err(|_| ApplicationError::invalid())?;
    let bytes = self
      .bytes
      .read_published(&object, request.maximum_bytes)
      .await
      .map_err(|_| ApplicationError::unavailable())?;
    let verified =
      VerifiedFactoryArtifact::new(identity.clone(), bytes).map_err(|_| ApplicationError::unavailable())?;
    Ok(FactoryRetainedArtifact { record, verified })
  }

  /// Verifies identical ownership and retention gates without fetching bytes.
  /// Candidate bundles are transferred to the Agent using their signed immutable
  /// identity; their binary contents do not belong in model-context rendering.
  pub async fn read_record(&self, request: ReadFactoryArtifact) -> Result<ArtifactRecord, ApplicationError> {
    Ok(self.retained_upload(&request).await?.artifact)
  }

  async fn retained_upload(&self, request: &ReadFactoryArtifact) -> Result<ArtifactUploadRecord, ApplicationError> {
    if request.artifact.encoded_size() > request.maximum_bytes
      || request.maximum_bytes > MAX_ARTIFACT_READ_BYTES
      || request.required_until <= request.observed_at
    {
      return Err(ApplicationError::invalid());
    }
    let upload = self
      .artifacts
      .published_artifact(request.artifact.artifact_id())
      .await?;
    let record = &upload.artifact;
    let identity = record.identity();
    if identity.artifact_id != request.artifact.artifact_id()
      || identity.size_bytes != request.artifact.encoded_size()
      || identity.digest.as_bytes() != request.artifact.content_digest().as_bytes()
      || !record.state().is_visible()
      || record
        .published_at()
        .is_none_or(|published| published > request.observed_at)
      || identity
        .retention
        .delete_after()
        .is_some_and(|deadline| deadline < request.required_until)
      // Regular output files are uploaded unchanged as binary, independently
      // of their logical JSON/trace media type. Other transport conversions
      // remain unsupported; this reader never unpacks or transforms content.
      || (upload.transport_media_type != identity.media_type
        && upload.transport_media_type.as_str() != "application/octet-stream")
    {
      return Err(ApplicationError::invalid());
    }
    let build = self.builds.build(identity.build_id).await?;
    if build.build.project_id != request.project_id {
      return Err(ApplicationError::invalid());
    }
    Ok(upload)
  }
}

/// Reads exactly the selected common input context with Project and retention checks.
/// Returned bytes are private execution data, never diagnostics or routing authority.
#[async_trait::async_trait]
pub trait FactoryNodeContextSource: Send + Sync {
  /// Resolves immutable selection at trusted time through the requested retained
  /// read window. Execution preparation uses its original deadline; read-only
  /// terminal settlement can require a later observation without granting dispatch.
  async fn read_context(
    &self,
    input: &octacity_server_factory::FlowNodeInput,
    observed_at: Timestamp,
    deadline: Timestamp,
  ) -> Result<std::collections::BTreeMap<octacity_server_domain::ArtifactId, Vec<u8>>, ApplicationError>;
}
#[async_trait::async_trait]
impl<A: ArtifactRecordStore, S: BuildQueryStore, B: ArtifactByteReader> FactoryNodeContextSource
  for FactoryArtifactReader<A, S, B>
{
  async fn read_context(
    &self,
    input: &octacity_server_factory::FlowNodeInput,
    observed_at: Timestamp,
    deadline: Timestamp,
  ) -> Result<std::collections::BTreeMap<octacity_server_domain::ArtifactId, Vec<u8>>, ApplicationError> {
    let mut result = std::collections::BTreeMap::new();
    let mut remaining = octacity_server_factory::MAX_CONTEXT_MANIFEST_BYTES;
    if let Some(context) = input.context() {
      for entry in context.entries() {
        let octacity_server_factory::FactoryContextReference::Artifact(artifact) = entry.source() else {
          return Err(ApplicationError::invalid());
        };
        if result.contains_key(&artifact.artifact_id()) {
          continue;
        }
        let retained = self
          .read_input(ReadFactoryArtifact {
            project_id: input.subject().project_id(),
            artifact: artifact.clone(),
            maximum_bytes: remaining.min(MAX_ARTIFACT_READ_BYTES),
            observed_at,
            required_until: deadline,
          })
          .await?;
        remaining = remaining
          .checked_sub(artifact.encoded_size())
          .ok_or_else(ApplicationError::invalid)?;
        result.insert(artifact.artifact_id(), retained.bytes().to_vec());
      }
    }
    Ok(result)
  }
}
