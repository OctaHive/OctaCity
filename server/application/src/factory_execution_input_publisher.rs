//! Durable bounded publication of trusted compiler documents.
use crate::{ApplicationError, FactoryExecutionClock};
use octacity_artifact_store::{ArtifactBytePublisher, ArtifactObject, MAX_ARTIFACT_READ_BYTES};
use octacity_protocol::{ProtectedInputManifestV3, ProtectedInputV3};
use octacity_server_domain::{ArtifactId, ArtifactUploadId};
use octacity_server_factory::{FactoryClaimOwnership, FactoryDigest, FlowBuildIntent};
use octacity_server_store::{
  ArtifactMediaType, FactoryExecutionInputDisposition, FactoryExecutionInputPublication, FactoryExecutionInputStore,
  PublishFactoryExecutionInputs,
};
use std::sync::Arc;
use uuid::Uuid;

/// Private trusted compiler output; bytes and destinations must not reach diagnostics.
#[derive(Clone)]
pub struct FactoryExecutionDocument {
  destination: String,
  media_type: ArtifactMediaType,
  bytes: Vec<u8>,
}
impl FactoryExecutionDocument {
  /// Bounds one document's bytes and media type. Complete destination and
  /// aggregate manifest validation occurs before reservation or storage I/O.
  pub fn new(
    destination: impl Into<String>,
    media_type: impl Into<String>,
    bytes: Vec<u8>,
  ) -> Result<Self, ApplicationError> {
    if bytes.len() as u64 > MAX_ARTIFACT_READ_BYTES {
      return Err(ApplicationError::invalid());
    }
    Ok(Self {
      destination: destination.into(),
      media_type: ArtifactMediaType::new(media_type).map_err(|_| ApplicationError::invalid())?,
      bytes,
    })
  }
}

/// Reserves cleanup identities before physical writes, then publishes under the same owner fence.
pub struct FactoryExecutionInputPublisher<S, B> {
  store: Arc<S>,
  bytes: Arc<B>,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<S: FactoryExecutionInputStore, B: ArtifactBytePublisher> FactoryExecutionInputPublisher<S, B> {
  /// Installs authoritative publication, verified bytes and trusted time.
  pub fn new(store: Arc<S>, bytes: Arc<B>, clock: Arc<dyn FactoryExecutionClock>) -> Self {
    Self { store, bytes, clock }
  }
  /// Publishes only explicit generated documents; repository files are never inferred.
  /// Identities are stable for this exact operation and document content. An
  /// interrupted write remains durably discoverable for expiration cleanup.
  pub async fn publish(
    &self,
    intent: &FlowBuildIntent,
    ownership: &FactoryClaimOwnership,
    documents: Vec<FactoryExecutionDocument>,
  ) -> Result<ProtectedInputManifestV3, ApplicationError> {
    let operation = intent.operation_id().map_err(|_| ApplicationError::invalid())?;
    let namespace = Uuid::new_v5(&Uuid::NAMESPACE_OID, &operation.as_bytes());
    let mut pending = documents
      .into_iter()
      .map(|document| {
        let digest = FactoryDigest::content_sha256(&document.bytes);
        let identity = FactoryDigest::sha256(
          "octacity.factory.execution-document.v1",
          &[
            document.destination.as_bytes(),
            document.media_type.as_str().as_bytes(),
            &digest.as_bytes(),
          ],
        );
        let artifact = ArtifactId::from_uuid(Uuid::new_v5(&namespace, &identity.as_bytes()))
          .map_err(|_| ApplicationError::invalid())?;
        let upload = ArtifactUploadId::from_uuid(Uuid::new_v5(&artifact.as_uuid(), b"trusted-publication:v1"))
          .map_err(|_| ApplicationError::invalid())?;
        Ok((
          FactoryExecutionInputPublication {
            input: ProtectedInputV3 {
              artifact_id: artifact.to_string(),
              size_bytes: document.bytes.len() as u64,
              sha256: digest.to_string(),
              media_type: document.media_type.as_str().to_owned(),
              destination: document.destination,
            },
            upload_id: upload,
          },
          document.bytes,
        ))
      })
      .collect::<Result<Vec<_>, ApplicationError>>()?;
    pending.sort_by(|a, b| a.0.input.artifact_id.cmp(&b.0.input.artifact_id));
    let manifest = ProtectedInputManifestV3 {
      inputs: pending.iter().map(|(entry, _)| entry.input.clone()).collect(),
    };
    manifest.validate().map_err(|_| ApplicationError::invalid())?;
    let mut request = PublishFactoryExecutionInputs {
      intent: intent.clone(),
      ownership: ownership.clone(),
      inputs: pending.iter().map(|(entry, _)| entry.clone()).collect(),
      published_at: self.clock.now()?,
    };
    if self.store.reserve_factory_execution_inputs(request.clone()).await?
      == FactoryExecutionInputDisposition::Published
    {
      return Ok(manifest);
    }
    for (entry, bytes) in pending {
      request.published_at = self.clock.now()?;
      // Revalidate cancellation and current ownership before each physical effect.
      self.store.reserve_factory_execution_inputs(request.clone()).await?;
      let object = ArtifactObject::new(
        entry
          .input
          .artifact_id
          .parse()
          .map_err(|_| ApplicationError::invalid())?,
        entry.upload_id,
        entry.input.size_bytes,
        entry.input.sha256,
        entry.input.media_type,
      )
      .map_err(|_| ApplicationError::invalid())?;
      self
        .bytes
        .publish_verified(&object, bytes)
        .await
        .map_err(|_| ApplicationError::unavailable())?;
    }
    request.published_at = self.clock.now()?;
    self.store.publish_factory_execution_inputs(request).await?;
    Ok(manifest)
  }
}
