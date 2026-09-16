use octacity_server_scheduler::{LeaseEvent, LeaseState, PoolDrainEvent, PoolDrainState};

#[test]
fn lease_transition_table_covers_every_state_and_event_pair() {
  let states = [
    LeaseState::Active,
    LeaseState::CancellationRequested,
    LeaseState::DrainRequested,
    LeaseState::Released,
    LeaseState::Expired,
    LeaseState::Fenced,
    LeaseState::Completed,
  ];
  let events = [
    LeaseEvent::Renew,
    LeaseEvent::RequestCancellation,
    LeaseEvent::RequestDrain,
    LeaseEvent::Release,
    LeaseEvent::Expire,
    LeaseEvent::Fence,
    LeaseEvent::Complete,
  ];
  let mut allowed = Vec::new();
  for state in [
    LeaseState::Active,
    LeaseState::CancellationRequested,
    LeaseState::DrainRequested,
  ] {
    allowed.extend([
      (state, LeaseEvent::Renew, state),
      (state, LeaseEvent::Release, LeaseState::Released),
      (state, LeaseEvent::Expire, LeaseState::Expired),
      (state, LeaseEvent::Fence, LeaseState::Fenced),
      (state, LeaseEvent::Complete, LeaseState::Completed),
    ]);
  }
  allowed.extend([
    (
      LeaseState::Active,
      LeaseEvent::RequestCancellation,
      LeaseState::CancellationRequested,
    ),
    (
      LeaseState::DrainRequested,
      LeaseEvent::RequestCancellation,
      LeaseState::CancellationRequested,
    ),
    (LeaseState::Active, LeaseEvent::RequestDrain, LeaseState::DrainRequested),
  ]);

  for state in states {
    for event in events {
      let expected = allowed
        .iter()
        .find_map(|&(from, candidate, to)| (from == state && candidate == event).then_some(to));
      match expected {
        Some(expected) => assert_eq!(state.transition(event), Ok(expected), "{state:?} + {event:?}"),
        None => assert!(
          state.transition(event).is_err(),
          "{state:?} + {event:?} must be forbidden"
        ),
      }
    }
  }
}

#[test]
fn only_final_lease_states_report_terminal() {
  for state in [
    LeaseState::Active,
    LeaseState::CancellationRequested,
    LeaseState::DrainRequested,
  ] {
    assert!(!state.is_terminal(), "{state:?}");
  }
  for state in [
    LeaseState::Released,
    LeaseState::Expired,
    LeaseState::Fenced,
    LeaseState::Completed,
  ] {
    assert!(state.is_terminal(), "{state:?}");
  }
}

#[test]
fn pool_drain_transition_table_covers_every_state_and_event_pair() {
  let states = [
    PoolDrainState::Accepting,
    PoolDrainState::GracefulDrain,
    PoolDrainState::ForcedDrain,
    PoolDrainState::Drained,
  ];
  let events = [
    PoolDrainEvent::BeginGraceful,
    PoolDrainEvent::BeginForced,
    PoolDrainEvent::LeasesCleared,
    PoolDrainEvent::Resume,
  ];
  let allowed = [
    (
      PoolDrainState::Accepting,
      PoolDrainEvent::BeginGraceful,
      PoolDrainState::GracefulDrain,
    ),
    (
      PoolDrainState::Accepting,
      PoolDrainEvent::BeginForced,
      PoolDrainState::ForcedDrain,
    ),
    (
      PoolDrainState::GracefulDrain,
      PoolDrainEvent::BeginForced,
      PoolDrainState::ForcedDrain,
    ),
    (
      PoolDrainState::GracefulDrain,
      PoolDrainEvent::LeasesCleared,
      PoolDrainState::Drained,
    ),
    (
      PoolDrainState::ForcedDrain,
      PoolDrainEvent::LeasesCleared,
      PoolDrainState::Drained,
    ),
    (
      PoolDrainState::Drained,
      PoolDrainEvent::Resume,
      PoolDrainState::Accepting,
    ),
  ];

  for state in states {
    for event in events {
      let expected = allowed
        .iter()
        .find_map(|&(from, candidate, to)| (from == state && candidate == event).then_some(to));
      match expected {
        Some(expected) => assert_eq!(state.transition(event), Ok(expected), "{state:?} + {event:?}"),
        None => assert!(
          state.transition(event).is_err(),
          "{state:?} + {event:?} must be forbidden"
        ),
      }
    }
  }
  assert!(PoolDrainState::Accepting.accepts_jobs());
  assert!(!PoolDrainState::GracefulDrain.accepts_jobs());
  assert!(!PoolDrainState::ForcedDrain.accepts_jobs());
  assert!(!PoolDrainState::Drained.accepts_jobs());
}
