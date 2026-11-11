//! External immutable byte port records access and can lose a publication acknowledgement.
use octacity_artifact_store::{
  ArtifactBytePublisher, ArtifactByteReader, ArtifactObject, ArtifactStoreError, ArtifactStoreOperation,
};
use octacity_server_domain::ArtifactId;
use std::sync::{
  Mutex,
  atomic::{AtomicBool, Ordering},
};

#[derive(Default)]
pub(super) struct Bytes {
  inner: crate::management::tests::project_text_intake_tests::DocumentBytes,
  pub(super) reads: Mutex<Vec<ArtifactId>>,
  pub(super) lose_ack: AtomicBool,
  pub(super) corrupt: Mutex<Option<ArtifactId>>,
}
#[async_trait::async_trait]
impl ArtifactBytePublisher for Bytes {
  async fn publish_verified(&self, object: &ArtifactObject, bytes: Vec<u8>) -> Result<(), ArtifactStoreError> {
    self.inner.publish_verified(object, bytes).await?;
    if self.lose_ack.swap(false, Ordering::SeqCst) {
      return Err(ArtifactStoreError::Backend {
        operation: ArtifactStoreOperation::PublishVerified,
      });
    }
    Ok(())
  }
}
#[async_trait::async_trait]
impl ArtifactByteReader for Bytes {
  async fn read_published(&self, object: &ArtifactObject, maximum: u64) -> Result<Vec<u8>, ArtifactStoreError> {
    self.reads.lock().unwrap().push(object.artifact_id());
    if *self.corrupt.lock().unwrap() == Some(object.artifact_id()) {
      return Ok(b"CORRUPTED_BYTES".to_vec());
    }
    self.inner.read_published(object, maximum).await
  }
}
