use octacity_server_domain::AttemptNumber;
use octacity_server_orchestrator::{AttemptState, BuildState, RetryDecisionError, decide_retry};

#[test]
fn retry_allocates_the_next_attempt_and_reactivates_the_build() {
  let decision = decide_retry(BuildState::Failed, AttemptState::Failed, AttemptNumber::FIRST).unwrap();

  assert_eq!(decision.attempt_number().get(), 2);
  assert_eq!(decision.attempt_state(), AttemptState::Running);
  assert_eq!(decision.build_state(), BuildState::Running);
}

#[test]
fn retry_requires_the_latest_build_and_attempt_to_have_failed() {
  assert_eq!(
    decide_retry(BuildState::Running, AttemptState::Failed, AttemptNumber::FIRST),
    Err(RetryDecisionError::BuildNotFailed)
  );
  assert_eq!(
    decide_retry(BuildState::Failed, AttemptState::Cancelled, AttemptNumber::FIRST),
    Err(RetryDecisionError::AttemptNotFailed)
  );
}
