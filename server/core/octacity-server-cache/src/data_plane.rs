//! Backend-neutral immutable cache-blob boundary and integrity checks.

use std::io::Read as _;

use async_trait::async_trait;
use octa_cache_protocol::{BlobDescriptor, BlobEncoding, Digest, DigestAlgorithm, ZSTD_V1_MAX_WINDOW_LOG};
use thiserror::Error;

const VERIFICATION_BUFFER_BYTES: usize = 1024 * 1024;

/// Physical cache object identity without exposing an adapter key or bucket.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CacheBlobObject {
  scope_id: String,
  descriptor: BlobDescriptor,
}

impl CacheBlobObject {
  /// Creates an object in one opaque Project/namespace scope.
  pub fn new(scope_id: impl Into<String>, descriptor: BlobDescriptor) -> Result<Self, &'static str> {
    let scope_id = scope_id.into();
    if scope_id.len() != 64
      || !scope_id
        .bytes()
        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
      return Err("invalid cache scope identity");
    }
    descriptor.validate().map_err(|_| "invalid cache blob descriptor")?;
    Ok(Self { scope_id, descriptor })
  }

  /// Borrows the opaque isolation scope.
  #[must_use]
  pub fn scope_id(&self) -> &str {
    &self.scope_id
  }

  /// Borrows the exact physical representation descriptor.
  #[must_use]
  pub const fn descriptor(&self) -> &BlobDescriptor {
    &self.descriptor
  }
}

/// Result of an immutable create-if-absent blob write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheBlobWrite {
  /// This call created the object.
  Written,
  /// The same verified bytes were already present.
  AlreadyPresent,
}

/// Stable blob-integrity rejection.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum CacheIntegrityError {
  /// Encoded transfer length differs from the descriptor.
  #[error("encoded cache blob size differs from its descriptor")]
  EncodedSize,
  /// The Zstandard representation cannot be decoded within the protocol bound.
  #[error("cache blob has an invalid Zstandard representation")]
  Encoding,
  /// Expanded content identity differs from the descriptor.
  #[error("expanded cache blob digest or size differs from its descriptor")]
  Digest,
}

/// Failure from an immutable cache byte-store operation.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum CacheBlobStoreError {
  /// The requested object does not exist.
  #[error("cache blob was not found")]
  NotFound,
  /// Existing bytes or supplied bytes fail independent integrity verification.
  #[error("cache blob integrity check failed: {0}")]
  Integrity(#[from] CacheIntegrityError),
  /// The concrete byte store cannot currently complete the operation.
  #[error("cache blob store is unavailable")]
  Unavailable,
}

/// Immutable bytes behind the cache-specific physical layout.
#[async_trait]
pub trait CacheBlobStore: Send + Sync {
  /// Creates the exact object or confirms that identical verified bytes exist.
  async fn put_if_absent(
    &self,
    object: &CacheBlobObject,
    bytes: Vec<u8>,
  ) -> Result<CacheBlobWrite, CacheBlobStoreError>;

  /// Reads and independently verifies one exact physical representation.
  async fn read(&self, object: &CacheBlobObject) -> Result<Vec<u8>, CacheBlobStoreError>;

  /// Idempotently removes one physical representation during retention cleanup.
  async fn delete(&self, object: &CacheBlobObject) -> Result<(), CacheBlobStoreError>;
}

/// Verifies encoded bytes against the published semantic digest and size.
pub fn verify_blob_bytes(descriptor: &BlobDescriptor, bytes: &[u8]) -> Result<(), CacheIntegrityError> {
  if bytes.len() as u64 != descriptor.encoded_size_bytes {
    return Err(CacheIntegrityError::EncodedSize);
  }
  let mut reader: Box<dyn std::io::Read + '_> = match descriptor.encoding {
    BlobEncoding::Identity => Box::new(bytes),
    BlobEncoding::ZstdV1 => {
      let mut decoder = zstd::stream::read::Decoder::new(bytes).map_err(|_| CacheIntegrityError::Encoding)?;
      decoder
        .window_log_max(ZSTD_V1_MAX_WINDOW_LOG)
        .map_err(|_| CacheIntegrityError::Encoding)?;
      Box::new(decoder)
    }
  };
  let mut hasher = blake3::Hasher::new();
  let mut expanded = 0_u64;
  let mut buffer = vec![0_u8; VERIFICATION_BUFFER_BYTES];
  loop {
    let read = reader.read(&mut buffer).map_err(|_| CacheIntegrityError::Encoding)?;
    if read == 0 {
      break;
    }
    expanded = expanded.checked_add(read as u64).ok_or(CacheIntegrityError::Digest)?;
    if expanded > descriptor.expanded_size_bytes {
      return Err(CacheIntegrityError::Digest);
    }
    hasher.update(&buffer[..read]);
  }
  let actual = Digest::new(DigestAlgorithm::Blake3, *hasher.finalize().as_bytes(), expanded);
  if actual != descriptor.digest {
    return Err(CacheIntegrityError::Digest);
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn descriptor(bytes: &[u8], encoding: BlobEncoding, encoded_size_bytes: u64) -> BlobDescriptor {
    BlobDescriptor {
      digest: Digest::blake3(bytes),
      encoding,
      encoded_size_bytes,
      expanded_size_bytes: bytes.len() as u64,
      entry_count: 1,
    }
  }

  #[test]
  fn verifies_identity_and_zstd_representations() {
    let expanded = b"octacity-cache-bundle";
    verify_blob_bytes(
      &descriptor(expanded, BlobEncoding::Identity, expanded.len() as u64),
      expanded,
    )
    .unwrap();

    let encoded = zstd::stream::encode_all(expanded.as_slice(), 3).unwrap();
    verify_blob_bytes(
      &descriptor(expanded, BlobEncoding::ZstdV1, encoded.len() as u64),
      &encoded,
    )
    .unwrap();
  }

  #[test]
  fn rejects_wrong_physical_or_semantic_identity() {
    let bytes = b"expected";
    let expected = descriptor(bytes, BlobEncoding::Identity, bytes.len() as u64);
    assert_eq!(
      verify_blob_bytes(&expected, b"short"),
      Err(CacheIntegrityError::EncodedSize)
    );
    assert_eq!(
      verify_blob_bytes(&expected, b"mismatch"),
      Err(CacheIntegrityError::Digest)
    );
  }
}
