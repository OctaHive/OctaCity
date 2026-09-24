//! Transport-independent coordination for Octa's HTTP L2 cache protocol.

use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_cache::{
  ActionResultV1, BlobDescriptor, CacheBlobStore, CacheBlobStoreError, CacheBlobWrite, CacheCredential, CacheNamespace,
  Digest, FindMissingBlobsRequestV1, FindMissingBlobsResponseV1, REMOTE_CACHE_PROTOCOL_V1, WriteActionRequestV1,
};
use octacity_server_domain::Timestamp;
use octacity_server_store::{
  CacheBlobPreparationOutcome, CacheDataAccess, CacheDataStore, CachePublicationOutcome, PublishCacheAction,
  PublishCacheBlob, StoreError,
};
use thiserror::Error;

/// Authenticated request facts supplied by the HTTP adapter.
#[derive(Clone, Debug)]
pub struct CacheRequestAuthority {
  /// Opaque bearer token received through the Authorization header.
  pub bearer_token: String,
  /// Exact logical namespace from the protocol request.
  pub namespace: Option<String>,
  /// Server-observed request arrival time.
  pub observed_at_unix_ms: i64,
}

/// Create-if-absent result mapped directly to Octa's HTTP statuses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheWriteResult {
  /// This call published new immutable metadata.
  Written,
  /// Equivalent immutable metadata was already published.
  AlreadyPresent,
}

/// Application boundary consumed by the cache HTTP adapter.
#[async_trait]
pub trait CacheDataPlaneUseCases: Send + Sync {
  /// Finds absent physical blob representations in request order.
  async fn find_missing(
    &self,
    authority: CacheRequestAuthority,
    request: FindMissingBlobsRequestV1,
  ) -> Result<FindMissingBlobsResponseV1, CacheDataPlaneError>;

  /// Reads one verified immutable blob.
  async fn read_blob(
    &self,
    authority: CacheRequestAuthority,
    blob: BlobDescriptor,
  ) -> Result<Option<Vec<u8>>, CacheDataPlaneError>;

  /// Verifies and atomically publishes one immutable blob.
  async fn write_blob(
    &self,
    authority: CacheRequestAuthority,
    blob: BlobDescriptor,
    bytes: Vec<u8>,
  ) -> Result<CacheWriteResult, CacheDataPlaneError>;

  /// Reads one opaque action result.
  async fn read_action(
    &self,
    authority: CacheRequestAuthority,
    action: Digest,
  ) -> Result<Option<ActionResultV1>, CacheDataPlaneError>;

  /// Publishes one opaque action result after its blob is visible.
  async fn write_action(
    &self,
    authority: CacheRequestAuthority,
    action: Digest,
    request: WriteActionRequestV1,
  ) -> Result<CacheWriteResult, CacheDataPlaneError>;
}

/// Store- and byte-adapter-backed L2 cache coordinator.
pub struct CacheDataPlaneService<S, B> {
  metadata: Arc<S>,
  blobs: Arc<B>,
  max_blob_bytes: u64,
}

impl<S, B> CacheDataPlaneService<S, B> {
  /// Builds a cache coordinator with an explicit encoded-blob ceiling.
  pub fn new(metadata: Arc<S>, blobs: Arc<B>, max_blob_bytes: u64) -> Result<Self, CacheDataPlaneError> {
    if max_blob_bytes == 0 || max_blob_bytes > usize::MAX as u64 {
      return Err(CacheDataPlaneError::InvalidRequest);
    }
    Ok(Self {
      metadata,
      blobs,
      max_blob_bytes,
    })
  }

  async fn access(&self, authority: &CacheRequestAuthority) -> Result<CacheDataAccess, CacheDataPlaneError>
  where
    S: CacheDataStore,
  {
    let credential =
      CacheCredential::from_token(&authority.bearer_token).map_err(|_| CacheDataPlaneError::AuthorizationRejected)?;
    let observed_at =
      Timestamp::from_unix_millis(authority.observed_at_unix_ms).map_err(|_| CacheDataPlaneError::InvalidRequest)?;
    let credential_digest = credential.digest();
    let namespace = match &authority.namespace {
      Some(value) => CacheNamespace::new(value.clone()).map_err(|_| CacheDataPlaneError::InvalidRequest)?,
      None => {
        self
          .metadata
          .resolve_cache_namespace(credential_digest, observed_at)
          .await?
      }
    };
    Ok(CacheDataAccess {
      credential_digest,
      namespace,
      observed_at,
    })
  }

  async fn prune(&self, access: CacheDataAccess) -> Result<(), CacheDataPlaneError>
  where
    S: CacheDataStore,
    B: CacheBlobStore,
  {
    let outcome = self.metadata.prune_cache(access).await?;
    for blob in outcome.deleted_blobs {
      self.blobs.delete(&blob).await?;
    }
    Ok(())
  }
}

#[async_trait]
impl<S, B> CacheDataPlaneUseCases for CacheDataPlaneService<S, B>
where
  S: CacheDataStore + 'static,
  B: CacheBlobStore + 'static,
{
  async fn find_missing(
    &self,
    authority: CacheRequestAuthority,
    request: FindMissingBlobsRequestV1,
  ) -> Result<FindMissingBlobsResponseV1, CacheDataPlaneError> {
    request.validate().map_err(|_| CacheDataPlaneError::InvalidRequest)?;
    let access = self.access(&authority).await?;
    self.prune(access.clone()).await?;
    let missing = self.metadata.find_missing_cache_blobs(access, request.blobs).await?;
    Ok(FindMissingBlobsResponseV1 {
      protocol_version: REMOTE_CACHE_PROTOCOL_V1,
      missing,
    })
  }

  async fn read_blob(
    &self,
    authority: CacheRequestAuthority,
    blob: BlobDescriptor,
  ) -> Result<Option<Vec<u8>>, CacheDataPlaneError> {
    blob.validate().map_err(|_| CacheDataPlaneError::InvalidRequest)?;
    let access = self.access(&authority).await?;
    self.prune(access.clone()).await?;
    let Some(object) = self.metadata.cache_blob(access, blob).await? else {
      return Ok(None);
    };
    match self.blobs.read(&object).await {
      Ok(bytes) => Ok(Some(bytes)),
      Err(CacheBlobStoreError::NotFound | CacheBlobStoreError::Integrity(_)) => Err(CacheDataPlaneError::Integrity),
      Err(CacheBlobStoreError::Unavailable) => Err(CacheDataPlaneError::Unavailable),
    }
  }

  async fn write_blob(
    &self,
    authority: CacheRequestAuthority,
    blob: BlobDescriptor,
    bytes: Vec<u8>,
  ) -> Result<CacheWriteResult, CacheDataPlaneError> {
    blob.validate().map_err(|_| CacheDataPlaneError::InvalidRequest)?;
    if blob.encoded_size_bytes > self.max_blob_bytes || bytes.len() as u64 > self.max_blob_bytes {
      return Err(CacheDataPlaneError::PayloadTooLarge);
    }
    let access = self.access(&authority).await?;
    self.prune(access.clone()).await?;
    let object = match self.metadata.prepare_cache_blob(access.clone(), blob.clone()).await? {
      CacheBlobPreparationOutcome::Upload(object) => object,
      CacheBlobPreparationOutcome::AlreadyPresent => return Ok(CacheWriteResult::AlreadyPresent),
      CacheBlobPreparationOutcome::QuotaExceeded => return Err(CacheDataPlaneError::QuotaExceeded),
    };
    let byte_outcome = self.blobs.put_if_absent(&object, bytes).await?;
    let metadata_outcome = self
      .metadata
      .publish_cache_blob(PublishCacheBlob { access, blob })
      .await?;
    map_publication(metadata_outcome, byte_outcome)
  }

  async fn read_action(
    &self,
    authority: CacheRequestAuthority,
    action: Digest,
  ) -> Result<Option<ActionResultV1>, CacheDataPlaneError> {
    let access = self.access(&authority).await?;
    self.prune(access.clone()).await?;
    self.metadata.cache_action(access, action).await.map_err(Into::into)
  }

  async fn write_action(
    &self,
    authority: CacheRequestAuthority,
    action: Digest,
    request: WriteActionRequestV1,
  ) -> Result<CacheWriteResult, CacheDataPlaneError> {
    request.validate().map_err(|_| CacheDataPlaneError::InvalidRequest)?;
    if authority.namespace.as_deref() != Some(request.namespace.as_str()) || request.result.action != action {
      return Err(CacheDataPlaneError::InvalidRequest);
    }
    let wire_size_bytes = serde_json::to_vec(&request.result)
      .map_err(|_| CacheDataPlaneError::InvalidRequest)?
      .len() as u64;
    let access = self.access(&authority).await?;
    self.prune(access.clone()).await?;
    let outcome = self
      .metadata
      .publish_cache_action(PublishCacheAction {
        access,
        action,
        result: request.result,
        wire_size_bytes,
      })
      .await?;
    map_publication(outcome, CacheBlobWrite::Written)
  }
}

fn map_publication(
  metadata: CachePublicationOutcome,
  bytes: CacheBlobWrite,
) -> Result<CacheWriteResult, CacheDataPlaneError> {
  match metadata {
    CachePublicationOutcome::Published => Ok(CacheWriteResult::Written),
    CachePublicationOutcome::AlreadyPresent => Ok(CacheWriteResult::AlreadyPresent),
    CachePublicationOutcome::Conflict => Err(CacheDataPlaneError::Conflict),
    CachePublicationOutcome::QuotaExceeded => Err(CacheDataPlaneError::QuotaExceeded),
  }
  .map(|outcome| match (outcome, bytes) {
    (CacheWriteResult::Written, CacheBlobWrite::AlreadyPresent) => CacheWriteResult::Written,
    (outcome, _) => outcome,
  })
}

/// Stable transport-independent cache data-plane failure.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum CacheDataPlaneError {
  /// Protocol metadata, route identity, or time is invalid.
  #[error("cache request is invalid")]
  InvalidRequest,
  /// The presented bearer has no current authority for the request.
  #[error("cache authorization was rejected")]
  AuthorizationRejected,
  /// The bounded blob ceiling was exceeded.
  #[error("cache payload is too large")]
  PayloadTooLarge,
  /// Immutable metadata or bytes conflict with an existing object.
  #[error("cache publication conflicts with an existing object")]
  Conflict,
  /// Publication would exceed Project cache quota.
  #[error("cache quota is exhausted")]
  QuotaExceeded,
  /// Published metadata does not have matching verified bytes.
  #[error("cache object integrity failed")]
  Integrity,
  /// A required adapter is unavailable.
  #[error("cache data plane is unavailable")]
  Unavailable,
}

impl From<StoreError> for CacheDataPlaneError {
  fn from(error: StoreError) -> Self {
    match error {
      StoreError::CredentialRejected => Self::AuthorizationRejected,
      StoreError::InvalidInput { .. } => Self::InvalidRequest,
      StoreError::Conflict { .. } | StoreError::Duplicate { .. } => Self::Conflict,
      StoreError::Unavailable => Self::Unavailable,
      _ => Self::Unavailable,
    }
  }
}

impl From<CacheBlobStoreError> for CacheDataPlaneError {
  fn from(error: CacheBlobStoreError) -> Self {
    match error {
      CacheBlobStoreError::Integrity(_) => Self::Integrity,
      CacheBlobStoreError::NotFound | CacheBlobStoreError::Unavailable => Self::Unavailable,
    }
  }
}
