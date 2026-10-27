//! External port fixtures for the public protected-input transfer boundary.
use async_trait::async_trait;
use octacity_artifact_store::*;
use octacity_server_application::*;
use std::{sync::Arc, time::Duration};

struct Registrations;
#[async_trait]
impl AgentRegistrationUseCases for Registrations {
  async fn register(&self, _: AgentRegistrationInput) -> Result<AgentRegistrationOutcome, AgentRegistrationError> {
    unreachable!("protected transfer does not register Agents")
  }
  async fn authorize(&self, _: AuthorizeAgentInput) -> Result<AuthorizedAgent, AgentRegistrationError> {
    unreachable!("input manifest has already been copied from the committed Lease")
  }
}
struct Bytes;
#[async_trait]
impl ArtifactStore for Bytes {
  async fn authorize_upload(&self, _: &ArtifactObject, _: Duration) -> Result<UploadAuthorization, ArtifactStoreError> {
    unreachable!()
  }
  async fn complete_upload(&self, _: &ArtifactObject) -> Result<(), ArtifactStoreError> {
    unreachable!()
  }
  async fn verify_published(&self, object: &ArtifactObject) -> Result<(), ArtifactStoreError> {
    assert_eq!(object.size_bytes(), 5);
    assert_eq!(
      object.sha256(),
      "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    );
    Ok(())
  }
  async fn authorize_download(
    &self,
    _: &ArtifactObject,
    expires_in: Duration,
  ) -> Result<DownloadAuthorization, ArtifactStoreError> {
    Ok(DownloadAuthorization {
      url: "https://objects.example.test/input?signature=private".to_owned(),
      expires_in,
    })
  }
  async fn delete(&self, object: &ArtifactObject) -> Result<(), ArtifactStoreError> {
    assert_eq!(object.size_bytes(), 5);
    Ok(())
  }
}
pub fn retention_bytes() -> Arc<dyn ArtifactStore> {
  Arc::new(Bytes)
}
pub fn service<S: octacity_server_store::ArtifactRecordStore + 'static>(
  store: Arc<S>,
) -> Arc<dyn AgentArtifactTransferUseCases> {
  service_with_bytes(store, Arc::new(Bytes))
}
pub fn service_with_bytes<S: octacity_server_store::ArtifactRecordStore + 'static, B: ArtifactStore + 'static>(
  store: Arc<S>,
  bytes: Arc<B>,
) -> Arc<dyn AgentArtifactTransferUseCases> {
  Arc::new(
    ArtifactHandlers::new(
      Arc::new(Registrations),
      store,
      bytes,
      Duration::from_secs(30),
      Duration::from_secs(30),
    )
    .unwrap(),
  )
}
