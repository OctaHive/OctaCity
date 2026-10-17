//! Logical secret references, policy, and short-lived grant interfaces.
//!
//! This module deliberately separates serializable logical configuration from
//! transient sensitive material. Raw secret values and provider credentials
//! cannot become durable domain data through this API.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod credential_publication;
mod enrollment;
mod grant;
mod provider_credentials;
pub use credential_publication::{CredentialPublication, CredentialPublicationKind, ProviderCredentialPublisher};
mod reference;
pub use provider_credentials::{
  MAX_PROVIDER_CREDENTIAL_BYTES, ProviderCredentialError, ProviderCredentialManager, ProviderCredentialReceipt,
  ProviderCredentialValue,
};
#[cfg(test)]
mod provider_credentials_tests;

pub use enrollment::AgentEnrollmentSecretKey;
pub use grant::{
  DelegatedGrant, DelegatedGrantCredential, GrantError, GrantRequest, GrantSubject, MAX_DELEGATED_GRANT_BYTES,
  MAX_DELEGATED_GRANT_LIFETIME_SECONDS, SecretGrantProvider, SecretProviderConfiguration, SecretProviderFailure,
  SecretProviderFailureClass, SecretProviderRegistry,
};
pub use reference::{
  IdentityProfileName, LogicalSecretReference, MAX_LOGICAL_REFERENCE_BYTES, MAX_PROFILE_REFERENCES, SecretProfileName,
  SecretProviderId,
};
