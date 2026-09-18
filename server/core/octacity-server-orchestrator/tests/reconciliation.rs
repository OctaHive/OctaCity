use octacity_server_domain::JobId;
use octacity_server_job::JobState;
use octacity_server_orchestrator::{AttemptState, BuildState, JobGraphNode, reconcile_job_graph};
use octacity_server_pipeline::DependencyPolicy;

fn job(value: u128) -> JobId {
  JobId::from_uuid(uuid::Uuid::from_u128(value)).unwrap()
}

fn node(value: u128, state: JobState, dependencies: &[u128], policy: DependencyPolicy) -> JobGraphNode {
  JobGraphNode::new(
    job(value),
    state,
    dependencies.iter().copied().map(job).collect(),
    policy,
  )
  .unwrap()
}

#[test]
fn cascades_skips_and_derives_terminal_failure() {
  let decision = reconcile_job_graph([
    node(4, JobState::Blocked, &[3], DependencyPolicy::AllSucceeded),
    node(1, JobState::Failed, &[], DependencyPolicy::AllSucceeded),
    node(3, JobState::Blocked, &[2], DependencyPolicy::AllSucceeded),
    node(2, JobState::Blocked, &[1], DependencyPolicy::AllSucceeded),
  ])
  .unwrap();

  assert_eq!(
    transitions(&decision),
    [
      (job(2), JobState::Skipped),
      (job(3), JobState::Skipped),
      (job(4), JobState::Skipped)
    ]
  );
  assert_eq!(decision.attempt_state(), AttemptState::Failed);
  assert_eq!(decision.build_state(), BuildState::Failed);
}

#[test]
fn applies_each_failure_policy_and_is_stable_on_replay() {
  let graph = [
    node(1, JobState::Failed, &[], DependencyPolicy::AllSucceeded),
    node(2, JobState::Succeeded, &[], DependencyPolicy::AllSucceeded),
    node(3, JobState::Blocked, &[1, 2], DependencyPolicy::AllCompleted),
    node(4, JobState::Blocked, &[1, 2], DependencyPolicy::AnySucceeded),
    node(5, JobState::Blocked, &[1, 2], DependencyPolicy::AllSucceeded),
  ];
  let decision = reconcile_job_graph(graph).unwrap();
  assert_eq!(
    transitions(&decision),
    [
      (job(3), JobState::Ready),
      (job(4), JobState::Ready),
      (job(5), JobState::Skipped)
    ]
  );
  assert_eq!(decision.attempt_state(), AttemptState::Running);

  let replay = reconcile_job_graph([
    node(1, JobState::Failed, &[], DependencyPolicy::AllSucceeded),
    node(2, JobState::Succeeded, &[], DependencyPolicy::AllSucceeded),
    node(3, JobState::Ready, &[1, 2], DependencyPolicy::AllCompleted),
    node(4, JobState::Ready, &[1, 2], DependencyPolicy::AnySucceeded),
    node(5, JobState::Skipped, &[1, 2], DependencyPolicy::AllSucceeded),
  ])
  .unwrap();
  assert!(replay.job_transitions().is_empty());
  assert_eq!(replay.attempt_state(), AttemptState::Running);
  assert_eq!(replay.build_state(), BuildState::Running);
}

#[test]
fn derives_success_only_after_every_job_is_terminal() {
  let running = reconcile_job_graph([
    node(1, JobState::Succeeded, &[], DependencyPolicy::AllSucceeded),
    node(2, JobState::Ready, &[1], DependencyPolicy::AllSucceeded),
  ])
  .unwrap();
  assert_eq!(running.attempt_state(), AttemptState::Running);

  let succeeded = reconcile_job_graph([
    node(1, JobState::Succeeded, &[], DependencyPolicy::AllSucceeded),
    node(2, JobState::Succeeded, &[1], DependencyPolicy::AllSucceeded),
  ])
  .unwrap();
  assert_eq!(succeeded.attempt_state(), AttemptState::Succeeded);
  assert_eq!(succeeded.build_state(), BuildState::Succeeded);
}

#[test]
fn derives_cancelled_when_cancellation_finishes_without_a_failure() {
  let decision = reconcile_job_graph([
    node(1, JobState::Succeeded, &[], DependencyPolicy::AllSucceeded),
    node(2, JobState::Cancelled, &[], DependencyPolicy::AllSucceeded),
  ])
  .unwrap();

  assert_eq!(decision.attempt_state(), AttemptState::Cancelled);
  assert_eq!(decision.build_state(), BuildState::Cancelled);
}

#[test]
fn durable_cancellation_intent_dominates_a_late_failure() {
  let decision = octacity_server_orchestrator::reconcile_cancelled_job_graph([
    node(1, JobState::Failed, &[], DependencyPolicy::AllSucceeded),
    node(2, JobState::Cancelled, &[], DependencyPolicy::AllSucceeded),
  ])
  .unwrap();

  assert_eq!(decision.attempt_state(), AttemptState::Cancelled);
  assert_eq!(decision.build_state(), BuildState::Cancelled);
}

fn transitions(decision: &octacity_server_orchestrator::OrchestrationDecision) -> Vec<(JobId, JobState)> {
  decision
    .job_transitions()
    .iter()
    .map(|transition| (transition.job_id(), transition.state()))
    .collect()
}
