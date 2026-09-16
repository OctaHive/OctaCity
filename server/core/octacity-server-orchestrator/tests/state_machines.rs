use std::fmt::Debug;

use octacity_server_domain::TransitionError;
use octacity_server_orchestrator::{
  AttemptEvent, AttemptState, BuildEvent, BuildState, OrchestrationEvent, OrchestrationState,
};

fn assert_complete_table<S, E>(
  states: &[S],
  events: &[E],
  allowed: &[(S, E, S)],
  transition: impl Fn(S, E) -> Result<S, TransitionError<S, E>>,
) where
  S: Copy + Debug + Eq,
  E: Copy + Debug + Eq,
{
  for &state in states {
    for &event in events {
      let expected = allowed
        .iter()
        .find_map(|&(from, candidate, to)| (from == state && candidate == event).then_some(to));
      match expected {
        Some(expected) => assert_eq!(transition(state, event), Ok(expected), "{state:?} + {event:?}"),
        None => assert!(
          transition(state, event).is_err(),
          "{state:?} + {event:?} must be forbidden"
        ),
      }
    }
  }
}

#[test]
fn build_transition_table_is_complete() {
  let states = [
    BuildState::Queued,
    BuildState::Running,
    BuildState::Succeeded,
    BuildState::Failed,
    BuildState::Cancelled,
  ];
  let events = [
    BuildEvent::Start,
    BuildEvent::Succeed,
    BuildEvent::Fail,
    BuildEvent::Cancel,
    BuildEvent::Retry,
  ];
  let allowed = [
    (BuildState::Queued, BuildEvent::Start, BuildState::Running),
    (BuildState::Queued, BuildEvent::Cancel, BuildState::Cancelled),
    (BuildState::Running, BuildEvent::Succeed, BuildState::Succeeded),
    (BuildState::Running, BuildEvent::Fail, BuildState::Failed),
    (BuildState::Running, BuildEvent::Cancel, BuildState::Cancelled),
    (BuildState::Failed, BuildEvent::Retry, BuildState::Queued),
  ];
  assert_complete_table(&states, &events, &allowed, BuildState::transition);
  assert!(!BuildState::Queued.is_terminal());
  assert!(BuildState::Succeeded.is_terminal());
  assert!(BuildState::Failed.is_terminal());
  assert!(BuildState::Cancelled.is_terminal());
}

#[test]
fn attempt_transition_table_is_complete() {
  let states = [
    AttemptState::Created,
    AttemptState::Running,
    AttemptState::Succeeded,
    AttemptState::Failed,
    AttemptState::Cancelled,
  ];
  let events = [
    AttemptEvent::Start,
    AttemptEvent::Succeed,
    AttemptEvent::Fail,
    AttemptEvent::Cancel,
  ];
  let allowed = [
    (AttemptState::Created, AttemptEvent::Start, AttemptState::Running),
    (AttemptState::Created, AttemptEvent::Cancel, AttemptState::Cancelled),
    (AttemptState::Running, AttemptEvent::Succeed, AttemptState::Succeeded),
    (AttemptState::Running, AttemptEvent::Fail, AttemptState::Failed),
    (AttemptState::Running, AttemptEvent::Cancel, AttemptState::Cancelled),
  ];
  assert_complete_table(&states, &events, &allowed, AttemptState::transition);
  assert!(!AttemptState::Running.is_terminal());
  assert!(AttemptState::Succeeded.is_terminal());
  assert!(AttemptState::Failed.is_terminal());
  assert!(AttemptState::Cancelled.is_terminal());
}

#[test]
fn orchestration_transition_table_is_complete() {
  let states = [
    OrchestrationState::Pending,
    OrchestrationState::Reconciling,
    OrchestrationState::Waiting,
    OrchestrationState::Completed,
    OrchestrationState::Failed,
  ];
  let events = [
    OrchestrationEvent::Begin,
    OrchestrationEvent::Wait,
    OrchestrationEvent::Resume,
    OrchestrationEvent::Complete,
    OrchestrationEvent::Fail,
    OrchestrationEvent::Retry,
  ];
  let allowed = [
    (
      OrchestrationState::Pending,
      OrchestrationEvent::Begin,
      OrchestrationState::Reconciling,
    ),
    (
      OrchestrationState::Reconciling,
      OrchestrationEvent::Wait,
      OrchestrationState::Waiting,
    ),
    (
      OrchestrationState::Reconciling,
      OrchestrationEvent::Complete,
      OrchestrationState::Completed,
    ),
    (
      OrchestrationState::Reconciling,
      OrchestrationEvent::Fail,
      OrchestrationState::Failed,
    ),
    (
      OrchestrationState::Waiting,
      OrchestrationEvent::Resume,
      OrchestrationState::Pending,
    ),
    (
      OrchestrationState::Failed,
      OrchestrationEvent::Retry,
      OrchestrationState::Pending,
    ),
  ];
  assert_complete_table(&states, &events, &allowed, OrchestrationState::transition);
  assert!(!OrchestrationState::Waiting.is_terminal());
  assert!(OrchestrationState::Completed.is_terminal());
}
