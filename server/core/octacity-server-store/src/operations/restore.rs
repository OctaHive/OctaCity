use async_trait::async_trait;
use octacity_server_cache::CacheBlobObject;
use octacity_server_domain::{ArtifactId, LogChunkId, Timestamp};

use crate::{ArtifactUploadRecord, LogChunkManifest, StoreError};

/// Maximum number of authoritative object references read in one restore pass.
pub const MAX_RESTORE_RECONCILIATION_BATCH_SIZE: u16 = 1_000;

/// Bounded page of visible Artifact objects that must exist after restore.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestoreArtifactPage {
  /// Published logical uploads ordered by Artifact identity.
  pub items: Vec<ArtifactUploadRecord>,
  /// Exclusive cursor for the next page.
  pub next_after: Option<ArtifactId>,
}

/// Bounded page of exact Artifacts retained by visible Factory Runs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestoreFactoryArtifactPage {
  /// Published objects ordered by Artifact identity.
  pub items: Vec<ArtifactUploadRecord>,
  /// Exclusive cursor for the next page.
  pub next_after: Option<ArtifactId>,
}

/// Result of one bounded offline Factory ownership recovery pass.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RestoreFactoryRecovery {
  /// Run claims made non-authoritative in this pass.
  pub expired_claims: u16,
  /// Factory-retention claims made non-authoritative in this pass.
  pub expired_retention_claims: u16,
  /// Claimed outbox operations returned to pending work.
  pub requeued_outbox: u16,
  /// Whether another bounded recovery pass is required.
  pub has_more: bool,
}

/// Bounded page of visible immutable Build-log chunks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestoreLogChunkPage {
  /// Committed manifests ordered by chunk identity.
  pub items: Vec<LogChunkManifest>,
  /// Exclusive cursor for the next page.
  pub next_after: Option<LogChunkId>,
}

/// Stable internal cursor for published remote-cache blobs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestoreCacheCursor {
  /// Opaque cache isolation scope.
  pub scope_id: String,
  /// Canonical descriptor key within the scope.
  pub blob_key: String,
}

/// Bounded page of published remote-cache blobs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestoreCacheBlobPage {
  /// Verified-object descriptions ordered by scope and descriptor key.
  pub items: Vec<CacheBlobObject>,
  /// Exclusive cursor for the next page.
  pub next_after: Option<RestoreCacheCursor>,
}

/// Read-only authoritative inventory used while a restored deployment is offline.
///
/// Implementations return only metadata that is logically visible. Callers
/// independently verify every referenced immutable object before allowing the
/// restored deployment to serve traffic.
#[async_trait]
pub trait RestoreInventoryStore: Send + Sync {
  /// Expires process ownership restored from a quiesced database snapshot.
  async fn recover_restored_factory_state(
    &self,
    observed_at: Timestamp,
    limit: u16,
  ) -> Result<RestoreFactoryRecovery, StoreError>;

  /// Reads one stable page of visible published Artifacts.
  async fn restore_artifact_page(
    &self,
    after: Option<ArtifactId>,
    limit: u16,
  ) -> Result<RestoreArtifactPage, StoreError>;

  /// Reads one stable page of exact Artifacts retained by visible Factory Runs.
  async fn restore_factory_artifact_page(
    &self,
    after: Option<ArtifactId>,
    limit: u16,
  ) -> Result<RestoreFactoryArtifactPage, StoreError>;

  /// Reads one stable page of visible committed Build-log chunks.
  async fn restore_log_chunk_page(
    &self,
    after: Option<LogChunkId>,
    limit: u16,
  ) -> Result<RestoreLogChunkPage, StoreError>;

  /// Reads one stable page of published remote-cache blobs.
  async fn restore_cache_blob_page(
    &self,
    after: Option<RestoreCacheCursor>,
    limit: u16,
  ) -> Result<RestoreCacheBlobPage, StoreError>;
}
