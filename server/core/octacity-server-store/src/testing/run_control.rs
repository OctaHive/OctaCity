use super::*;

pub(super) async fn cancel(store: &InMemoryStore, request: CancelBuild) -> Result<CancellationDisposition, StoreError> {
  let mut state = MemoryTransaction::begin(store.lock()?);
  if let Some(build_id) = state.cancellation_keys.get(&request.idempotency_key) {
    if *build_id != request.build_id {
      return Err(StoreError::Conflict {
        entity: EntityKind::Build,
      });
    }
    let mut outcome = state
      .cancellations
      .get(build_id)
      .ok_or(StoreError::Unavailable)?
      .outcome
      .clone();
    outcome.disposition = MutationDisposition::Replayed;
    return Ok(outcome);
  }
  if let Some(existing) = state.cancellations.get(&request.build_id) {
    let mut outcome = existing.outcome.clone();
    outcome.disposition = MutationDisposition::Replayed;
    let evidence_identity = format!("cancel-build:{}", request.idempotency_key);
    ensure_evidence_available(&state, &evidence_identity)?;
    state
      .cancellation_keys
      .insert(request.idempotency_key, request.build_id);
    record_evidence(&mut state, evidence_identity);
    state.commit();
    return Ok(outcome);
  }
  let build_state = *state.builds.get(&request.build_id).ok_or(StoreError::NotFound {
    entity: EntityKind::Build,
  })?;
  let (attempt_id, attempt) = latest_attempt(&state, request.build_id)?;
  let attempt_state = attempt.state;
  let decision = cancel_job_states(
    build_state,
    attempt_state,
    state
      .jobs
      .iter()
      .filter_map(|(job_id, job)| (job.attempt_id == attempt_id).then_some((*job_id, job.state))),
  )
  .map_err(|error| match error {
    octacity_server_orchestrator::CancellationError::BuildNotActive => StoreError::Conflict {
      entity: EntityKind::Build,
    },
    octacity_server_orchestrator::CancellationError::AttemptNotActive => StoreError::Conflict {
      entity: EntityKind::Attempt,
    },
    octacity_server_orchestrator::CancellationError::EmptyAttempt
    | octacity_server_orchestrator::CancellationError::DuplicateJob { .. }
    | octacity_server_orchestrator::CancellationError::InvalidTransition { .. } => StoreError::Unavailable,
  })?;
  let cancelled_jobs: Vec<_> = decision
    .transitions()
    .iter()
    .filter_map(|transition| (transition.state() == JobState::Cancelled).then_some(transition.job_id()))
    .collect();
  let cancelling_jobs: Vec<_> = decision
    .transitions()
    .iter()
    .filter_map(|transition| (transition.state() == JobState::Cancelling).then_some(transition.job_id()))
    .collect();
  let outcome = CancellationDisposition {
    disposition: MutationDisposition::Applied,
    build_id: request.build_id,
    attempt_id,
    cancelled_jobs,
    cancelling_jobs,
    attempt_state: decision.attempt_state(),
    build_state: decision.build_state(),
  };
  let evidence_identity = format!("cancel-build:{}", request.idempotency_key);
  ensure_evidence_available(&state, &evidence_identity)?;
  for transition in decision.transitions() {
    state
      .jobs
      .get_mut(&transition.job_id())
      .ok_or(StoreError::Unavailable)?
      .state = transition.state();
    if transition.state() == JobState::Cancelled {
      state.ready_jobs.remove(&transition.job_id());
      state.ready_queue.retain(|entry| entry.job_id != transition.job_id());
    }
  }
  state
    .attempts
    .get_mut(&attempt_id)
    .ok_or(StoreError::Unavailable)?
    .state = decision.attempt_state();
  *state.builds.get_mut(&request.build_id).ok_or(StoreError::Unavailable)? = decision.build_state();
  state
    .cancellation_keys
    .insert(request.idempotency_key.clone(), request.build_id);
  state.cancellations.insert(
    request.build_id,
    CancellationRecord {
      outcome: outcome.clone(),
    },
  );
  record_evidence(&mut state, evidence_identity);
  state.commit();
  Ok(outcome)
}

pub(super) async fn retry(store: &InMemoryStore, request: RetryBuild) -> Result<RetryDisposition, StoreError> {
  request.validate()?;
  let mut state = MemoryTransaction::begin(store.lock()?);
  if let Some(existing) = state.retries.get(&request.idempotency_key) {
    if same_retry(&existing.request, &request) {
      let mut outcome = existing.outcome.clone();
      outcome.disposition = MutationDisposition::Replayed;
      return Ok(outcome);
    }
    return Err(StoreError::Conflict {
      entity: EntityKind::Build,
    });
  }
  let current_build_state = *state.builds.get(&request.build_id).ok_or(StoreError::NotFound {
    entity: EntityKind::Build,
  })?;
  let (source_attempt_id, source_attempt) = latest_attempt(&state, request.build_id)?;
  let retry_decision =
    decide_retry(current_build_state, source_attempt.state, source_attempt.number).map_err(|error| match error {
      octacity_server_orchestrator::RetryDecisionError::BuildNotFailed => StoreError::Conflict {
        entity: EntityKind::Build,
      },
      octacity_server_orchestrator::RetryDecisionError::AttemptNotFailed => StoreError::Conflict {
        entity: EntityKind::Attempt,
      },
      octacity_server_orchestrator::RetryDecisionError::AttemptNumberOverflow => StoreError::Unavailable,
    })?;
  let next_number = retry_decision.attempt_number();
  if request.attempt_number != next_number {
    return Err(StoreError::Conflict {
      entity: EntityKind::Attempt,
    });
  }
  if state.attempts.contains_key(&request.attempt_id) || request.jobs.iter().any(|job| state.jobs.contains_key(&job.id))
  {
    return Err(StoreError::Conflict {
      entity: EntityKind::Attempt,
    });
  }
  let source_jobs: Vec<_> = state
    .jobs
    .values()
    .filter(|job| job.attempt_id == source_attempt_id)
    .map(|job| job.materialized.clone())
    .collect();
  if !retry_graph_is_equivalent(&source_jobs, &request.jobs) {
    return Err(StoreError::Conflict {
      entity: EntityKind::Attempt,
    });
  }
  let ready_jobs: Vec<_> = request
    .jobs
    .iter()
    .filter(|job| job.dependencies.is_empty())
    .map(|job| job.id)
    .collect();
  let outcome = RetryDisposition {
    disposition: MutationDisposition::Applied,
    build_id: request.build_id,
    source_attempt_id,
    attempt_id: request.attempt_id,
    attempt_number: next_number,
    ready_jobs: ready_jobs.clone(),
  };
  let evidence_identity = format!("retry-build:{}", request.idempotency_key);
  ensure_evidence_available(&state, &evidence_identity)?;
  ensure_enqueue_capacity(&state, ready_jobs.iter().copied())?;
  state.attempts.insert(
    request.attempt_id,
    MemoryAttempt {
      build_id: request.build_id,
      number: next_number,
      state: retry_decision.attempt_state(),
    },
  );
  for job in &request.jobs {
    state.jobs.insert(
      job.id,
      MemoryJob {
        attempt_id: request.attempt_id,
        materialized: job.clone(),
        state: if job.dependencies.is_empty() {
          JobState::Ready
        } else {
          JobState::Blocked
        },
      },
    );
  }
  for job_id in ready_jobs {
    sign_ready_job(&mut state, &store.job_spec_signer, job_id, request.requested_at)?;
    enqueue(&mut state, job_id);
  }
  *state.builds.get_mut(&request.build_id).ok_or(StoreError::Unavailable)? = retry_decision.build_state();
  state.retries.insert(
    request.idempotency_key.clone(),
    RetryRecord {
      request,
      outcome: outcome.clone(),
    },
  );
  record_evidence(&mut state, evidence_identity);
  state.commit();
  Ok(outcome)
}

fn latest_attempt(state: &MemoryState, build_id: BuildId) -> Result<(AttemptId, &MemoryAttempt), StoreError> {
  state
    .attempts
    .iter()
    .filter(|(_, attempt)| attempt.build_id == build_id)
    .max_by_key(|(_, attempt)| attempt.number)
    .map(|(attempt_id, attempt)| (*attempt_id, attempt))
    .ok_or(StoreError::NotFound {
      entity: EntityKind::Attempt,
    })
}

fn same_retry(left: &RetryBuild, right: &RetryBuild) -> bool {
  left.build_id == right.build_id
    && left.attempt_id == right.attempt_id
    && left.attempt_number == right.attempt_number
    && left.jobs == right.jobs
}
