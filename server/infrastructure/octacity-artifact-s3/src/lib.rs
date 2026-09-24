//! S3-compatible artifact adapter with private immutable object namespaces.
//!
//! Agents upload artifacts only to a temporary key with a signed SHA-256
//! checksum. Completion verifies the exact object generation before copying it
//! to a key derived from the immutable artifact identity and digest, so a
//! still-live PUT capability cannot mutate a published artifact. Compatible
//! stores that omit checksums from HEAD are verified by streaming the body.
//!
//! Artifact publication, cache blobs, and Build-log chunks share one isolated
//! client and bounded object mechanics while keeping their domain contracts in
//! dedicated internal modules.

use std::{sync::Arc, time::Duration};

use aws_sdk_s3::{
  Client,
  config::{BehaviorVersion, Credentials, Region},
};
use octacity_artifact_store::ArtifactObject;
use octacity_server_artifacts::LogChunkManifest;
use octacity_server_cache::CacheBlobObject;

mod artifact;
mod cache;
mod config;
mod health;
mod immutable;
mod log;

use config::validate_config;
pub use config::{S3ArtifactStoreConfig, S3ArtifactStoreConfigError, validate_s3_settings};
pub use health::S3ArtifactStoreHealthError;

const SHA256_METADATA: &str = "octacity-sha256";
const SIZE_METADATA: &str = "octacity-size";
const MAX_PRESIGNED_LIFETIME: Duration = Duration::from_secs(60 * 60);
/// Maximum object size supported by one S3 PUT and one S3 CopyObject request.
const MAX_SINGLE_OBJECT_BYTES: u64 = 5 * 1024 * 1024 * 1024;

/// S3-compatible object store that never exposes credentials or physical keys.
#[derive(Clone)]
pub struct S3ArtifactStore {
  client: Client,
  bucket: String,
  prefix: String,
  health_probe_prefix: String,
  operation_timeout: Duration,
  capability_recheck_interval: Duration,
  capability_state: Arc<tokio::sync::Mutex<health::CapabilityState>>,
}

impl S3ArtifactStore {
  /// Validates configuration and creates an isolated S3 client.
  pub fn new(config: S3ArtifactStoreConfig) -> Result<Self, S3ArtifactStoreConfigError> {
    validate_config(&config)?;
    let credentials = Credentials::new(
      config.access_key.to_string(),
      config.secret_key.to_string(),
      None,
      None,
      "octacity-server",
    );
    let sdk_config = aws_sdk_s3::Config::builder()
      .behavior_version(BehaviorVersion::latest())
      .endpoint_url(config.endpoint)
      .region(Region::new(config.region))
      .credentials_provider(credentials)
      .force_path_style(config.force_path_style)
      .build();
    let health_probe_prefix = physical_key(
      &config.prefix,
      &format!("health/readiness-{}", uuid::Uuid::new_v4().simple()),
    );
    Ok(Self {
      client: Client::from_conf(sdk_config),
      bucket: config.bucket,
      prefix: config.prefix,
      health_probe_prefix,
      operation_timeout: config.operation_timeout,
      capability_recheck_interval: config.capability_recheck_interval,
      capability_state: Arc::new(tokio::sync::Mutex::new(health::CapabilityState::default())),
    })
  }

  fn pending_key(&self, object: &ArtifactObject) -> String {
    self.key(&format!("uploads/{}", object.upload_id()))
  }

  fn published_key(&self, object: &ArtifactObject) -> String {
    self.key(&format!("objects/{}/{}", object.artifact_id(), object.sha256()))
  }

  fn cache_key(&self, object: &CacheBlobObject) -> String {
    let descriptor = object.descriptor();
    let encoding = match descriptor.encoding {
      octacity_server_cache::BlobEncoding::Identity => "identity",
      octacity_server_cache::BlobEncoding::ZstdV1 => "zstd-v1",
    };
    self.key(&format!(
      "cache/{}/blobs/blake3/{}/{}/{encoding}/{}/{}",
      object.scope_id(),
      descriptor.digest.hex(),
      descriptor.expanded_size_bytes,
      descriptor.encoded_size_bytes,
      descriptor.entry_count,
    ))
  }

  fn log_chunk_key(&self, manifest: &LogChunkManifest) -> String {
    self.key(&format!("logs/chunks/{}/{}", manifest.chunk_id(), manifest.digest()))
  }

  fn key(&self, suffix: &str) -> String {
    physical_key(&self.prefix, suffix)
  }
}

fn physical_key(prefix: &str, suffix: &str) -> String {
  if prefix.is_empty() {
    suffix.to_owned()
  } else {
    format!("{prefix}/{suffix}")
  }
}

#[cfg(test)]
mod tests;
