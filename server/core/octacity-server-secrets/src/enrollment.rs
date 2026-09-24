use std::fmt;

use hmac::{Hmac, Mac as _};
use zeroize::Zeroizing;

const AGENT_ENROLLMENT_SECRET_DOMAIN: &[u8] = b"octacity.agent-enrollment-secret.v1\0";

/// Server-owned key used only to derive replay-safe enrollment bearer secrets.
#[derive(Clone)]
pub struct AgentEnrollmentSecretKey(Zeroizing<[u8; 32]>);

impl AgentEnrollmentSecretKey {
  /// Wraps independently generated 256-bit server credential material.
  #[must_use]
  pub fn new(bytes: [u8; 32]) -> Self {
    Self(Zeroizing::new(bytes))
  }

  /// Derives one domain-separated secret for a stable enrollment identity.
  #[must_use]
  pub fn derive(&self, credential_id: &str) -> [u8; 32] {
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(self.0.as_slice())
      .expect("HMAC accepts independently generated keys of every size");
    mac.update(AGENT_ENROLLMENT_SECRET_DOMAIN);
    mac.update(credential_id.as_bytes());
    mac.finalize().into_bytes().into()
  }
}

impl fmt::Debug for AgentEnrollmentSecretKey {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("AgentEnrollmentSecretKey([REDACTED])")
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn enrollment_derivation_is_stable_scoped_and_redacted() {
    let key = AgentEnrollmentSecretKey::new([7; 32]);
    assert_eq!(key.derive("credential-a"), key.derive("credential-a"));
    assert_ne!(key.derive("credential-a"), key.derive("credential-b"));
    assert_eq!(format!("{key:?}"), "AgentEnrollmentSecretKey([REDACTED])");
  }
}
