use std::collections::BTreeMap;

use octacity_server_domain::JobId;
use octacity_server_pipeline::{DependencyOutcome, DependencyPolicy};

/// One persisted predecessor outcome observed for a blocked Job.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DependencyObservation {
  /// Blocked Job whose fan-in is being evaluated.
  pub job_id: JobId,
  /// Immutable dependency policy captured with the materialized Job.
  pub policy: DependencyPolicy,
  /// Current terminal outcome of one predecessor, or [`DependencyOutcome::Pending`].
  pub outcome: DependencyOutcome,
}

/// Failure to derive a deterministic DAG readiness decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DagDecisionError {
  /// Persisted edges for one Job disagree about its immutable policy.
  InconsistentDependencyPolicy {
    /// Job whose persisted policy snapshot is inconsistent.
    job_id: JobId,
  },
}

/// Returns blocked Jobs whose complete persisted fan-in satisfies its policy.
///
/// Persistence adapters supply observations while the Orchestrator owns the
/// policy decision. The result is stably ordered by Job identity so retries and
/// different adapters produce the same transition set.
pub fn newly_ready_jobs(
  observations: impl IntoIterator<Item = DependencyObservation>,
) -> Result<Vec<JobId>, DagDecisionError> {
  let mut fan_ins: BTreeMap<JobId, (DependencyPolicy, Vec<DependencyOutcome>)> = BTreeMap::new();
  for observation in observations {
    match fan_ins.entry(observation.job_id) {
      std::collections::btree_map::Entry::Vacant(entry) => {
        entry.insert((observation.policy, vec![observation.outcome]));
      }
      std::collections::btree_map::Entry::Occupied(mut entry) => {
        if entry.get().0 != observation.policy {
          return Err(DagDecisionError::InconsistentDependencyPolicy {
            job_id: observation.job_id,
          });
        }
        entry.get_mut().1.push(observation.outcome);
      }
    }
  }

  Ok(
    fan_ins
      .into_iter()
      .filter_map(|(job_id, (policy, outcomes))| policy.is_satisfied(outcomes).then_some(job_id))
      .collect(),
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  fn job(value: u128) -> JobId {
    JobId::from_uuid(uuid::Uuid::from_u128(value)).unwrap()
  }

  fn observation(job_id: JobId, outcome: DependencyOutcome) -> DependencyObservation {
    DependencyObservation {
      job_id,
      policy: DependencyPolicy::AllSucceeded,
      outcome,
    }
  }

  #[test]
  fn fan_in_is_ready_only_after_every_predecessor_succeeds() {
    let ready = newly_ready_jobs([
      observation(job(2), DependencyOutcome::Succeeded),
      observation(job(1), DependencyOutcome::Succeeded),
      observation(job(2), DependencyOutcome::Pending),
    ])
    .unwrap();
    assert_eq!(ready, [job(1)]);
  }

  #[test]
  fn readiness_is_stably_ordered_by_job_identity() {
    let observations = [
      observation(job(3), DependencyOutcome::Succeeded),
      observation(job(1), DependencyOutcome::Succeeded),
      observation(job(2), DependencyOutcome::Succeeded),
    ];
    assert_eq!(newly_ready_jobs(observations).unwrap(), [job(1), job(2), job(3)]);
  }
}
