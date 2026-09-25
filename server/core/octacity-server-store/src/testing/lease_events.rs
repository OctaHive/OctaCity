use super::*;
use crate::StoreInputError;

pub(super) async fn claim(store: &InMemoryStore, request: JobClaim) -> Result<JobClaimOutcome, StoreError> {
  request.validate()?;
  let mut state = MemoryTransaction::begin(store.lock()?);
  if let Some(existing) = state.claims.get(&request.lease_id) {
    if same_job_claim(&existing.request, &request) {
      return Ok(existing.outcome.clone());
    }
    return Err(StoreError::Conflict {
      entity: EntityKind::Lease,
    });
  }
  let Some(registration) = state.registrations.get(&(request.agent_id, request.registration_epoch)) else {
    return Ok(JobClaimOutcome::Empty);
  };
  let snapshot_is_valid = registration.inventory.as_ref().is_some_and(|inventory| {
    request.snapshot.active_job.is_none() && request.snapshot.validate(&inventory.host_capacity).is_ok()
  });
  if !snapshot_is_valid {
    return Err(StoreError::invalid(
      StoreOperation::ClaimReadyJob,
      StoreInputError::InvalidAgentInventory,
    ));
  }
  let pool = state.pools.get(&(request.pool_id, registration.pool_version));
  if registration.pool_id != request.pool_id
    || registration.revoked
    || registration.expires_at <= request.claimed_at
    || !pool.is_some_and(|pool| pool.enabled && pool.accepting)
  {
    return Ok(JobClaimOutcome::Empty);
  }
  let pool = pool.expect("accepting Pool was checked");
  let active_grants = state
    .current_lease_by_job
    .values()
    .filter_map(|lease_id| state.leases.get(lease_id));
  let mut active_in_pool = 0_usize;
  for grant in active_grants {
    if grant.agent_id == request.agent_id {
      return Ok(JobClaimOutcome::Empty);
    }
    if grant.pool_id == request.pool_id {
      active_in_pool += 1;
    }
  }
  if active_in_pool >= pool.concurrency_limit {
    return Ok(JobClaimOutcome::Empty);
  }
  let selected = state.ready_queue.iter().copied().find(|entry| {
    state.jobs.get(&entry.job_id).is_some_and(|job| {
      job.state == JobState::Ready
        && job.materialized.allowed_pools.binary_search(&request.pool_id).is_ok()
        && registration.inventory.as_ref().is_some_and(|inventory| {
          octacity_server_scheduler::is_compatible(
            inventory,
            &request.snapshot,
            &job.materialized.requirements,
            &job.materialized.job_spec_template,
          )
        })
    })
  });
  let Some(selected) = selected else {
    return Ok(JobClaimOutcome::Empty);
  };
  let job = state.jobs.get(&selected.job_id).ok_or(StoreError::Unavailable)?;
  let attempt = state
    .attempts
    .get(&job.attempt_id)
    .ok_or(StoreError::Unavailable)?
    .number;
  let signed_job_spec = state
    .signed_job_specs
    .get(&selected.job_id)
    .ok_or(StoreError::Unavailable)?
    .envelope()
    .clone();
  let grant = LeaseGrant {
    lease_id: request.lease_id,
    fence: request.fence,
    job_id: selected.job_id,
    attempt,
    agent_id: request.agent_id,
    registration_epoch: request.registration_epoch,
    pool_id: request.pool_id,
    claimed_at: request.claimed_at,
    expires_at: request.expires_at,
    signed_job_spec,
  };
  let evidence_identity = format!("claim-ready-job:{}", request.lease_id);
  ensure_evidence_available(&state, &evidence_identity)?;
  state.ready_queue.remove(&selected);
  state.ready_jobs.remove(&selected.job_id);
  state
    .jobs
    .get_mut(&selected.job_id)
    .ok_or(StoreError::Unavailable)?
    .state = JobState::Leased;
  state.current_lease_by_job.insert(selected.job_id, request.lease_id);
  state.leases.insert(request.lease_id, grant.clone());
  state
    .lease_states
    .insert(request.lease_id, octacity_server_scheduler::LeaseState::Active);
  state.claims.insert(
    request.lease_id,
    ClaimRecord {
      request,
      outcome: JobClaimOutcome::Claimed(Box::new(grant.clone())),
    },
  );
  record_evidence(&mut state, evidence_identity);
  state.commit();
  Ok(JobClaimOutcome::Claimed(Box::new(grant)))
}

pub(super) async fn renew(store: &InMemoryStore, request: RenewLease) -> Result<LeaseHeartbeatOutcome, StoreError> {
  request.validate()?;
  let mut state = MemoryTransaction::begin(store.lock()?);
  if let Some(existing) = state.lease_heartbeats.get(&request.idempotency_key) {
    if same_heartbeat_intent(&existing.request, &request) {
      return Ok(existing.outcome);
    }
    return Err(StoreError::Conflict {
      entity: EntityKind::Lease,
    });
  }
  let grant = match current_grant(&state, request.lease, request.observed_at) {
    Ok(grant) if grant.job_id == request.job_id && grant.attempt == request.attempt => grant,
    Ok(_) | Err(StoreError::Fenced { .. } | StoreError::Expired { .. }) => {
      return Ok(LeaseHeartbeatOutcome::Fenced);
    }
    Err(error) => return Err(error),
  };
  let lease_state = state
    .lease_states
    .get(&grant.lease_id)
    .copied()
    .unwrap_or(octacity_server_scheduler::LeaseState::Fenced);
  let outcome = match lease_state {
    octacity_server_scheduler::LeaseState::Active => {
      state
        .leases
        .get_mut(&grant.lease_id)
        .ok_or(StoreError::Unavailable)?
        .expires_at = request.expires_at;
      LeaseHeartbeatOutcome::Continue {
        expires_at: request.expires_at,
      }
    }
    octacity_server_scheduler::LeaseState::CancellationRequested => LeaseHeartbeatOutcome::Cancel,
    octacity_server_scheduler::LeaseState::DrainRequested => {
      state
        .leases
        .get_mut(&grant.lease_id)
        .ok_or(StoreError::Unavailable)?
        .expires_at = request.expires_at;
      LeaseHeartbeatOutcome::Drain {
        expires_at: request.expires_at,
      }
    }
    octacity_server_scheduler::LeaseState::Released
    | octacity_server_scheduler::LeaseState::Expired
    | octacity_server_scheduler::LeaseState::Fenced
    | octacity_server_scheduler::LeaseState::Completed => LeaseHeartbeatOutcome::Fenced,
  };
  let evidence_identity = format!("renew-lease:{}", request.idempotency_key);
  ensure_evidence_available(&state, &evidence_identity)?;
  state
    .lease_heartbeats
    .insert(request.idempotency_key.clone(), HeartbeatRecord { request, outcome });
  record_evidence(&mut state, evidence_identity);
  state.commit();
  Ok(outcome)
}

pub(super) async fn append_events(
  store: &InMemoryStore,
  request: AppendJobEvents,
) -> Result<AppendJobEventsOutcome, StoreError> {
  request.validate()?;
  let mut state = MemoryTransaction::begin(store.lock()?);
  let first = request.events[0].sequence();
  let last = request.events[request.events.len() - 1].sequence();
  let append_key = (request.lease.lease_id, first, last);
  if let Some(existing) = state.event_appends.get(&append_key) {
    if same_event_append(&existing.request, &request) {
      return Ok(existing.outcome);
    }
    return Err(StoreError::Conflict {
      entity: EntityKind::Job,
    });
  }
  let grant = current_grant(&state, request.lease, request.accepted_at)?;
  let events = state.events.get(&grant.job_id);
  let durable_through = events
    .and_then(|events| events.last_key_value().map(|(sequence, _)| sequence.get()))
    .unwrap_or(0);
  let mut expected = durable_through.saturating_add(1);
  let mut new_events = Vec::new();

  for event in &request.events {
    let sequence = event.sequence().get();
    if sequence <= durable_through {
      if events.and_then(|events| events.get(&event.sequence())) != Some(&event.digest()) {
        return Err(StoreError::Conflict {
          entity: EntityKind::Job,
        });
      }
    } else if sequence != expected {
      return Err(StoreError::EventGap {
        job: grant.job_id,
        expected,
        actual: sequence,
      });
    } else {
      new_events.push(event.clone());
      expected = expected.checked_add(1).ok_or(StoreError::Unavailable)?;
    }
  }
  validate_new_log_chunks(&request.events, durable_through, &request.log_chunks)?;

  let acknowledged = durable_through
    .checked_add(u64::try_from(new_events.len()).map_err(|_| StoreError::Unavailable)?)
    .ok_or(StoreError::Unavailable)?;
  let acknowledged_through = EventSequence::new(acknowledged).map_err(|source| StoreError::InvalidInput {
    operation: StoreOperation::AppendJobEvents,
    source,
  })?;
  let outcome = AppendJobEventsOutcome {
    acknowledged_through,
    inserted: new_events.len(),
  };
  let evidence_identity = format!(
    "append-job-events:{}:{}-{}",
    append_key.0,
    append_key.1.get(),
    append_key.2.get()
  );
  ensure_evidence_available(&state, &evidence_identity)?;
  if !new_events.is_empty() {
    let job = state.jobs.get_mut(&grant.job_id).ok_or(StoreError::Unavailable)?;
    job.state = start_job_execution(job.state)?;
  }
  let events = state.events.entry(grant.job_id).or_default();
  for event in &new_events {
    events.insert(event.sequence(), event.digest());
  }
  let attempt_id = state.jobs.get(&grant.job_id).ok_or(StoreError::Unavailable)?.attempt_id;
  let build_id = state.attempts.get(&attempt_id).ok_or(StoreError::Unavailable)?.build_id;
  let project_id = state
    .accepted
    .values()
    .find(|accepted| accepted.request.build.id == build_id)
    .map(|accepted| accepted.request.build.project_id)
    .ok_or(StoreError::Unavailable)?;
  for chunk in &request.log_chunks {
    match state.log_chunks.get(&chunk.chunk_id()) {
      Some(existing) if existing != chunk => {
        return Err(StoreError::Conflict {
          entity: EntityKind::LogChunk,
        });
      }
      Some(_) => {}
      None => {
        let next = state
          .committed_log_index_positions
          .get(&project_id)
          .map_or(1, |position| position.get().saturating_add(1));
        let next = LogIndexPosition::new(next).map_err(|_| StoreError::Unavailable)?;
        state.committed_log_index_positions.insert(project_id, next);
        state.log_chunks.insert(chunk.chunk_id(), chunk.clone());
      }
    }
  }
  state
    .event_appends
    .insert(append_key, EventAppendRecord { request, outcome });
  record_evidence(&mut state, evidence_identity);
  state.commit();
  Ok(outcome)
}

pub(super) async fn complete(
  store: &InMemoryStore,
  request: JobCompletion,
) -> Result<CompletionDisposition, StoreError> {
  let mut state = MemoryTransaction::begin(store.lock()?);
  let grant = lease_grant(&state, request.lease)?;
  if let Some(existing) = state.completions.get(&grant.job_id) {
    if same_completion(&existing.request, &request) {
      let mut outcome = existing.outcome.clone();
      outcome.disposition = MutationDisposition::Replayed;
      return Ok(outcome);
    }
    return Err(StoreError::Conflict {
      entity: EntityKind::Job,
    });
  }
  current_grant(&state, request.lease, request.completed_at)?;

  let durable_through = state
    .events
    .get(&grant.job_id)
    .and_then(|events| events.last_key_value().map(|(sequence, _)| sequence.get()))
    .unwrap_or(0);
  let required = request.final_sequence.map_or(0, EventSequence::get);
  if required > durable_through {
    return Err(StoreError::EventsMissing {
      job: grant.job_id,
      durable_through,
      required,
    });
  }
  if required < durable_through {
    return Err(StoreError::Conflict {
      entity: EntityKind::Job,
    });
  }

  let completed_job = state.jobs.get(&grant.job_id).ok_or(StoreError::Unavailable)?;
  let completed_state = complete_job_state(completed_job.state, request.kind)?;
  let attempt_id = completed_job.attempt_id;
  let graph = state
    .jobs
    .values()
    .filter(|job| job.attempt_id == attempt_id)
    .map(|job| {
      JobGraphNode::new(
        job.materialized.id,
        if job.materialized.id == grant.job_id {
          completed_state
        } else {
          job.state
        },
        job.materialized.dependencies.clone(),
        job.materialized.dependency_policy,
      )
    })
    .collect::<Result<Vec<_>, _>>()
    .map_err(|_| StoreError::Unavailable)?;
  let build_id = state.attempts.get(&attempt_id).ok_or(StoreError::Unavailable)?.build_id;
  let decision = if state.cancellations.contains_key(&build_id) {
    reconcile_cancelled_job_graph(graph)
  } else {
    reconcile_job_graph(graph)
  }
  .map_err(|_| StoreError::Unavailable)?;
  let ready_jobs: Vec<_> = decision
    .job_transitions()
    .iter()
    .filter_map(|transition| (transition.state() == JobState::Ready).then_some(transition.job_id()))
    .collect();
  let skipped_jobs: Vec<_> = decision
    .job_transitions()
    .iter()
    .filter_map(|transition| (transition.state() == JobState::Skipped).then_some(transition.job_id()))
    .collect();
  let ready_pools = ready_jobs
    .iter()
    .filter_map(|job_id| state.jobs.get(job_id))
    .flat_map(|job| job.materialized.allowed_pools.iter().copied())
    .collect();
  let outcome = CompletionDisposition {
    disposition: MutationDisposition::Applied,
    job_id: grant.job_id,
    failure_class: request.kind.failure_class(),
    ready_jobs: ready_jobs.clone(),
    ready_pools,
    skipped_jobs,
    attempt_state: decision.attempt_state(),
    build_state: decision.build_state(),
  };
  let evidence_identity = format!("complete-job:{}", request.completion_id);
  let orchestrator_audit_identity = format!("reconcile-job-completion:{}", request.completion_id);
  ensure_evidence_available(&state, &evidence_identity)?;
  if state.audit_facts.contains(&orchestrator_audit_identity) {
    return Err(StoreError::Unavailable);
  }
  ensure_enqueue_capacity(&state, ready_jobs.iter().copied())?;
  state.current_lease_by_job.remove(&grant.job_id);
  state.jobs.get_mut(&grant.job_id).ok_or(StoreError::Unavailable)?.state = completed_state;
  for transition in decision.job_transitions() {
    state
      .jobs
      .get_mut(&transition.job_id())
      .ok_or(StoreError::Unavailable)?
      .state = transition.state();
  }
  let attempt = state.attempts.get_mut(&attempt_id).ok_or(StoreError::Unavailable)?;
  attempt.state = decision.attempt_state();
  let build_id = attempt.build_id;
  *state.builds.get_mut(&build_id).ok_or(StoreError::Unavailable)? = decision.build_state();
  for job_id in &ready_jobs {
    sign_ready_job(&mut state, &store.job_spec_signer, *job_id, request.completed_at)?;
    enqueue(&mut state, *job_id);
  }
  state.completions.insert(
    grant.job_id,
    CompletionRecord {
      request,
      outcome: outcome.clone(),
    },
  );
  record_evidence(&mut state, evidence_identity);
  let orchestrator_audit_inserted = state.audit_facts.insert(orchestrator_audit_identity);
  debug_assert!(orchestrator_audit_inserted);
  state.commit();
  Ok(outcome)
}

fn same_job_claim(left: &JobClaim, right: &JobClaim) -> bool {
  left.lease_id == right.lease_id
    && left.fence == right.fence
    && left.agent_id == right.agent_id
    && left.registration_epoch == right.registration_epoch
    && left.pool_id == right.pool_id
}

fn same_event_append(left: &AppendJobEvents, right: &AppendJobEvents) -> bool {
  left.lease == right.lease && left.events == right.events && left.log_chunks == right.log_chunks
}

pub(super) async fn prepare_append(
  store: &InMemoryStore,
  lease: LeaseAccess,
  accepted_at: Timestamp,
) -> Result<JobEventAppendPreparation, StoreError> {
  let state = store.lock()?;
  let grant = current_grant(&state, lease, accepted_at)?;
  let durable_through = state
    .events
    .get(&grant.job_id)
    .and_then(|events| events.last_key_value().map(|(sequence, _)| sequence.get()))
    .unwrap_or(0);
  Ok(JobEventAppendPreparation { durable_through })
}

fn same_completion(left: &JobCompletion, right: &JobCompletion) -> bool {
  left.completion_id == right.completion_id
    && left.lease == right.lease
    && left.final_sequence == right.final_sequence
    && left.kind == right.kind
}

fn same_heartbeat_intent(left: &RenewLease, right: &RenewLease) -> bool {
  left.lease == right.lease && left.job_id == right.job_id && left.attempt == right.attempt
}

fn lease_grant(state: &MemoryState, access: LeaseAccess) -> Result<LeaseGrant, StoreError> {
  let grant = state
    .leases
    .get(&access.lease_id)
    .cloned()
    .ok_or(StoreError::Fenced { lease: access.lease_id })?;
  if grant.fence != access.fence
    || grant.agent_id != access.agent_id
    || grant.registration_epoch != access.registration_epoch
  {
    return Err(StoreError::Fenced { lease: access.lease_id });
  }
  Ok(grant)
}

fn current_grant(state: &MemoryState, access: LeaseAccess, observed_at: Timestamp) -> Result<LeaseGrant, StoreError> {
  let grant = lease_grant(state, access)?;
  if observed_at >= grant.expires_at {
    return Err(StoreError::Expired { lease: access.lease_id });
  }
  if !state
    .registrations
    .get(&(grant.agent_id, grant.registration_epoch))
    .is_some_and(|registration| !registration.revoked && registration.expires_at > observed_at)
  {
    return Err(StoreError::Fenced { lease: access.lease_id });
  }
  if state.current_lease_by_job.get(&grant.job_id) != Some(&grant.lease_id) {
    return Err(StoreError::Fenced { lease: access.lease_id });
  }
  Ok(grant)
}
