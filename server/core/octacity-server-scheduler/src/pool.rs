use octacity_server_domain::{EntityKind, TransitionError};

/// Drain lifecycle controlling whether an Agent Pool accepts new Jobs.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
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

  /// Reports whether new Jobs may be placed in the Pool.
  #[must_use]
  pub const fn accepts_jobs(self) -> bool {
    matches!(self, Self::Accepting)
  }
}
