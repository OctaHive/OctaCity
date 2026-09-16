use octacity_server_trigger::{TriggerOccurrenceEvent, TriggerOccurrenceState};

#[test]
fn trigger_occurrence_transition_table_covers_every_state_and_event_pair() {
  let states = [
    TriggerOccurrenceState::Pending,
    TriggerOccurrenceState::Evaluating,
    TriggerOccurrenceState::Deferred,
    TriggerOccurrenceState::Accepted,
    TriggerOccurrenceState::Suppressed,
    TriggerOccurrenceState::Rejected,
  ];
  let events = [
    TriggerOccurrenceEvent::BeginEvaluation,
    TriggerOccurrenceEvent::Defer,
    TriggerOccurrenceEvent::Accept,
    TriggerOccurrenceEvent::Suppress,
    TriggerOccurrenceEvent::Reject,
    TriggerOccurrenceEvent::Retry,
  ];
  let allowed = [
    (
      TriggerOccurrenceState::Pending,
      TriggerOccurrenceEvent::BeginEvaluation,
      TriggerOccurrenceState::Evaluating,
    ),
    (
      TriggerOccurrenceState::Evaluating,
      TriggerOccurrenceEvent::Defer,
      TriggerOccurrenceState::Deferred,
    ),
    (
      TriggerOccurrenceState::Evaluating,
      TriggerOccurrenceEvent::Accept,
      TriggerOccurrenceState::Accepted,
    ),
    (
      TriggerOccurrenceState::Evaluating,
      TriggerOccurrenceEvent::Suppress,
      TriggerOccurrenceState::Suppressed,
    ),
    (
      TriggerOccurrenceState::Evaluating,
      TriggerOccurrenceEvent::Reject,
      TriggerOccurrenceState::Rejected,
    ),
    (
      TriggerOccurrenceState::Deferred,
      TriggerOccurrenceEvent::Retry,
      TriggerOccurrenceState::Pending,
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
}

#[test]
fn only_permanent_trigger_outcomes_report_terminal() {
  for state in [
    TriggerOccurrenceState::Pending,
    TriggerOccurrenceState::Evaluating,
    TriggerOccurrenceState::Deferred,
  ] {
    assert!(!state.is_terminal(), "{state:?}");
  }
  for state in [
    TriggerOccurrenceState::Accepted,
    TriggerOccurrenceState::Suppressed,
    TriggerOccurrenceState::Rejected,
  ] {
    assert!(state.is_terminal(), "{state:?}");
  }
}
