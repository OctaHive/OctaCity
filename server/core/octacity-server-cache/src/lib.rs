//! Cache namespace policy and fenced session lifecycle.
//!
//! Octa action keys and task-result semantics remain opaque to this module.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use octacity_server_domain::{EntityKind, TransitionError};

/// Authorization lifecycle of one short-lived remote-cache session.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CacheSessionState {
  /// The current lease and credential may authorize cache operations.
  Active,
  /// Explicit revocation or lease fencing permanently removed authority.
  Revoked,
  /// The session deadline elapsed and permanently removed authority.
  Expired,
}

/// Fact applied to a [`CacheSessionState`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CacheSessionEvent {
  /// Revoke authority; repeated revocation is idempotent.
  Revoke,
  /// Record authoritative expiry; repeated expiry is idempotent.
  Expire,
}

impl CacheSessionState {
  /// Applies one fact without issuing, storing, or checking credentials.
  pub fn transition(self, event: CacheSessionEvent) -> Result<Self, TransitionError<Self, CacheSessionEvent>> {
    match (self, event) {
      (Self::Active, CacheSessionEvent::Revoke) => Ok(Self::Revoked),
      (Self::Active, CacheSessionEvent::Expire) => Ok(Self::Expired),
      (Self::Revoked, CacheSessionEvent::Revoke) => Ok(Self::Revoked),
      (Self::Expired, CacheSessionEvent::Expire) => Ok(Self::Expired),
      _ => Err(TransitionError::new(EntityKind::CacheSession, self, event)),
    }
  }

  /// Reports whether this session may authorize a cache request.
  #[must_use]
  pub const fn is_authorized(self) -> bool {
    matches!(self, Self::Active)
  }
}
