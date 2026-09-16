use octacity_server_job::{JobEvent, JobState};

#[test]
fn job_transition_table_covers_every_state_and_event_pair() {
  let states = [
    JobState::Blocked,
    JobState::Ready,
    JobState::Leased,
    JobState::Running,
    JobState::Cancelling,
    JobState::Succeeded,
    JobState::Failed,
    JobState::Cancelled,
    JobState::Skipped,
  ];
  let events = [
    JobEvent::DependenciesSatisfied,
    JobEvent::DependencyFailed,
    JobEvent::LeaseGranted,
    JobEvent::ExecutionStarted,
    JobEvent::CancellationRequested,
    JobEvent::LeaseLost,
    JobEvent::Succeed,
    JobEvent::Fail,
    JobEvent::Cancel,
  ];
  let allowed = [
    (JobState::Blocked, JobEvent::DependenciesSatisfied, JobState::Ready),
    (JobState::Blocked, JobEvent::DependencyFailed, JobState::Skipped),
    (JobState::Blocked, JobEvent::CancellationRequested, JobState::Cancelled),
    (JobState::Ready, JobEvent::LeaseGranted, JobState::Leased),
    (JobState::Ready, JobEvent::CancellationRequested, JobState::Cancelled),
    (JobState::Leased, JobEvent::ExecutionStarted, JobState::Running),
    (JobState::Leased, JobEvent::CancellationRequested, JobState::Cancelling),
    (JobState::Leased, JobEvent::LeaseLost, JobState::Ready),
    (JobState::Running, JobEvent::CancellationRequested, JobState::Cancelling),
    (JobState::Running, JobEvent::LeaseLost, JobState::Ready),
    (JobState::Running, JobEvent::Succeed, JobState::Succeeded),
    (JobState::Running, JobEvent::Fail, JobState::Failed),
    (JobState::Running, JobEvent::Cancel, JobState::Cancelled),
    (JobState::Cancelling, JobEvent::LeaseLost, JobState::Cancelled),
    (JobState::Cancelling, JobEvent::Succeed, JobState::Succeeded),
    (JobState::Cancelling, JobEvent::Fail, JobState::Failed),
    (JobState::Cancelling, JobEvent::Cancel, JobState::Cancelled),
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
fn only_terminal_job_states_report_terminal() {
  for state in [
    JobState::Blocked,
    JobState::Ready,
    JobState::Leased,
    JobState::Running,
    JobState::Cancelling,
  ] {
    assert!(!state.is_terminal(), "{state:?}");
  }
  for state in [
    JobState::Succeeded,
    JobState::Failed,
    JobState::Cancelled,
    JobState::Skipped,
  ] {
    assert!(state.is_terminal(), "{state:?}");
  }
}
