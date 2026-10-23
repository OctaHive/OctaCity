//! Factory admission uses the same ordinary memory Build/Attempt/Job state and queue.
use super::*;
use crate::{AcceptFactoryBuild, AcceptedFactoryBuild, FactoryBuildAcceptanceStore};
use octacity_server_factory::FactoryDigest;

impl InMemoryStore {
  /// Seeds a retained failed Attempt for management-control contracts, without active Leases.
  pub fn seed_failed_factory_build(&self, build: BuildId) -> Result<(), StoreError> {
    let mut state = self.lock()?;
    let accepted = state
      .factory_builds
      .values()
      .find(|(_, accepted)| accepted.build_id == build)
      .map(|(_, accepted)| accepted.clone())
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Build,
      })?;
    *state.builds.get_mut(&build).ok_or(StoreError::Unavailable)? = BuildState::Failed;
    state
      .attempts
      .get_mut(&accepted.attempt_id)
      .ok_or(StoreError::Unavailable)?
      .state = AttemptState::Failed;
    for job in &accepted.job_ids {
      let retained = state.jobs.get_mut(job).ok_or(StoreError::Unavailable)?;
      retained.state = if retained.materialized.dependencies.is_empty() {
        JobState::Failed
      } else {
        JobState::Skipped
      };
      state.ready_jobs.remove(job);
      state.ready_queue.retain(|entry| entry.job_id != *job);
    }
    Ok(())
  }
}

#[async_trait]
impl FactoryBuildAcceptanceStore for InMemoryStore {
  async fn factory_build_for_operation(
    &self,
    operation: FactoryDigest,
  ) -> Result<Option<AcceptedFactoryBuild>, StoreError> {
    Ok(
      self
        .lock()?
        .factory_builds
        .get(&operation)
        .map(|(_, value)| value.clone()),
    )
  }
  async fn accept_factory_build(&self, request: AcceptFactoryBuild) -> Result<AcceptedFactoryBuild, StoreError> {
    request.validate()?;
    let operation = request.intent.operation_id().map_err(|_| StoreError::Unavailable)?;
    let conflict = || StoreError::Conflict {
      entity: EntityKind::Build,
    };
    let authority = self.factory_authority.as_ref().ok_or(StoreError::Unavailable)?;
    let catalog = authority.lock()?;
    let mut state = MemoryTransaction::begin(self.lock()?);
    if let Some((previous, accepted)) = state.factory_builds.get(&operation) {
      if previous.intent != request.intent
        || previous.build != request.build
        || previous.jobs != request.jobs
        || previous.attempt_id != request.attempt_id
      {
        return Err(conflict());
      }
      return Ok(accepted.clone());
    }
    request.validate_fresh()?;
    let run = catalog
      .factory_runs
      .get(&request.intent.input().factory_run_id())
      .ok_or(StoreError::Unavailable)?
      .snapshot()?;
    crate::factory_role_package_testing::verify_dispatch(
      &run,
      &request.intent,
      &request.ownership,
      request.accepted_at,
    )?;
    if let Some(invocation) = catalog.role_invocations.get(&operation) {
      let role = invocation.package.inputs().role.reference();
      let current = catalog
        .role_profiles
        .roles
        .get(&role.id)
        .and_then(|versions| versions.last_key_value())
        .map(|(_, role)| role)
        .filter(|current| current.reference().project_id == role.project_id)
        .ok_or(StoreError::Unavailable)?;
      if let Some(limit) = current.definition().concurrency_limit {
        let active = state
          .factory_builds
          .iter()
          .filter(|(operation, (_, accepted))| {
            state
              .builds
              .get(&accepted.build_id)
              .is_some_and(|state| !state.is_terminal())
              && catalog.role_invocations.get(operation).is_some_and(|invocation| {
                let selected = invocation.package.inputs().role.reference();
                selected.project_id == role.project_id && selected.id == role.id
              })
          })
          .count();
        if active >= limit.get() as usize {
          return Err(StoreError::CapacityUnavailable);
        }
      }
    }
    if state.builds.contains_key(&request.build.id)
      || state.attempts.contains_key(&request.attempt_id)
      || request.jobs.iter().any(|job| state.jobs.contains_key(&job.id))
    {
      return Err(conflict());
    }
    if request
      .jobs
      .iter()
      .flat_map(|job| &job.allowed_pools)
      .any(|pool| !state.pools.keys().any(|(id, _)| id == pool))
    {
      return Err(StoreError::NotFound {
        entity: EntityKind::Pool,
      });
    }
    let roots: Vec<_> = request
      .jobs
      .iter()
      .filter(|job| job.dependencies.is_empty())
      .map(|job| job.id)
      .collect();
    ensure_enqueue_capacity(&state, roots.iter().copied())?;
    let accepted = AcceptedFactoryBuild {
      build_id: request.build.id,
      attempt_id: request.attempt_id,
      job_ids: request.jobs.iter().map(|job| job.id).collect(),
      effective_policy_digest: request.intent.permissions().digest(),
    };
    insert_ordinary_build(
      &mut state,
      &self.job_spec_signer,
      request.build.id,
      (request.attempt_id, AttemptNumber::FIRST),
      &request.jobs,
      request.accepted_at,
      roots,
    )?;
    state.factory_builds.insert(operation, (request, accepted.clone()));
    state.commit();
    Ok(accepted)
  }
}
