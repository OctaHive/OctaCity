use std::fmt;

use octacity_server_factory::{
  AuthorizedFactoryCredentialProfile, FactoryCredentialConsumer, FactoryCredentialProfiles, FactoryError, FactoryKey,
  StageAttemptId,
};
use octacity_server_secrets::{DelegatedGrantCredential, GrantError};
use thiserror::Error;

/// Secret-safe failure while authorizing or materializing one Factory credential.
#[derive(Debug, Error)]
pub enum FactoryCredentialResolutionError {
  /// The requested profile does not belong to the trusted consumer.
  #[error("Factory credential profile is not authorized for this consumer")]
  Scope(#[source] FactoryError),
  /// The credential provider rejected or could not satisfy the authorized request.
  #[error("Factory credential provider could not issue scoped material")]
  Provider(#[source] GrantError),
}

/// One transient credential bound to public Stage authorization.
///
/// Debug output is deliberately redacted by the enclosed zeroizing credential;
/// this type implements neither serialization nor display.
pub struct ScopedFactoryCredential {
  authorization: AuthorizedFactoryCredentialProfile,
  material: DelegatedGrantCredential,
}

impl ScopedFactoryCredential {
  /// Returns public authorization metadata without exposing material.
  #[must_use]
  pub const fn authorization(&self) -> &AuthorizedFactoryCredentialProfile {
    &self.authorization
  }

  /// Consumes the scope and returns material to the already-authorized trusted consumer.
  #[must_use]
  pub fn into_material(self) -> DelegatedGrantCredential {
    self.material
  }
}

impl fmt::Debug for ScopedFactoryCredential {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("ScopedFactoryCredential")
      .field("authorization", &self.authorization)
      .field("material", &"[REDACTED]")
      .finish()
  }
}

/// Authorizes a logical profile before allowing a provider to materialize it.
///
/// The resolver is never invoked for a cross-purpose request. Callers should
/// pass the returned material directly to the named trusted consumer and must
/// not persist it in Factory state, logs, traces, or Artifact metadata.
pub fn resolve_factory_credential<F>(
  profiles: &FactoryCredentialProfiles,
  stage_attempt_id: StageAttemptId,
  consumer: FactoryCredentialConsumer,
  requested_profile: &FactoryKey,
  resolver: F,
) -> Result<ScopedFactoryCredential, FactoryCredentialResolutionError>
where
  F: FnOnce(&AuthorizedFactoryCredentialProfile) -> Result<DelegatedGrantCredential, GrantError>,
{
  let authorization = profiles
    .authorize(stage_attempt_id, consumer, requested_profile)
    .map_err(FactoryCredentialResolutionError::Scope)?;
  let material = resolver(&authorization).map_err(FactoryCredentialResolutionError::Provider)?;
  Ok(ScopedFactoryCredential {
    authorization,
    material,
  })
}
