use octacity_server_domain::{EntityKind, TransitionError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Drain lifecycle controlling whether an Agent Pool accepts new Jobs.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PoolDrainState {
  /// New compatible Jobs may be placed in the Pool.
  Accepting,
  /// New placement is stopped while active leases finish.
  GracefulDrain,
  /// New placement is stopped and active leases must be terminated.
  ForcedDrain,
  /// No active lease remains and new placement is still disabled.
  Drained,
}

/// Fact applied to a [`PoolDrainState`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PoolDrainEvent {
  /// Stop placement and allow active leases to finish.
  BeginGraceful,
  /// Stop placement and terminate active leases.
  BeginForced,
  /// The authoritative store reports no remaining active lease.
  LeasesCleared,
  /// Return a fully drained Pool to accepting state.
  Resume,
}

/// A requested Pool drain state is not reachable from the current state.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("cannot change Pool drain state from {current:?} to {requested:?} (active leases: {active_leases})")]
pub struct PoolDrainTransitionError {
  /// Current authoritative drain state.
  pub current: PoolDrainState,
  /// Requested replacement state.
  pub requested: PoolDrainState,
  /// Whether the authoritative store observed active Leases.
  pub active_leases: bool,
}

impl PoolDrainState {
  /// Applies one drain fact without inspecting or mutating leases.
  pub fn transition(self, event: PoolDrainEvent) -> Result<Self, TransitionError<Self, PoolDrainEvent>> {
    match (self, event) {
      (Self::Accepting, PoolDrainEvent::BeginGraceful) => Ok(Self::GracefulDrain),
      (Self::Accepting | Self::GracefulDrain, PoolDrainEvent::BeginForced) => Ok(Self::ForcedDrain),
      (Self::GracefulDrain | Self::ForcedDrain, PoolDrainEvent::LeasesCleared) => Ok(Self::Drained),
      (Self::Drained, PoolDrainEvent::Resume) => Ok(Self::Accepting),
      _ => Err(TransitionError::new(EntityKind::Pool, self, event)),
    }
  }

  /// Validates an operator-requested target state against current Lease state.
  pub fn transition_to(self, requested: Self, active_leases: bool) -> Result<Self, PoolDrainTransitionError> {
    if self == requested {
      return Ok(self);
    }
    let event = match (self, requested, active_leases) {
      (Self::Accepting, Self::GracefulDrain, _) => PoolDrainEvent::BeginGraceful,
      (Self::Accepting | Self::GracefulDrain, Self::ForcedDrain, _) => PoolDrainEvent::BeginForced,
      (Self::GracefulDrain | Self::ForcedDrain, Self::Drained, false) => PoolDrainEvent::LeasesCleared,
      (Self::Drained, Self::Accepting, _) => PoolDrainEvent::Resume,
      _ => {
        return Err(PoolDrainTransitionError {
          current: self,
          requested,
          active_leases,
        });
      }
    };
    self.transition(event).map_err(|_| PoolDrainTransitionError {
      current: self,
      requested,
      active_leases,
    })
  }

  /// Reports whether new Jobs may be placed in the Pool.
  #[must_use]
  pub const fn accepts_jobs(self) -> bool {
    matches!(self, Self::Accepting)
  }
}
