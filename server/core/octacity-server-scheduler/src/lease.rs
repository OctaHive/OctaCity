use octacity_server_domain::{EntityKind, TransitionError};

/// Durable lifecycle of one fenced Job lease.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LeaseState {
  /// The current registration may execute and renew the lease.
  Active,
  /// Heartbeats direct the owner to cancel the Job.
  CancellationRequested,
  /// Heartbeats direct the owner to drain after stopping the Job.
  DrainRequested,
  /// The owner released the lease without a terminal Job outcome.
  Released,
  /// The lease deadline elapsed before renewal or completion.
  Expired,
  /// A newer owner or registration invalidated the fence.
  Fenced,
  /// The owner committed a terminal Job outcome.
  Completed,
}

/// Fact applied to a [`LeaseState`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LeaseEvent {
  /// A valid heartbeat renews the current lease.
  Renew,
  /// Durable cancellation intent targets the leased Job.
  RequestCancellation,
  /// Agent or Pool drain targets the lease.
  RequestDrain,
  /// The owner voluntarily releases the lease.
  Release,
  /// The authoritative deadline expires.
  Expire,
  /// Ownership changes and invalidates the current fence.
  Fence,
  /// The current owner commits terminal completion.
  Complete,
}

impl LeaseState {
  /// Applies one fact without changing deadlines or persistence.
  pub fn transition(self, event: LeaseEvent) -> Result<Self, TransitionError<Self, LeaseEvent>> {
    match (self, event) {
      (Self::Active | Self::CancellationRequested | Self::DrainRequested, LeaseEvent::Renew) => Ok(self),
      (Self::Active | Self::DrainRequested, LeaseEvent::RequestCancellation) => Ok(Self::CancellationRequested),
      (Self::Active, LeaseEvent::RequestDrain) => Ok(Self::DrainRequested),
      (Self::Active | Self::CancellationRequested | Self::DrainRequested, LeaseEvent::Release) => Ok(Self::Released),
      (Self::Active | Self::CancellationRequested | Self::DrainRequested, LeaseEvent::Expire) => Ok(Self::Expired),
      (Self::Active | Self::CancellationRequested | Self::DrainRequested, LeaseEvent::Fence) => Ok(Self::Fenced),
      (Self::Active | Self::CancellationRequested | Self::DrainRequested, LeaseEvent::Complete) => Ok(Self::Completed),
      _ => Err(TransitionError::new(EntityKind::Lease, self, event)),
    }
  }

  /// Reports whether the lease can no longer be renewed or completed.
  #[must_use]
  pub const fn is_terminal(self) -> bool {
    matches!(self, Self::Released | Self::Expired | Self::Fenced | Self::Completed)
  }
}
