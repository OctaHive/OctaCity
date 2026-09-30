//! Transport-independent, credential-free management security vocabulary.

mod audit;
mod authorization;
mod context;
mod dispatch;
mod policy;

pub(crate) use audit::audited_mutation;

#[cfg(test)]
mod tests;

pub use authorization::{
  MAX_MANAGEMENT_RESOURCE_IDENTITY_BYTES, MAX_MANAGEMENT_VISIBILITY_RESOURCES, ManagementAction,
  ManagementAuthorizationDenial, ManagementAuthorizationGrant, ManagementAuthorizationMapping, ManagementResource,
  ManagementResourceIdentity, ManagementResourceKind, ManagementResourcePattern, ManagementResourceResult,
  ManagementVisibility, ManagementVisibilityKind, ManagementVisibilityView,
};
pub(crate) use authorization::{instance_resource, owned_collection_resource};
pub use context::{
  MAX_MANAGEMENT_ACTOR_IDENTITY_BYTES, MAX_MANAGEMENT_SECURITY_SCOPE_BYTES, ManagementActor, ManagementActorKind,
  ManagementClientKind, ManagementIngress, ManagementRequestAttributes, ManagementRequestContext, ManagementRequestId,
  ManagementSecurityScope,
};
pub use dispatch::{
  AuthorizedCommandHandler, AuthorizedManagementCommandHandler, AuthorizedManagementQueryHandler,
  AuthorizedQueryHandler, ManagementAuthorizationFailure, ManagementAuthorizationTarget, ManagementCommandUseCase,
  ManagementHandlerError, ManagementQueryUseCase,
};
pub use policy::{ManagementAuthorizationPolicy, TrustedNetworkManagementPolicy};

use thiserror::Error;

/// Stable validation failures for the management security vocabulary.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ManagementSecurityError {
  /// An actor identity was empty.
  #[error("management actor identity must not be empty")]
  EmptyActorIdentity,
  /// An actor identity exceeded its UTF-8 byte limit.
  #[error("management actor identity is too long")]
  ActorIdentityTooLong,
  /// An actor identity was not canonical safe text.
  #[error("management actor identity is not canonical safe text")]
  InvalidActorIdentity,
  /// An anonymous actor supplied an identity.
  #[error("anonymous management actor must not carry an identity")]
  UnexpectedActorIdentity,
  /// An authenticated actor omitted its verified identity.
  #[error("authenticated management actor requires a verified identity")]
  MissingActorIdentity,
  /// A security scope was empty, oversized, or outside its canonical alphabet.
  #[error("management security scope is invalid")]
  InvalidSecurityScope,
  /// A request correlation identity was malformed, non-canonical, or nil.
  #[error("management request identity is invalid")]
  InvalidRequestId,
  /// An unauthenticated management actor was not bound to the canonical trusted-network context.
  #[error("unauthenticated management actor requires the trusted-network context")]
  InvalidAnonymousContext,
  /// An authenticated management actor was not supplied by a verified-identity ingress.
  #[error("authenticated management actor requires verified-identity ingress")]
  InvalidAuthenticatedContext,
  /// A resource identity was empty, oversized, or not canonical safe text.
  #[error("management resource identity is invalid")]
  InvalidResourceIdentity,
  /// A collection was assigned to an unsupported owner resource kind.
  #[error("management resource ownership shape is unsupported")]
  UnsupportedResourceShape,
  /// A restricted visibility set was empty; callers must use explicit `None` visibility.
  #[error("restricted management visibility must contain at least one resource")]
  EmptyVisibility,
  /// A restricted visibility set exceeded its resource limit.
  #[error("restricted management visibility contains too many resources")]
  VisibilityTooLarge,
  /// A restricted visibility set repeated a resource.
  #[error("restricted management visibility contains a duplicate resource")]
  DuplicateVisibilityResource,
}

pub(super) fn is_canonical_text(value: &str, maximum_bytes: usize) -> bool {
  !value.is_empty() && value.len() <= maximum_bytes && value.trim() == value && !value.chars().any(char::is_control)
}
