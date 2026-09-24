use std::{collections::BTreeSet, fmt, num::NonZeroU64, sync::Arc};

use async_trait::async_trait;
use octacity_server_domain::{BuildId, JobId, LeaseId, ProjectId, Timestamp};
use thiserror::Error;
use zeroize::Zeroizing;

use crate::{LogicalSecretReference, SecretProfileName, SecretProviderId, reference::validate_reference_set};

/// Maximum bytes in one opaque delegated-access credential.
pub const MAX_DELEGATED_GRANT_BYTES: usize = 16 * 1024;
/// Maximum lifetime accepted for any delegated secret-provider grant.
pub const MAX_DELEGATED_GRANT_LIFETIME_SECONDS: u64 = 60 * 60;

/// Validated public configuration shared with the secret-provider boundary.
///
/// Provider credentials and provider-specific mapping configuration are
/// intentionally absent and remain owned by the concrete adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecretProviderConfiguration {
  provider: SecretProviderId,
  maximum_grant_lifetime_seconds: NonZeroU64,
}

impl SecretProviderConfiguration {
  /// Creates a delegated-only provider configuration.
  pub fn delegated(provider: SecretProviderId, maximum_grant_lifetime_seconds: u64) -> Result<Self, GrantError> {
    let maximum_grant_lifetime_seconds = NonZeroU64::new(maximum_grant_lifetime_seconds)
      .filter(|value| value.get() <= MAX_DELEGATED_GRANT_LIFETIME_SECONDS)
      .ok_or(GrantError::InvalidProviderConfiguration)?;
    Ok(Self {
      provider,
      maximum_grant_lifetime_seconds,
    })
  }

  /// Returns the stable provider identity.
  #[must_use]
  pub const fn provider(&self) -> &SecretProviderId {
    &self.provider
  }

  /// Returns the configured grant-lifetime ceiling.
  #[must_use]
  pub const fn maximum_grant_lifetime_seconds(&self) -> u64 {
    self.maximum_grant_lifetime_seconds.get()
  }
}

/// Non-secret authority to which a delegated grant is bound.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GrantSubject {
  /// Project whose effective policy allowed the profile.
  pub project_id: ProjectId,
  /// Immutable Build using the profile.
  pub build_id: BuildId,
  /// Current Job receiving the delegated access.
  pub job_id: JobId,
  /// Current Lease already authenticated and fenced by the application layer.
  pub lease_id: LeaseId,
}

/// Validated request for provider-issued delegated Octa access.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GrantRequest {
  profile: SecretProfileName,
  references: BTreeSet<LogicalSecretReference>,
  subject: GrantSubject,
  issued_at: Timestamp,
  expires_at: Timestamp,
}

impl GrantRequest {
  /// Constructs a bounded request whose references all belong to one provider.
  pub fn new(
    configuration: &SecretProviderConfiguration,
    profile: SecretProfileName,
    references: BTreeSet<LogicalSecretReference>,
    subject: GrantSubject,
    issued_at: Timestamp,
    expires_at: Timestamp,
  ) -> Result<Self, GrantError> {
    if !validate_reference_set(&references)
      || references
        .iter()
        .any(|reference| reference.provider() != configuration.provider())
    {
      return Err(GrantError::InvalidReferenceSet);
    }
    let lifetime_millis = expires_at
      .unix_millis()
      .checked_sub(issued_at.unix_millis())
      .filter(|lifetime| *lifetime > 0)
      .ok_or(GrantError::InvalidLifetime)?;
    let maximum_millis = i64::try_from(configuration.maximum_grant_lifetime_seconds())
      .ok()
      .and_then(|seconds| seconds.checked_mul(1_000))
      .ok_or(GrantError::InvalidLifetime)?;
    if lifetime_millis > maximum_millis {
      return Err(GrantError::InvalidLifetime);
    }
    Ok(Self {
      profile,
      references,
      subject,
      issued_at,
      expires_at,
    })
  }

  /// Returns the selected logical profile.
  #[must_use]
  pub const fn profile(&self) -> &SecretProfileName {
    &self.profile
  }

  /// Returns the provider-scoped logical references.
  #[must_use]
  pub const fn references(&self) -> &BTreeSet<LogicalSecretReference> {
    &self.references
  }

  /// Returns the non-secret Job authority bound to this request.
  #[must_use]
  pub const fn subject(&self) -> GrantSubject {
    self.subject
  }

  /// Returns the authoritative issue time.
  #[must_use]
  pub const fn issued_at(&self) -> Timestamp {
    self.issued_at
  }

  /// Returns the requested expiry.
  #[must_use]
  pub const fn expires_at(&self) -> Timestamp {
    self.expires_at
  }
}

/// Opaque transient credential issued for delegated Octa access.
///
/// It intentionally implements neither `Clone`, `Display`, `Serialize`, nor
/// `Deserialize`, preventing accidental use in durable records, REST DTOs,
/// audit facts, logs, or metric labels.
///
/// ```compile_fail
/// use octacity_server_secrets::DelegatedGrantCredential;
/// fn durable<T: serde::Serialize>() {}
/// durable::<DelegatedGrantCredential>();
/// ```
pub struct DelegatedGrantCredential(Zeroizing<Vec<u8>>);

impl DelegatedGrantCredential {
  /// Protects one bounded non-empty credential returned by an adapter.
  pub fn new(bytes: Vec<u8>) -> Result<Self, GrantError> {
    if bytes.is_empty() || bytes.len() > MAX_DELEGATED_GRANT_BYTES {
      return Err(GrantError::InvalidCredential);
    }
    Ok(Self(Zeroizing::new(bytes)))
  }

  /// Borrows the sensitive bytes for the bounded delivery mechanism only.
  #[must_use]
  pub fn expose_for_delivery(&self) -> &[u8] {
    self.0.as_slice()
  }
}

impl fmt::Debug for DelegatedGrantCredential {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("DelegatedGrantCredential([REDACTED])")
  }
}

/// Provider-issued short-lived delegated access and its public expiry.
pub struct DelegatedGrant {
  provider: SecretProviderId,
  expires_at: Timestamp,
  credential: DelegatedGrantCredential,
}

impl DelegatedGrant {
  /// Wraps a credential only when its provider and expiry match the request.
  pub fn new(
    request: &GrantRequest,
    provider: SecretProviderId,
    expires_at: Timestamp,
    credential: DelegatedGrantCredential,
  ) -> Result<Self, GrantError> {
    let expected_provider = request
      .references()
      .first()
      .expect("a validated grant request has at least one reference")
      .provider();
    if &provider != expected_provider
      || expires_at.unix_millis() <= request.issued_at().unix_millis()
      || expires_at.unix_millis() > request.expires_at().unix_millis()
    {
      return Err(GrantError::InvalidProviderResponse);
    }
    Ok(Self {
      provider,
      expires_at,
      credential,
    })
  }

  /// Returns the logical provider that issued the grant.
  #[must_use]
  pub const fn provider(&self) -> &SecretProviderId {
    &self.provider
  }

  /// Returns the provider-declared expiry, bounded by the request.
  #[must_use]
  pub const fn expires_at(&self) -> Timestamp {
    self.expires_at
  }

  /// Consumes the grant and returns its protected credential for delivery.
  #[must_use]
  pub fn into_credential(self) -> DelegatedGrantCredential {
    self.credential
  }
}

impl fmt::Debug for DelegatedGrant {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("DelegatedGrant")
      .field("provider", &self.provider)
      .field("expires_at", &self.expires_at)
      .field("credential", &"[REDACTED]")
      .finish()
  }
}

/// Stable secret-provider failure classification safe for diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecretProviderFailureClass {
  /// The logical reference or requested authority is invalid.
  InvalidRequest,
  /// The provider does not support delegated access for this request.
  Unsupported,
  /// Provider authentication or policy permanently rejected the operation.
  Denied,
  /// A retry may succeed without changing the request.
  Unavailable,
}

/// Secret-safe provider failure without raw provider messages.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("secret provider operation failed: {class:?}")]
pub struct SecretProviderFailure {
  class: SecretProviderFailureClass,
}

impl SecretProviderFailure {
  /// Creates one classified failure without accepting provider diagnostics.
  #[must_use]
  pub const fn new(class: SecretProviderFailureClass) -> Self {
    Self { class }
  }

  /// Returns the stable failure class.
  #[must_use]
  pub const fn class(self) -> SecretProviderFailureClass {
    self.class
  }
}

/// Failure while validating or dispatching a delegated grant.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum GrantError {
  /// Public provider configuration violates a bound.
  #[error("invalid secret provider configuration")]
  InvalidProviderConfiguration,
  /// The reference set is empty, oversized, or spans another provider.
  #[error("invalid logical secret reference set")]
  InvalidReferenceSet,
  /// The requested lifetime is empty or exceeds the provider ceiling.
  #[error("invalid delegated grant lifetime")]
  InvalidLifetime,
  /// Provider returned an empty or oversized credential.
  #[error("invalid delegated grant credential")]
  InvalidCredential,
  /// Provider response does not match the request authority.
  #[error("invalid delegated grant provider response")]
  InvalidProviderResponse,
  /// No provider is registered for the logical identity.
  #[error("secret provider is not configured")]
  ProviderNotConfigured,
  /// Provider rejected or could not complete the operation.
  #[error(transparent)]
  Provider(SecretProviderFailure),
}

/// Provider adapter capable of issuing delegated access without returning raw
/// secret values or exposing its own credentials.
#[async_trait]
pub trait SecretGrantProvider: Send + Sync {
  /// Returns validated public configuration for registry selection.
  fn configuration(&self) -> &SecretProviderConfiguration;

  /// Checks whether the configured provider can currently issue grants.
  async fn check(&self) -> Result<(), SecretProviderFailure>;

  /// Issues one bounded short-lived delegated grant.
  async fn issue_grant(&self, request: &GrantRequest) -> Result<DelegatedGrant, SecretProviderFailure>;
}

/// Immutable registry that selects adapters only by logical provider identity.
#[derive(Default)]
pub struct SecretProviderRegistry {
  providers: std::collections::BTreeMap<SecretProviderId, Arc<dyn SecretGrantProvider>>,
}

impl SecretProviderRegistry {
  /// Builds a registry and rejects duplicate provider identities.
  pub fn new(providers: impl IntoIterator<Item = Arc<dyn SecretGrantProvider>>) -> Result<Self, GrantError> {
    let mut registry = Self::default();
    for provider in providers {
      let identity = provider.configuration().provider().clone();
      if registry.providers.insert(identity, provider).is_some() {
        return Err(GrantError::InvalidProviderConfiguration);
      }
    }
    Ok(registry)
  }

  /// Issues through the single provider named by the request references.
  pub async fn issue(&self, request: &GrantRequest) -> Result<DelegatedGrant, GrantError> {
    let provider = request
      .references()
      .first()
      .expect("a validated grant request has at least one reference")
      .provider();
    self
      .providers
      .get(provider)
      .ok_or(GrantError::ProviderNotConfigured)?
      .issue_grant(request)
      .await
      .map_err(GrantError::Provider)
  }

  /// Checks every configured mandatory provider without exposing diagnostics.
  pub async fn check(&self) -> Result<(), GrantError> {
    for provider in self.providers.values() {
      provider.check().await.map_err(GrantError::Provider)?;
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use std::{
    future::Future,
    task::{Context, Poll, Waker},
  };

  use super::*;
  use octacity_server_domain::{BuildId, JobId, LeaseId, ProjectId};

  struct FixtureProvider {
    configuration: SecretProviderConfiguration,
  }

  #[async_trait]
  impl SecretGrantProvider for FixtureProvider {
    fn configuration(&self) -> &SecretProviderConfiguration {
      &self.configuration
    }

    async fn check(&self) -> Result<(), SecretProviderFailure> {
      Ok(())
    }

    async fn issue_grant(&self, request: &GrantRequest) -> Result<DelegatedGrant, SecretProviderFailure> {
      Ok(
        DelegatedGrant::new(
          request,
          self.configuration.provider().clone(),
          request.expires_at(),
          DelegatedGrantCredential::new(b"fixture-delegated-token".to_vec()).unwrap(),
        )
        .unwrap(),
      )
    }
  }

  fn timestamp(value: i64) -> Timestamp {
    Timestamp::from_unix_millis(value).unwrap()
  }

  fn configuration() -> SecretProviderConfiguration {
    SecretProviderConfiguration::delegated(SecretProviderId::new("vault").unwrap(), 60).unwrap()
  }

  fn request(configuration: &SecretProviderConfiguration) -> GrantRequest {
    GrantRequest::new(
      configuration,
      SecretProfileName::new("ci/release").unwrap(),
      BTreeSet::from([LogicalSecretReference::new(configuration.provider().clone(), "kv/release/signing").unwrap()]),
      GrantSubject {
        project_id: ProjectId::generate(),
        build_id: BuildId::generate(),
        job_id: JobId::generate(),
        lease_id: LeaseId::generate(),
      },
      timestamp(1_000),
      timestamp(31_000),
    )
    .unwrap()
  }

  #[test]
  fn configuration_and_requests_enforce_provider_and_lifetime_bounds() {
    let configuration = configuration();
    let valid = request(&configuration);
    assert_eq!(valid.profile().as_str(), "ci/release");

    let too_late = GrantRequest::new(
      &configuration,
      SecretProfileName::new("ci").unwrap(),
      valid.references().clone(),
      valid.subject(),
      timestamp(1_000),
      timestamp(62_000),
    );
    assert_eq!(too_late.unwrap_err(), GrantError::InvalidLifetime);

    let other = LogicalSecretReference::new(SecretProviderId::new("other").unwrap(), "kv/item").unwrap();
    assert_eq!(
      GrantRequest::new(
        &configuration,
        SecretProfileName::new("ci").unwrap(),
        BTreeSet::from([other]),
        valid.subject(),
        timestamp(1_000),
        timestamp(2_000),
      )
      .unwrap_err(),
      GrantError::InvalidReferenceSet
    );
  }

  #[test]
  fn sensitive_grants_are_redacted_and_provider_bound() {
    let configuration = configuration();
    let request = request(&configuration);
    let credential = DelegatedGrantCredential::new(b"provider-issued-sensitive-token".to_vec()).unwrap();
    assert_eq!(format!("{credential:?}"), "DelegatedGrantCredential([REDACTED])");
    let grant = DelegatedGrant::new(
      &request,
      configuration.provider().clone(),
      timestamp(30_000),
      credential,
    )
    .unwrap();
    let debug = format!("{grant:?}");
    assert!(!debug.contains("provider-issued-sensitive-token"));
    assert_eq!(
      DelegatedGrant::new(
        &request,
        SecretProviderId::new("other").unwrap(),
        timestamp(30_000),
        DelegatedGrantCredential::new(vec![1]).unwrap(),
      )
      .unwrap_err(),
      GrantError::InvalidProviderResponse
    );
  }

  #[test]
  fn registry_rejects_duplicate_provider_configuration() {
    let first: Arc<dyn SecretGrantProvider> = Arc::new(FixtureProvider {
      configuration: configuration(),
    });
    let second: Arc<dyn SecretGrantProvider> = Arc::new(FixtureProvider {
      configuration: configuration(),
    });
    assert!(matches!(
      SecretProviderRegistry::new([first, second]),
      Err(GrantError::InvalidProviderConfiguration)
    ));
  }

  #[test]
  fn registry_dispatches_a_bounded_delegated_grant() {
    let configuration = configuration();
    let request = request(&configuration);
    let provider: Arc<dyn SecretGrantProvider> = Arc::new(FixtureProvider { configuration });
    let registry = SecretProviderRegistry::new([provider]).unwrap();

    run_ready(registry.check()).unwrap();
    let grant = run_ready(registry.issue(&request)).unwrap();
    assert_eq!(grant.provider().as_str(), "vault");
    assert_eq!(grant.expires_at(), request.expires_at());
    assert_eq!(
      grant.into_credential().expose_for_delivery(),
      b"fixture-delegated-token"
    );
  }

  fn run_ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
      Poll::Ready(output) => output,
      Poll::Pending => panic!("fixture provider unexpectedly yielded"),
    }
  }
}
