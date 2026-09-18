use octacity_server_domain::JobId;
use octacity_server_job::JobState;
use octacity_server_orchestrator::{AttemptState, BuildState, CancellationError, cancel_job_states};

fn job(value: u128) -> JobId {
  JobId::from_uuid(uuid::Uuid::from_u128(value)).unwrap()
}

#[test]
fn cancellation_preserves_terminal_jobs_and_targets_every_active_state() {
  let decision = cancel_job_states(
    BuildState::Running,
    AttemptState::Running,
    [
      (job(1), JobState::Blocked),
      (job(2), JobState::Ready),
      (job(3), JobState::Leased),
      (job(4), JobState::Running),
      (job(5), JobState::Succeeded),
      (job(6), JobState::Failed),
      (job(7), JobState::Cancelled),
      (job(8), JobState::Skipped),
    ],
  )
  .unwrap();

  assert_eq!(
    decision
      .transitions()
      .iter()
      .map(|transition| (transition.job_id(), transition.state()))
      .collect::<Vec<_>>(),
    [
      (job(1), JobState::Cancelled),
      (job(2), JobState::Cancelled),
      (job(3), JobState::Cancelling),
      (job(4), JobState::Cancelling),
    ]
  );
  assert_eq!(decision.attempt_state(), AttemptState::Running);
  assert_eq!(decision.build_state(), BuildState::Running);
}

#[test]
fn cancellation_is_terminal_when_no_owner_must_acknowledge_it() {
  let decision = cancel_job_states(
    BuildState::Running,
    AttemptState::Running,
    [
      (job(1), JobState::Blocked),
      (job(2), JobState::Ready),
      (job(3), JobState::Succeeded),
    ],
  )
  .unwrap();

  assert_eq!(decision.attempt_state(), AttemptState::Cancelled);
  assert_eq!(decision.build_state(), BuildState::Cancelled);
}

#[test]
fn cancellation_rejects_terminal_aggregate_state() {
  assert_eq!(
    cancel_job_states(BuildState::Failed, AttemptState::Failed, [(job(1), JobState::Failed)]),
    Err(CancellationError::BuildNotActive)
  );
}
