use std::fmt;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signer as _, SigningKey, VerifyingKey};
use octacity_protocol::{
  JobBinding, JobSpecV1, MAX_SIGNED_JOB_SPEC_BYTES, SIGNATURE_ALGORITHM, SignedEnvelope, valid_signing_key_id,
};
use thiserror::Error;

/// One active server-owned Ed25519 key used to sign Agent execution intent.
///
/// The private key deliberately lives in the server Job core rather than the
/// cross-product protocol crate. Agents only need the corresponding public key
/// and the shared envelope verification contract.
pub struct JobSpecSigner {
  key_id: String,
  signing_key: SigningKey,
}

impl JobSpecSigner {
  /// Constructs a signer from an operator-owned key identifier and Ed25519 seed.
  pub fn new(key_id: impl Into<String>, seed: [u8; 32]) -> Result<Self, JobSpecSigningError> {
    let key_id = key_id.into();
    if !valid_signing_key_id(&key_id) {
      return Err(JobSpecSigningError::InvalidKeyId);
    }
    Ok(Self {
      key_id,
      signing_key: SigningKey::from_bytes(&seed),
    })
  }

  /// Borrows the public identifier included in every produced envelope.
  #[must_use]
  pub fn key_id(&self) -> &str {
    &self.key_id
  }

  /// Returns the public verification key distributed to agents.
  #[must_use]
  pub fn verifying_key(&self) -> VerifyingKey {
    self.signing_key.verifying_key()
  }

  /// Checks that the private and public halves form a usable signing pair.
  #[must_use]
  pub fn is_usable(&self) -> bool {
    let challenge = b"octacity-jobspec-signing-self-test-v1";
    self
      .verifying_key()
      .verify_strict(challenge, &self.signing_key.sign(challenge))
      .is_ok()
  }

  /// Validates and signs the deterministic server serialization of `spec`.
  pub(super) fn sign(&self, spec: &JobSpecV1) -> Result<SignedEnvelope, JobSpecSigningError> {
    spec
      .validate(&JobBinding {
        job_id: &spec.job_id,
        attempt: spec.attempt,
        now: spec.issued_at,
      })
      .map_err(JobSpecSigningError::Validation)?;
    let payload = serde_json::to_vec(spec).map_err(JobSpecSigningError::Json)?;
    if payload.len() > MAX_SIGNED_JOB_SPEC_BYTES {
      return Err(JobSpecSigningError::PayloadTooLarge);
    }
    Ok(SignedEnvelope {
      key_id: self.key_id.clone(),
      algorithm: SIGNATURE_ALGORITHM.to_owned(),
      payload: BASE64.encode(&payload),
      signature: BASE64.encode(self.signing_key.sign(&payload).to_bytes()),
    })
  }
}

impl fmt::Debug for JobSpecSigner {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("JobSpecSigner")
      .field("key_id", &self.key_id)
      .field("signing_key", &"[REDACTED]")
      .finish()
  }
}

/// Failure while producing a server-signed JobSpec envelope.
#[derive(Debug, Error)]
pub enum JobSpecSigningError {
  /// The public key identifier is empty, malformed, or exceeds its bound.
  #[error("JobSpec signing key identifier is invalid")]
  InvalidKeyId,
  /// The server-derived JobSpec violates the shared wire contract.
  #[error("server-derived JobSpec is invalid: {0}")]
  Validation(String),
  /// The validated JobSpec could not be serialized.
  #[error("server-derived JobSpec could not be serialized: {0}")]
  Json(#[source] serde_json::Error),
  /// The canonical payload exceeds the shared protocol bound.
  #[error("server-derived JobSpec exceeds the {MAX_SIGNED_JOB_SPEC_BYTES}-byte limit")]
  PayloadTooLarge,
}
