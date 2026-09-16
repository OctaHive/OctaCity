use std::fmt;

use crate::{DomainError, EntityKind};

/// Typed rejection of one state-machine event.
///
/// The original state and event remain available to application code and
/// tests, while conversion to [`DomainError`] deliberately exposes only the
/// safe entity classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitionError<S, E> {
  entity: EntityKind,
  state: S,
  event: E,
}

impl<S, E> TransitionError<S, E> {
  /// Constructs a rejection for an unsupported state and event pair.
  #[must_use]
  pub const fn new(entity: EntityKind, state: S, event: E) -> Self {
    Self { entity, state, event }
  }

  /// Returns the entity whose transition was rejected.
  #[must_use]
  pub const fn entity(&self) -> EntityKind {
    self.entity
  }

  /// Borrows the original state.
  #[must_use]
  pub const fn state(&self) -> &S {
    &self.state
  }

  /// Borrows the rejected event.
  #[must_use]
  pub const fn event(&self) -> &E {
    &self.event
  }

  /// Returns the entity, original state, and rejected event.
  #[must_use]
  pub fn into_parts(self) -> (EntityKind, S, E) {
    (self.entity, self.state, self.event)
  }
}

impl<S: fmt::Debug, E: fmt::Debug> fmt::Display for TransitionError<S, E> {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
      formatter,
      "invalid {} transition from {:?} with {:?}",
      self.entity, self.state, self.event
    )
  }
}

impl<S: fmt::Debug, E: fmt::Debug> std::error::Error for TransitionError<S, E> {}

impl<S, E> From<TransitionError<S, E>> for DomainError {
  fn from(error: TransitionError<S, E>) -> Self {
    Self::InvalidTransition { entity: error.entity }
  }
}
