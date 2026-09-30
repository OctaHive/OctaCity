use std::{fmt, str::FromStr};

use uuid::Uuid;

use super::{ManagementSecurityError, is_canonical_text};

/// Maximum UTF-8 bytes in a verified management actor identity.
pub const MAX_MANAGEMENT_ACTOR_IDENTITY_BYTES: usize = 256;
/// Maximum ASCII bytes in an opaque management security scope.
pub const MAX_MANAGEMENT_SECURITY_SCOPE_BYTES: usize = 128;

const TRUSTED_NETWORK_SECURITY_SCOPE: &str = "trusted-network";

/// Stable classification of a management actor.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ManagementActorKind {
  /// Anonymous operator admitted only by the trusted-network deployment policy.
  UnauthenticatedManagement,
  /// Subject whose identity was verified before entering the application layer.
  AuthenticatedManagement,
}

/// A normalized management actor containing no credentials or provider claims.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ManagementActor {
  kind: ManagementActorKind,
  identity: Option<String>,
}

impl ManagementActor {
  /// Constructs an actor and rejects identity/kind combinations that could spoof attribution.
  pub fn new(kind: ManagementActorKind, identity: Option<String>) -> Result<Self, ManagementSecurityError> {
    match (kind, identity.as_deref()) {
      (ManagementActorKind::UnauthenticatedManagement, Some(_)) => {
        return Err(ManagementSecurityError::UnexpectedActorIdentity);
      }
      (ManagementActorKind::AuthenticatedManagement, None) => {
        return Err(ManagementSecurityError::MissingActorIdentity);
      }
      (ManagementActorKind::AuthenticatedManagement, Some(value)) => validate_actor_identity(value)?,
      (ManagementActorKind::UnauthenticatedManagement, None) => {}
    }
    Ok(Self { kind, identity })
  }

  /// Constructs the canonical anonymous actor for the trusted-network deployment.
  #[must_use]
  pub const fn unauthenticated_management() -> Self {
    Self {
      kind: ManagementActorKind::UnauthenticatedManagement,
      identity: None,
    }
  }

  /// Constructs a management actor from an identity already verified by an authentication adapter.
  pub fn authenticated(identity: impl Into<String>) -> Result<Self, ManagementSecurityError> {
    Self::new(ManagementActorKind::AuthenticatedManagement, Some(identity.into()))
  }

  /// Returns the stable actor classification.
  #[must_use]
  pub const fn kind(&self) -> ManagementActorKind {
    self.kind
  }

  /// Borrows the verified identity when the actor is authenticated.
  #[must_use]
  pub fn identity(&self) -> Option<&str> {
    self.identity.as_deref()
  }
}

impl fmt::Debug for ManagementActor {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("ManagementActor")
      .field("kind", &self.kind)
      .field("identity", &self.identity.as_ref().map(|_| "<redacted>"))
      .finish()
  }
}

fn validate_actor_identity(value: &str) -> Result<(), ManagementSecurityError> {
  if value.is_empty() {
    Err(ManagementSecurityError::EmptyActorIdentity)
  } else if value.len() > MAX_MANAGEMENT_ACTOR_IDENTITY_BYTES {
    Err(ManagementSecurityError::ActorIdentityTooLong)
  } else if !is_canonical_text(value, MAX_MANAGEMENT_ACTOR_IDENTITY_BYTES) {
    Err(ManagementSecurityError::InvalidActorIdentity)
  } else {
    Ok(())
  }
}

/// Opaque stable partition used to isolate management idempotency outcomes.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ManagementSecurityScope(String);

impl ManagementSecurityScope {
  /// Constructs a canonical scope using `[a-z0-9][a-z0-9._:-]*`.
  pub fn new(value: impl Into<String>) -> Result<Self, ManagementSecurityError> {
    let value = value.into();
    let mut characters = value.chars();
    let valid = is_canonical_text(&value, MAX_MANAGEMENT_SECURITY_SCOPE_BYTES)
      && characters
        .next()
        .is_some_and(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
      && characters.all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || matches!(character, '.' | '_' | ':' | '-')
      });
    if !valid {
      return Err(ManagementSecurityError::InvalidSecurityScope);
    }
    Ok(Self(value))
  }

  /// Constructs the one scope used by the trusted-network deployment.
  #[must_use]
  pub fn trusted_network() -> Self {
    Self(TRUSTED_NETWORK_SECURITY_SCOPE.to_owned())
  }

  /// Borrows the stable scope identity.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }

  pub(super) fn is_trusted_network(&self) -> bool {
    self.0 == TRUSTED_NETWORK_SECURITY_SCOPE
  }
}

impl fmt::Debug for ManagementSecurityScope {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("ManagementSecurityScope(<redacted>)")
  }
}

impl FromStr for ManagementSecurityScope {
  type Err = ManagementSecurityError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    Self::new(value)
  }
}

/// Source that established the safe facts in a management request context.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ManagementIngress {
  /// Current deployment in which network placement is the operator trust control.
  TrustedNetwork,
  /// Future adapter that verified an external subject before constructing the context.
  VerifiedIdentity,
}

/// Bounded caller classification safe for authorization decisions and diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ManagementClientKind {
  /// Human-operated interactive client.
  Interactive,
  /// Non-interactive automation client.
  Automation,
}

/// Explicit allowlist of request facts admitted into management authorization.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ManagementRequestAttributes {
  ingress: ManagementIngress,
  client_kind: Option<ManagementClientKind>,
}

impl ManagementRequestAttributes {
  /// Constructs safe typed request attributes without accepting arbitrary headers or claims.
  #[must_use]
  pub const fn new(ingress: ManagementIngress, client_kind: Option<ManagementClientKind>) -> Self {
    Self { ingress, client_kind }
  }

  /// Constructs the attributes used by the trusted-network REST adapter.
  #[must_use]
  pub const fn trusted_network() -> Self {
    Self::new(ManagementIngress::TrustedNetwork, None)
  }

  /// Returns the ingress classification that established these facts.
  #[must_use]
  pub const fn ingress(self) -> ManagementIngress {
    self.ingress
  }

  /// Returns the bounded caller classification when one was verified.
  #[must_use]
  pub const fn client_kind(self) -> Option<ManagementClientKind> {
    self.client_kind
  }
}

/// Safe UUID correlation identity shared by management diagnostics and error responses.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ManagementRequestId(Uuid);

impl ManagementRequestId {
  /// Constructs a non-nil request identity.
  pub fn new(value: Uuid) -> Result<Self, ManagementSecurityError> {
    if value.is_nil() {
      return Err(ManagementSecurityError::InvalidRequestId);
    }
    Ok(Self(value))
  }

  /// Returns the UUID value.
  #[must_use]
  pub const fn as_uuid(self) -> Uuid {
    self.0
  }
}

impl fmt::Debug for ManagementRequestId {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt::Display::fmt(self, formatter)
  }
}

impl fmt::Display for ManagementRequestId {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "{}", self.0.hyphenated())
  }
}

impl FromStr for ManagementRequestId {
  type Err = ManagementSecurityError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    let parsed = Uuid::parse_str(value).map_err(|_| ManagementSecurityError::InvalidRequestId)?;
    if value != parsed.hyphenated().to_string() {
      return Err(ManagementSecurityError::InvalidRequestId);
    }
    Self::new(parsed)
  }
}

/// Credential-free facts carried with every management command and query.
#[derive(Clone, Eq, PartialEq)]
pub struct ManagementRequestContext {
  actor: ManagementActor,
  security_scope: ManagementSecurityScope,
  request_id: ManagementRequestId,
  attributes: ManagementRequestAttributes,
}

impl ManagementRequestContext {
  /// Constructs a request context and rejects actor/ingress combinations that weaken attribution.
  pub fn new(
    actor: ManagementActor,
    security_scope: ManagementSecurityScope,
    request_id: ManagementRequestId,
    attributes: ManagementRequestAttributes,
  ) -> Result<Self, ManagementSecurityError> {
    match actor.kind() {
      ManagementActorKind::UnauthenticatedManagement
        if attributes.ingress() != ManagementIngress::TrustedNetwork || !security_scope.is_trusted_network() =>
      {
        return Err(ManagementSecurityError::InvalidAnonymousContext);
      }
      ManagementActorKind::AuthenticatedManagement if attributes.ingress() != ManagementIngress::VerifiedIdentity => {
        return Err(ManagementSecurityError::InvalidAuthenticatedContext);
      }
      ManagementActorKind::AuthenticatedManagement if security_scope.is_trusted_network() => {
        return Err(ManagementSecurityError::InvalidAuthenticatedContext);
      }
      _ => {}
    }
    Ok(Self {
      actor,
      security_scope,
      request_id,
      attributes,
    })
  }

  /// Constructs the canonical context used by the current trusted-network deployment.
  pub fn trusted_network(request_id: ManagementRequestId) -> Self {
    Self::new(
      ManagementActor::unauthenticated_management(),
      ManagementSecurityScope::trusted_network(),
      request_id,
      ManagementRequestAttributes::trusted_network(),
    )
    .expect("the canonical trusted-network context is valid")
  }

  /// Borrows the normalized actor.
  #[must_use]
  pub const fn actor(&self) -> &ManagementActor {
    &self.actor
  }

  /// Borrows the stable idempotency security scope.
  #[must_use]
  pub const fn security_scope(&self) -> &ManagementSecurityScope {
    &self.security_scope
  }

  /// Returns the safe request correlation identity.
  #[must_use]
  pub const fn request_id(&self) -> ManagementRequestId {
    self.request_id
  }

  /// Returns the typed safe request attributes.
  #[must_use]
  pub const fn attributes(&self) -> ManagementRequestAttributes {
    self.attributes
  }

  pub(super) fn is_canonical_trusted_network(&self) -> bool {
    self.actor == ManagementActor::unauthenticated_management()
      && self.security_scope.is_trusted_network()
      && self.attributes == ManagementRequestAttributes::trusted_network()
  }
}

impl fmt::Debug for ManagementRequestContext {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("ManagementRequestContext")
      .field("actor", &self.actor)
      .field("security_scope", &self.security_scope)
      .field("request_id", &self.request_id)
      .field("attributes", &self.attributes)
      .finish()
  }
}
