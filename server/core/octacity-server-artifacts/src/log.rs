use std::fmt;

use octacity_server_domain::{JobId, LogChunkId, LogIndexingWorkId};
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use uuid::Uuid;

/// Maximum bytes stored in one immutable Build-log object.
pub const MAX_LOG_CHUNK_BYTES: usize = 256 * 1_024;

const CHUNK_NAMESPACE: Uuid = Uuid::from_u128(0x514a_903d_0681_5f0e_a199_e9d7_21d5_43f0);
const WORK_NAMESPACE: Uuid = Uuid::from_u128(0x6614_a6a2_5334_5d06_8e10_93ad_239b_736e);

/// Logical stream within a Job's Build log.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BuildLogStream {
  /// Process standard output.
  Stdout,
  /// Process standard error.
  Stderr,
}

impl BuildLogStream {
  /// Stable persistence representation.
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Stdout => "stdout",
      Self::Stderr => "stderr",
    }
  }
}

/// Exact SHA-256 identity of one redacted immutable log chunk.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LogChunkDigest([u8; 32]);

impl LogChunkDigest {
  /// Computes a digest from the exact redacted bytes.
  #[must_use]
  pub fn digest(bytes: &[u8]) -> Self {
    Self(Sha256::digest(bytes).into())
  }

  /// Constructs a digest from its exact binary representation.
  #[must_use]
  pub const fn from_bytes(bytes: [u8; 32]) -> Self {
    Self(bytes)
  }

  /// Returns the binary digest.
  #[must_use]
  pub const fn as_bytes(self) -> [u8; 32] {
    self.0
  }
}

impl fmt::Display for LogChunkDigest {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    for byte in self.0 {
      write!(formatter, "{byte:02x}")?;
    }
    Ok(())
  }
}

/// Immutable logical manifest committed only after its bytes are verified.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogChunkManifest {
  chunk_id: LogChunkId,
  indexing_work_id: LogIndexingWorkId,
  stream: BuildLogStream,
  first_sequence: u64,
  last_sequence: u64,
  byte_length: u64,
  digest: LogChunkDigest,
  object_identity: String,
}

impl LogChunkManifest {
  /// Builds the deterministic logical and work identities for verified bytes.
  pub fn prepare(
    job_id: JobId,
    stream: BuildLogStream,
    first_sequence: u64,
    last_sequence: u64,
    bytes: &[u8],
  ) -> Result<Self, LogChunkManifestError> {
    if first_sequence == 0 || last_sequence < first_sequence {
      return Err(LogChunkManifestError::InvalidSequenceRange);
    }
    if bytes.is_empty() || bytes.len() > MAX_LOG_CHUNK_BYTES {
      return Err(LogChunkManifestError::InvalidByteLength);
    }
    let digest = LogChunkDigest::digest(bytes);
    let mut identity = Vec::with_capacity(16 + 1 + 8 + 8 + 32);
    identity.extend_from_slice(job_id.as_uuid().as_bytes());
    identity.push(match stream {
      BuildLogStream::Stdout => 1,
      BuildLogStream::Stderr => 2,
    });
    identity.extend_from_slice(&first_sequence.to_be_bytes());
    identity.extend_from_slice(&last_sequence.to_be_bytes());
    identity.extend_from_slice(&digest.as_bytes());
    let chunk_id = LogChunkId::from_uuid(Uuid::new_v5(&CHUNK_NAMESPACE, &identity))
      .map_err(|_| LogChunkManifestError::InvalidIdentity)?;
    let indexing_work_id = LogIndexingWorkId::from_uuid(Uuid::new_v5(&WORK_NAMESPACE, chunk_id.as_uuid().as_bytes()))
      .map_err(|_| LogChunkManifestError::InvalidIdentity)?;
    Ok(Self {
      chunk_id,
      indexing_work_id,
      stream,
      first_sequence,
      last_sequence,
      byte_length: bytes.len() as u64,
      digest,
      object_identity: format!("log-chunk:v1:{chunk_id}:{digest}"),
    })
  }

  /// Verifies bytes against the immutable size and digest.
  pub fn verify(&self, bytes: &[u8]) -> Result<(), LogChunkManifestError> {
    if bytes.len() as u64 != self.byte_length {
      return Err(LogChunkManifestError::ByteLengthMismatch);
    }
    if LogChunkDigest::digest(bytes) != self.digest {
      return Err(LogChunkManifestError::DigestMismatch);
    }
    Ok(())
  }

  /// Returns the logical chunk identity.
  #[must_use]
  pub const fn chunk_id(&self) -> LogChunkId {
    self.chunk_id
  }

  /// Returns the idempotent indexing-work identity.
  #[must_use]
  pub const fn indexing_work_id(&self) -> LogIndexingWorkId {
    self.indexing_work_id
  }

  /// Returns the logical output stream.
  #[must_use]
  pub const fn stream(&self) -> BuildLogStream {
    self.stream
  }

  /// Returns the first represented global event sequence.
  #[must_use]
  pub const fn first_sequence(&self) -> u64 {
    self.first_sequence
  }

  /// Returns the last represented global event sequence.
  #[must_use]
  pub const fn last_sequence(&self) -> u64 {
    self.last_sequence
  }

  /// Returns the exact redacted byte length.
  #[must_use]
  pub const fn byte_length(&self) -> u64 {
    self.byte_length
  }

  /// Returns the exact redacted-byte digest.
  #[must_use]
  pub const fn digest(&self) -> LogChunkDigest {
    self.digest
  }

  /// Borrows the backend-neutral immutable object identity.
  #[must_use]
  pub fn object_identity(&self) -> &str {
    &self.object_identity
  }
}

/// Invalid or mismatched immutable log-chunk data.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum LogChunkManifestError {
  /// The event-sequence range is empty, reversed, or starts at zero.
  #[error("log chunk sequence range is invalid")]
  InvalidSequenceRange,
  /// A chunk is empty or exceeds the configured immutable-object bound.
  #[error("log chunk byte length is invalid")]
  InvalidByteLength,
  /// A deterministic domain identity could not be constructed.
  #[error("log chunk identity is invalid")]
  InvalidIdentity,
  /// Supplied bytes do not have the declared length.
  #[error("log chunk byte length does not match its manifest")]
  ByteLengthMismatch,
  /// Supplied bytes do not have the declared digest.
  #[error("log chunk digest does not match its manifest")]
  DigestMismatch,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn identity_is_deterministic_and_binds_every_immutable_dimension() {
    let job = JobId::from_uuid(Uuid::from_u128(1)).unwrap();
    let first = LogChunkManifest::prepare(job, BuildLogStream::Stdout, 2, 3, b"safe output").unwrap();
    let replay = LogChunkManifest::prepare(job, BuildLogStream::Stdout, 2, 3, b"safe output").unwrap();
    assert_eq!(first, replay);
    assert_ne!(
      first.chunk_id(),
      LogChunkManifest::prepare(job, BuildLogStream::Stderr, 2, 3, b"safe output")
        .unwrap()
        .chunk_id()
    );
    assert_eq!(first.verify(b"safe output"), Ok(()));
    assert_eq!(first.verify(b"evil output"), Err(LogChunkManifestError::DigestMismatch));
  }
}
