use std::sync::Arc;

use octacity_artifact_store::{ArtifactObject, ArtifactStore, ArtifactStoreError, LogChunkStore, LogChunkStoreError};
use octacity_server_cache::{CacheBlobStore, CacheBlobStoreError};
use octacity_server_store::{MAX_RESTORE_RECONCILIATION_BATCH_SIZE, RestoreInventoryStore, StoreError};
use thiserror::Error;

/// Counts independently verified object references in one restored snapshot.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RestoreReconciliationSummary {
  /// Published Artifact and report objects.
  pub artifacts: u64,
  /// Visible immutable Build-log chunks.
  pub log_chunks: u64,
  /// Published remote-cache blobs.
  pub cache_blobs: u64,
}

/// Coordinates authoritative restored metadata with a backend-neutral byte store.
pub struct RestoreReconciler<S, B> {
  inventory: Arc<S>,
  bytes: Arc<B>,
  batch_size: u16,
}

impl<S, B> RestoreReconciler<S, B> {
  /// Creates a bounded reconciliation pass.
  pub fn new(inventory: Arc<S>, bytes: Arc<B>, batch_size: u16) -> Result<Self, RestoreReconciliationError> {
    if batch_size == 0 || batch_size > MAX_RESTORE_RECONCILIATION_BATCH_SIZE {
      return Err(RestoreReconciliationError::InvalidBatchSize);
    }
    Ok(Self {
      inventory,
      bytes,
      batch_size,
    })
  }
}

impl<S, B> RestoreReconciler<S, B>
where
  S: RestoreInventoryStore + 'static,
  B: ArtifactStore + LogChunkStore + CacheBlobStore + 'static,
{
  /// Verifies every visible object referenced by the restored authoritative store.
  ///
  /// The deployment must remain offline for the complete pass so its stable
  /// cursor pages describe one quiesced consistency unit.
  pub async fn reconcile(&self) -> Result<RestoreReconciliationSummary, RestoreReconciliationError> {
    let mut summary = RestoreReconciliationSummary::default();
    let mut after = None;
    loop {
      let page = self.inventory.restore_artifact_page(after, self.batch_size).await?;
      for upload in page.items {
        let identity = upload.artifact.identity();
        let object = ArtifactObject::new(
          identity.artifact_id,
          upload.upload_id,
          identity.size_bytes,
          identity.digest.to_string(),
          upload.transport_media_type.as_str(),
        )
        .map_err(|_| RestoreReconciliationError::InvalidInventory)?;
        self.bytes.verify_published(&object).await?;
        summary.artifacts = checked_increment(summary.artifacts)?;
      }
      match page.next_after {
        Some(next) if Some(next) != after => after = Some(next),
        Some(_) => return Err(RestoreReconciliationError::InvalidInventory),
        None => break,
      }
    }

    let mut after = None;
    loop {
      let page = self.inventory.restore_log_chunk_page(after, self.batch_size).await?;
      for manifest in page.items {
        self.bytes.read_verified(&manifest).await?;
        summary.log_chunks = checked_increment(summary.log_chunks)?;
      }
      match page.next_after {
        Some(next) if Some(next) != after => after = Some(next),
        Some(_) => return Err(RestoreReconciliationError::InvalidInventory),
        None => break,
      }
    }

    let mut after = None;
    loop {
      let page = self
        .inventory
        .restore_cache_blob_page(after.clone(), self.batch_size)
        .await?;
      for object in page.items {
        self.bytes.read(&object).await?;
        summary.cache_blobs = checked_increment(summary.cache_blobs)?;
      }
      match page.next_after {
        Some(next) if Some(&next) != after.as_ref() => after = Some(next),
        Some(_) => return Err(RestoreReconciliationError::InvalidInventory),
        None => break,
      }
    }
    Ok(summary)
  }
}

fn checked_increment(value: u64) -> Result<u64, RestoreReconciliationError> {
  value.checked_add(1).ok_or(RestoreReconciliationError::InvalidInventory)
}

/// Safe failure from restored object reconciliation.
#[derive(Debug, Error)]
pub enum RestoreReconciliationError {
  /// The requested page size is zero or exceeds the contract ceiling.
  #[error("restore reconciliation batch size is invalid")]
  InvalidBatchSize,
  /// Authoritative metadata is malformed or pagination did not advance.
  #[error("restored authoritative object inventory is invalid")]
  InvalidInventory,
  /// PostgreSQL could not provide the authoritative inventory.
  #[error("restored authoritative object inventory is unavailable")]
  Store(#[from] StoreError),
  /// Published Artifact or report bytes are absent or fail integrity verification.
  #[error("restored Artifact object failed reconciliation")]
  Artifact(#[from] ArtifactStoreError),
  /// A visible immutable Build-log chunk is absent or corrupt.
  #[error("restored Build-log object failed reconciliation")]
  LogChunk(#[from] LogChunkStoreError),
  /// A published remote-cache blob is absent or corrupt.
  #[error("restored remote-cache object failed reconciliation")]
  CacheBlob(#[from] CacheBlobStoreError),
}

#[cfg(test)]
mod tests {
  use std::{
    collections::BTreeMap,
    fmt::Debug,
    future::Future,
    str::FromStr,
    task::{Context, Poll, Waker},
    time::Duration,
  };

  use async_trait::async_trait;
  use octacity_artifact_store::{DownloadAuthorization, LogChunkWrite, UploadAuthorization};
  use octacity_server_cache::{BlobDescriptor, BlobEncoding, CacheBlobObject, CacheBlobWrite, Digest};
  use octacity_server_domain::{ArtifactId, ArtifactName, LogChunkId, Timestamp};
  use octacity_server_store::{
    ArtifactContentDigest, ArtifactEvent, ArtifactIdentity, ArtifactMediaType, ArtifactRecord, ArtifactRetentionPolicy,
    ArtifactType, ArtifactUploadRecord, BuildLogStream, IdempotencyKey, LogChunkManifest, RestoreArtifactPage,
    RestoreCacheBlobPage, RestoreCacheCursor, RestoreLogChunkPage,
  };

  use super::*;

  struct Inventory {
    artifact: ArtifactUploadRecord,
    log_chunk: LogChunkManifest,
    cache: CacheBlobObject,
  }

  #[async_trait]
  impl RestoreInventoryStore for Inventory {
    async fn restore_artifact_page(
      &self,
      after: Option<ArtifactId>,
      _limit: u16,
    ) -> Result<RestoreArtifactPage, StoreError> {
      Ok(RestoreArtifactPage {
        items: after.is_none().then(|| self.artifact.clone()).into_iter().collect(),
        next_after: None,
      })
    }

    async fn restore_log_chunk_page(
      &self,
      after: Option<LogChunkId>,
      _limit: u16,
    ) -> Result<RestoreLogChunkPage, StoreError> {
      Ok(RestoreLogChunkPage {
        items: after.is_none().then(|| self.log_chunk.clone()).into_iter().collect(),
        next_after: None,
      })
    }

    async fn restore_cache_blob_page(
      &self,
      after: Option<RestoreCacheCursor>,
      _limit: u16,
    ) -> Result<RestoreCacheBlobPage, StoreError> {
      Ok(RestoreCacheBlobPage {
        items: after.is_none().then(|| self.cache.clone()).into_iter().collect(),
        next_after: None,
      })
    }
  }

  #[derive(Clone, Copy, Eq, PartialEq)]
  enum MissingObject {
    Artifact,
    LogChunk,
    CacheBlob,
  }

  struct Bytes {
    missing: Option<MissingObject>,
  }

  #[async_trait]
  impl ArtifactStore for Bytes {
    async fn authorize_upload(
      &self,
      _object: &ArtifactObject,
      _expires_in: Duration,
    ) -> Result<UploadAuthorization, ArtifactStoreError> {
      Ok(UploadAuthorization {
        url: String::new(),
        required_headers: BTreeMap::new(),
        expires_in: Duration::from_secs(1),
      })
    }

    async fn complete_upload(&self, _object: &ArtifactObject) -> Result<(), ArtifactStoreError> {
      Ok(())
    }

    async fn verify_published(&self, _object: &ArtifactObject) -> Result<(), ArtifactStoreError> {
      if self.missing == Some(MissingObject::Artifact) {
        Err(ArtifactStoreError::NotFound)
      } else {
        Ok(())
      }
    }

    async fn authorize_download(
      &self,
      _object: &ArtifactObject,
      _expires_in: Duration,
    ) -> Result<DownloadAuthorization, ArtifactStoreError> {
      unreachable!()
    }

    async fn delete(&self, _object: &ArtifactObject) -> Result<(), ArtifactStoreError> {
      unreachable!()
    }
  }

  #[async_trait]
  impl LogChunkStore for Bytes {
    async fn put_verified(
      &self,
      _manifest: &octacity_server_store::LogChunkManifest,
      _bytes: Vec<u8>,
    ) -> Result<LogChunkWrite, LogChunkStoreError> {
      unreachable!()
    }

    async fn read_verified(
      &self,
      _manifest: &octacity_server_store::LogChunkManifest,
    ) -> Result<Vec<u8>, LogChunkStoreError> {
      if self.missing == Some(MissingObject::LogChunk) {
        Err(LogChunkStoreError::NotFound)
      } else {
        Ok(b"log".to_vec())
      }
    }

    async fn delete_chunk(
      &self,
      _manifest: &octacity_server_store::LogChunkManifest,
    ) -> Result<(), LogChunkStoreError> {
      unreachable!()
    }
  }

  #[async_trait]
  impl CacheBlobStore for Bytes {
    async fn put_if_absent(
      &self,
      _object: &CacheBlobObject,
      _bytes: Vec<u8>,
    ) -> Result<CacheBlobWrite, CacheBlobStoreError> {
      unreachable!()
    }

    async fn read(&self, object: &CacheBlobObject) -> Result<Vec<u8>, CacheBlobStoreError> {
      if self.missing == Some(MissingObject::CacheBlob) {
        Err(CacheBlobStoreError::NotFound)
      } else {
        Ok(vec![0; object.descriptor().encoded_size_bytes as usize])
      }
    }

    async fn delete(&self, _object: &CacheBlobObject) -> Result<(), CacheBlobStoreError> {
      unreachable!()
    }
  }

  #[test]
  fn restored_metadata_is_accepted_only_after_object_verification() {
    let inventory = Arc::new(inventory());
    let reconciler = RestoreReconciler::new(inventory.clone(), Arc::new(Bytes { missing: None }), 8).unwrap();
    let valid = run_ready(reconciler.reconcile()).unwrap();
    assert_eq!(
      valid,
      RestoreReconciliationSummary {
        artifacts: 1,
        log_chunks: 1,
        cache_blobs: 1,
      }
    );

    for (missing, label) in [
      (MissingObject::Artifact, "artifact"),
      (MissingObject::LogChunk, "log"),
      (MissingObject::CacheBlob, "cache"),
    ] {
      let reconciler =
        RestoreReconciler::new(inventory.clone(), Arc::new(Bytes { missing: Some(missing) }), 8).unwrap();
      let error = run_ready(reconciler.reconcile()).unwrap_err();
      assert!(
        matches!(
          (&error, missing),
          (
            RestoreReconciliationError::Artifact(ArtifactStoreError::NotFound),
            MissingObject::Artifact
          ) | (
            RestoreReconciliationError::LogChunk(LogChunkStoreError::NotFound),
            MissingObject::LogChunk
          ) | (
            RestoreReconciliationError::CacheBlob(CacheBlobStoreError::NotFound),
            MissingObject::CacheBlob
          )
        ),
        "missing {label} object returned {error:?}"
      );
    }
  }

  #[test]
  fn reconciliation_batch_is_bounded() {
    assert!(matches!(
      RestoreReconciler::<Inventory, Bytes>::new(Arc::new(inventory()), Arc::new(Bytes { missing: None }), 0,),
      Err(RestoreReconciliationError::InvalidBatchSize)
    ));
  }

  fn inventory() -> Inventory {
    Inventory {
      artifact: artifact_upload(),
      log_chunk: LogChunkManifest::prepare(id(4), BuildLogStream::Stdout, 1, 1, b"log").unwrap(),
      cache: cache_object(),
    }
  }

  fn artifact_upload() -> ArtifactUploadRecord {
    let identity = ArtifactIdentity {
      artifact_id: id(1),
      build_id: id(2),
      attempt_id: id(3),
      job_id: id(4),
      lease_id: id(5),
      logical_name: ArtifactName::new("dist/result").unwrap(),
      artifact_type: ArtifactType::Artifact,
      media_type: ArtifactMediaType::new("application/octet-stream").unwrap(),
      size_bytes: 4,
      digest: ArtifactContentDigest::from_bytes([1; 32]),
      retention: ArtifactRetentionPolicy::Keep,
    };
    let pending = ArtifactRecord::pending(identity.clone(), timestamp(1)).unwrap();
    let verifying = pending
      .transition(
        &identity,
        pending.version(),
        ArtifactEvent::BeginVerification,
        timestamp(2),
      )
      .unwrap();
    let artifact = verifying
      .transition(&identity, verifying.version(), ArtifactEvent::Publish, timestamp(3))
      .unwrap();
    ArtifactUploadRecord {
      upload_id: id(6),
      idempotency_key: IdempotencyKey::new("restore:artifact").unwrap(),
      artifact,
      producer_run_id: 1,
      producer_task_id: 1,
      transport_media_type: ArtifactMediaType::new("application/octet-stream").unwrap(),
      capability_expires_at: timestamp(10),
    }
  }

  fn cache_object() -> CacheBlobObject {
    let bytes = [0_u8; 4];
    CacheBlobObject::new(
      "a".repeat(64),
      BlobDescriptor {
        digest: Digest::blake3(&bytes),
        encoding: BlobEncoding::Identity,
        encoded_size_bytes: bytes.len() as u64,
        expanded_size_bytes: bytes.len() as u64,
        entry_count: 1,
      },
    )
    .unwrap()
  }

  fn timestamp(value: i64) -> Timestamp {
    Timestamp::from_unix_millis(value).unwrap()
  }

  fn id<T>(value: u64) -> T
  where
    T: FromStr,
    T::Err: Debug,
  {
    format!("00000000-0000-0000-0000-{value:012x}").parse().unwrap()
  }

  fn run_ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
      Poll::Ready(value) => value,
      Poll::Pending => panic!("in-memory restore test unexpectedly awaited external I/O"),
    }
  }
}
