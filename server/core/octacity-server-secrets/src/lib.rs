//! Logical secret references, policy, and short-lived grant interfaces.
//!
//! This module deliberately separates serializable logical configuration from
//! transient sensitive material. Raw secret values and provider credentials
//! cannot become durable domain data through this API.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod enrollment;
mod grant;
mod reference;

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
