use octacity_server_job::JobState;
use octacity_server_orchestrator::{AttemptState, BuildState};
use octacity_server_store::StoreError;

pub(crate) const fn job_state(state: JobState) -> &'static str {
  match state {
    JobState::Blocked => "blocked",
    JobState::Ready => "ready",
    JobState::Leased => "leased",
    JobState::Running => "running",
    JobState::Cancelling => "cancelling",
    JobState::Succeeded => "succeeded",
    JobState::Failed => "failed",
    JobState::Cancelled => "cancelled",
    JobState::Skipped => "skipped",
  }
}

pub(crate) fn parse_job_state(state: &str) -> Result<JobState, StoreError> {
  match state {
    "blocked" => Ok(JobState::Blocked),
    "ready" => Ok(JobState::Ready),
    "leased" => Ok(JobState::Leased),
    "running" => Ok(JobState::Running),
    "cancelling" => Ok(JobState::Cancelling),
    "succeeded" => Ok(JobState::Succeeded),
    "failed" => Ok(JobState::Failed),
    "cancelled" => Ok(JobState::Cancelled),
    "skipped" => Ok(JobState::Skipped),
    _ => Err(StoreError::Unavailable),
  }
}

pub(crate) const fn attempt_state(state: AttemptState) -> &'static str {
  match state {
    AttemptState::Created => "created",
    AttemptState::Running => "running",
    AttemptState::Succeeded => "succeeded",
    AttemptState::Failed => "failed",
    AttemptState::Cancelled => "cancelled",
  }
}

pub(crate) fn parse_attempt_state(state: &str) -> Result<AttemptState, StoreError> {
  match state {
    "created" => Ok(AttemptState::Created),
    "running" => Ok(AttemptState::Running),
    "succeeded" => Ok(AttemptState::Succeeded),
    "failed" => Ok(AttemptState::Failed),
    "cancelled" => Ok(AttemptState::Cancelled),
    _ => Err(StoreError::Unavailable),
  }
}

pub(crate) const fn build_state(state: BuildState) -> &'static str {
  match state {
    BuildState::Queued => "queued",
    BuildState::Running => "running",
    BuildState::Succeeded => "succeeded",
    BuildState::Failed => "failed",
    BuildState::Cancelled => "cancelled",
  }
}

pub(crate) fn parse_build_state(state: &str) -> Result<BuildState, StoreError> {
  match state {
    "queued" => Ok(BuildState::Queued),
    "running" => Ok(BuildState::Running),
    "succeeded" => Ok(BuildState::Succeeded),
    "failed" => Ok(BuildState::Failed),
    "cancelled" => Ok(BuildState::Cancelled),
    _ => Err(StoreError::Unavailable),
  }
}
