use octacity_server_cache::{CacheSessionEvent, CacheSessionState};

#[test]
fn cache_session_transition_table_covers_every_state_and_event_pair() {
  let states = [
    CacheSessionState::Active,
    CacheSessionState::Revoked,
    CacheSessionState::Expired,
  ];
  let events = [CacheSessionEvent::Revoke, CacheSessionEvent::Expire];
  let allowed = [
    (
      CacheSessionState::Active,
      CacheSessionEvent::Revoke,
      CacheSessionState::Revoked,
    ),
    (
      CacheSessionState::Active,
      CacheSessionEvent::Expire,
      CacheSessionState::Expired,
    ),
    (
      CacheSessionState::Revoked,
      CacheSessionEvent::Revoke,
      CacheSessionState::Revoked,
    ),
    (
      CacheSessionState::Expired,
      CacheSessionEvent::Expire,
      CacheSessionState::Expired,
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
  assert!(CacheSessionState::Active.is_authorized());
  assert!(!CacheSessionState::Revoked.is_authorized());
  assert!(!CacheSessionState::Expired.is_authorized());
}
