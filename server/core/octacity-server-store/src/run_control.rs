use std::collections::BTreeMap;

use octacity_server_domain::{AttemptId, AttemptNumber, BuildId, JobId, PipelineNodeId, Timestamp};
use octacity_server_orchestrator::{AttemptState, BuildState};

use crate::{IdempotencyKey, MaterializedJob, MutationDisposition, StoreError, StoreInputError, StoreOperation};

/// Maximum encoded bytes in one retry graph.
pub const MAX_RETRY_BUILD_BYTES: usize = 8 * 1_024 * 1_024;

/// Complete atomic input for durable Build cancellation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelBuild {
  /// Build receiving durable cancellation intent.
  pub build_id: BuildId,
  /// Caller identity used for exact replay and mismatched-key detection.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative request time.
  pub requested_at: Timestamp,
}

/// Result of propagating one Build cancellation through its current Attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancellationDisposition {
  /// Whether cancellation was newly applied or exactly replayed.
  pub disposition: MutationDisposition,
  /// Cancelled Build.
  pub build_id: BuildId,
  /// Attempt that was current when cancellation was first applied.
  pub attempt_id: AttemptId,
  /// Unowned Jobs made terminal immediately.
  pub cancelled_jobs: Vec<JobId>,
  /// Owned Jobs waiting for their current Agent to acknowledge cancellation.
  pub cancelling_jobs: Vec<JobId>,
  /// Attempt state after immediate propagation.
  pub attempt_state: AttemptState,
  /// Build state after immediate propagation.
  pub build_state: BuildState,
}

/// Complete atomic input for materializing a retry Attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetryBuild {
  /// Failed Build whose immutable snapshots are reused.
  pub build_id: BuildId,
  /// Caller-generated identity for the new Attempt.
  pub attempt_id: AttemptId,
  /// Expected next positive Attempt number bound when ready JobSpecs are signed.
  pub attempt_number: AttemptNumber,
  /// Candidate graph; the store verifies every snapshot against the prior Attempt.
  pub jobs: Vec<MaterializedJob>,
  /// Caller identity used for exact replay and mismatched-key detection.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative retry time.
  pub requested_at: Timestamp,
}

impl RetryBuild {
  /// Creates a canonical retry request and validates its candidate graph.
  pub fn new(
    build_id: BuildId,
    attempt_id: AttemptId,
    attempt_number: AttemptNumber,
    mut jobs: Vec<MaterializedJob>,
    idempotency_key: IdempotencyKey,
    requested_at: Timestamp,
  ) -> Result<Self, StoreError> {
    jobs.sort_unstable_by_key(|job| job.id);
    let request = Self {
      build_id,
      attempt_id,
      attempt_number,
      jobs,
      idempotency_key,
      requested_at,
    };
    request.validate()?;
    Ok(request)
  }

  /// Revalidates bounded graph shape and stable JobSpec-template bindings.
  pub fn validate(&self) -> Result<(), StoreError> {
    if self.attempt_number == AttemptNumber::FIRST {
      return Err(StoreError::invalid(
        StoreOperation::RetryBuild,
        StoreInputError::InvalidRetryAttempt,
      ));
    }
    crate::model::validate_materialized_jobs(StoreOperation::RetryBuild, self.build_id, &self.jobs)?;
    if serde_json::to_vec(&self.jobs)
      .ok()
      .is_none_or(|encoded| encoded.len() > MAX_RETRY_BUILD_BYTES)
    {
      return Err(StoreError::invalid(
        StoreOperation::RetryBuild,
        StoreInputError::RequestTooLarge,
      ));
    }
    Ok(())
  }
}

/// Result of atomically materializing one retry Attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetryDisposition {
  /// Whether the retry was newly applied or exactly replayed.
  pub disposition: MutationDisposition,
  /// Retried Build.
  pub build_id: BuildId,
  /// Failed Attempt retained as retry causality.
  pub source_attempt_id: AttemptId,
  /// Newly materialized Attempt.
  pub attempt_id: AttemptId,
  /// Authoritatively allocated next Attempt number.
  pub attempt_number: AttemptNumber,
  /// Root Jobs inserted into the durable ready queue.
  pub ready_jobs: Vec<JobId>,
}

/// Compares two materialized Attempts by immutable Pipeline-node semantics.
///
/// Job identities may differ between Attempts. Every other execution,
/// placement, dependency-policy, and dependency-topology fact must be exactly
/// equal. Store adapters share this decision so backend implementations cannot
/// drift on what a retry is allowed to change.
#[must_use]
pub fn retry_graph_is_equivalent(source: &[MaterializedJob], candidate: &[MaterializedJob]) -> bool {
  let source_by_node = jobs_by_node(source);
  let candidate_by_node = jobs_by_node(candidate);
  let source_nodes_by_id = nodes_by_job_id(source);
  let candidate_nodes_by_id = nodes_by_job_id(candidate);
  source_by_node.len() == candidate_by_node.len()
    && candidate_by_node.iter().all(|(node_id, candidate_job)| {
      source_by_node.get(node_id).is_some_and(|source_job| {
        source_job.dependency_policy == candidate_job.dependency_policy
          && source_job.allowed_pools == candidate_job.allowed_pools
          && source_job.requirements == candidate_job.requirements
          && source_job.job_spec_template == candidate_job.job_spec_template
          && dependency_nodes(source_job, &source_nodes_by_id)
            == dependency_nodes(candidate_job, &candidate_nodes_by_id)
      })
    })
}

fn jobs_by_node(jobs: &[MaterializedJob]) -> BTreeMap<&PipelineNodeId, &MaterializedJob> {
  jobs.iter().map(|job| (&job.pipeline_node_id, job)).collect()
}

fn nodes_by_job_id(jobs: &[MaterializedJob]) -> BTreeMap<JobId, &PipelineNodeId> {
  jobs.iter().map(|job| (job.id, &job.pipeline_node_id)).collect()
}

fn dependency_nodes<'a>(
  job: &'a MaterializedJob,
  nodes_by_job_id: &BTreeMap<JobId, &'a PipelineNodeId>,
) -> Option<Vec<&'a PipelineNodeId>> {
  let mut dependencies = job
    .dependencies
    .iter()
    .map(|job_id| nodes_by_job_id.get(job_id).copied())
    .collect::<Option<Vec<_>>>()?;
  dependencies.sort_unstable();
  Some(dependencies)
}
