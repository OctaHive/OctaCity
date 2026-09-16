use octacity_server_artifacts::{ArtifactEvent, ArtifactState};

#[test]
fn artifact_transition_table_covers_every_state_and_event_pair() {
  let states = [
    ArtifactState::Pending,
    ArtifactState::Verifying,
    ArtifactState::Published,
    ArtifactState::Expired,
    ArtifactState::Deleted,
  ];
  let events = [
    ArtifactEvent::BeginVerification,
    ArtifactEvent::VerificationFailed,
    ArtifactEvent::Publish,
    ArtifactEvent::Expire,
    ArtifactEvent::Delete,
  ];
  let allowed = [
    (
      ArtifactState::Pending,
      ArtifactEvent::BeginVerification,
      ArtifactState::Verifying,
    ),
    (ArtifactState::Pending, ArtifactEvent::Delete, ArtifactState::Deleted),
    (
      ArtifactState::Verifying,
      ArtifactEvent::VerificationFailed,
      ArtifactState::Pending,
    ),
    (
      ArtifactState::Verifying,
      ArtifactEvent::Publish,
      ArtifactState::Published,
    ),
    (ArtifactState::Published, ArtifactEvent::Expire, ArtifactState::Expired),
    (ArtifactState::Expired, ArtifactEvent::Delete, ArtifactState::Deleted),
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
  assert!(ArtifactState::Published.is_visible());
  for state in [
    ArtifactState::Pending,
    ArtifactState::Verifying,
    ArtifactState::Expired,
    ArtifactState::Deleted,
  ] {
    assert!(!state.is_visible(), "{state:?}");
  }
}
