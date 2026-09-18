use super::*;

pub(super) async fn replay(
  store: &InMemoryStore,
  request: TriggerAcceptanceProbe,
) -> Result<Option<TriggerEvaluationOutcome>, StoreError> {
  let state = store.lock()?;
  let existing_occurrence = existing_occurrence(&state, &request.trigger);
  let Some(existing_occurrence) = existing_occurrence else {
    return Ok(None);
  };
  if let Some(existing) = state.accepted.get(&existing_occurrence) {
    require_matching_intent(existing.request.intent_digest, request.intent_digest)?;
    let mut outcome = existing.outcome.clone();
    outcome.disposition = MutationDisposition::Replayed;
    return Ok(Some(TriggerEvaluationOutcome::Accepted(outcome)));
  }
  let existing = state
    .suppressed
    .get(&existing_occurrence)
    .ok_or(StoreError::Unavailable)?;
  require_matching_intent(existing.request.intent_digest, request.intent_digest)?;
  let mut outcome = existing.outcome;
  outcome.disposition = MutationDisposition::Replayed;
  Ok(Some(TriggerEvaluationOutcome::Suppressed(outcome)))
}

pub(super) async fn accept(store: &InMemoryStore, request: AcceptTrigger) -> Result<AcceptTriggerOutcome, StoreError> {
  request.validate()?;
  let mut state = MemoryTransaction::begin(store.lock()?);
  if !state
    .trigger_prerequisites
    .contains(&TriggerPrerequisites::from_request(&request))
  {
    return Err(StoreError::NotFound {
      entity: EntityKind::Trigger,
    });
  }
  if request
    .jobs
    .iter()
    .flat_map(|job| &job.allowed_pools)
    .any(|pool| !state.pools.keys().any(|(pool_id, _)| pool_id == pool))
  {
    return Err(StoreError::NotFound {
      entity: EntityKind::Pool,
    });
  }
  let deduplication_key = request.trigger.deduplication_key();
  let evidence_identity = format!("accept-trigger:{}", request.trigger.id);
  let existing_occurrence = existing_occurrence(&state, &request.trigger);
  if let Some(existing_occurrence) = existing_occurrence {
    if let Some(existing) = state.accepted.get(&existing_occurrence) {
      require_matching_intent(existing.request.intent_digest, request.intent_digest)?;
      let mut outcome = existing.outcome.clone();
      outcome.disposition = MutationDisposition::Replayed;
      if request.trigger.id != existing_occurrence && record_replay_evidence(&mut state, evidence_identity)? {
        state.commit();
      }
      return Ok(outcome);
    }
    return Err(StoreError::Conflict {
      entity: EntityKind::Trigger,
    });
  }

  if state.builds.contains_key(&request.build.id) {
    return Err(StoreError::Conflict {
      entity: EntityKind::Build,
    });
  }
  if state.attempts.contains_key(&request.attempt_id) {
    return Err(StoreError::Conflict {
      entity: EntityKind::Attempt,
    });
  }
  if request.jobs.iter().any(|job| state.jobs.contains_key(&job.id)) {
    return Err(StoreError::Conflict {
      entity: EntityKind::Job,
    });
  }

  let ready_jobs: Vec<_> = request
    .jobs
    .iter()
    .filter(|job| job.dependencies.is_empty())
    .map(|job| job.id)
    .collect();
  let outcome = AcceptTriggerOutcome {
    disposition: MutationDisposition::Applied,
    trigger_occurrence_id: request.trigger.id,
    build_id: request.build.id,
    attempt_id: request.attempt_id,
    ready_jobs: ready_jobs.clone(),
  };
  ensure_evidence_available(&state, &evidence_identity)?;
  ensure_enqueue_capacity(&state, ready_jobs.iter().copied())?;

  state.builds.insert(request.build.id, BuildState::Running);
  state.attempts.insert(
    request.attempt_id,
    MemoryAttempt {
      build_id: request.build.id,
      number: request.attempt_number,
      state: AttemptState::Running,
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
    sign_ready_job(&mut state, &store.job_spec_signer, job_id, request.accepted_at)?;
    enqueue(&mut state, job_id);
  }
  let occurrence_id = request.trigger.id;
  state.accepted.insert(
    occurrence_id,
    AcceptedRecord {
      request,
      outcome: outcome.clone(),
    },
  );
  state
    .evaluated_by_deduplication
    .insert(deduplication_key, occurrence_id);
  record_evidence(&mut state, evidence_identity);
  state.commit();
  Ok(outcome)
}

pub(super) async fn suppress(
  store: &InMemoryStore,
  request: SuppressTrigger,
) -> Result<SuppressTriggerOutcome, StoreError> {
  request.validate()?;
  let mut state = MemoryTransaction::begin(store.lock()?);
  if !state
    .trigger_prerequisites
    .iter()
    .any(|prerequisite| prerequisite.matches(&request.trigger))
  {
    return Err(StoreError::NotFound {
      entity: EntityKind::Trigger,
    });
  }
  let deduplication_key = request.trigger.deduplication_key();
  let evidence_identity = format!("suppress-trigger:{}", request.trigger.id);
  if let Some(existing_occurrence) = existing_occurrence(&state, &request.trigger) {
    if let Some(existing) = state.suppressed.get(&existing_occurrence) {
      require_matching_intent(existing.request.intent_digest, request.intent_digest)?;
      let mut outcome = existing.outcome;
      outcome.disposition = MutationDisposition::Replayed;
      if request.trigger.id != existing_occurrence && record_replay_evidence(&mut state, evidence_identity)? {
        state.commit();
      }
      return Ok(outcome);
    }
    return Err(StoreError::Conflict {
      entity: EntityKind::Trigger,
    });
  }

  ensure_evidence_available(&state, &evidence_identity)?;
  let occurrence_id = request.trigger.id;
  let outcome = SuppressTriggerOutcome {
    disposition: MutationDisposition::Applied,
    trigger_occurrence_id: occurrence_id,
  };
  state
    .suppressed
    .insert(occurrence_id, SuppressedRecord { request, outcome });
  state
    .evaluated_by_deduplication
    .insert(deduplication_key, occurrence_id);
  record_evidence(&mut state, evidence_identity);
  state.commit();
  Ok(outcome)
}

fn existing_occurrence(state: &MemoryState, trigger: &NormalizedTriggerOccurrence) -> Option<TriggerOccurrenceId> {
  (state.accepted.contains_key(&trigger.id) || state.suppressed.contains_key(&trigger.id))
    .then_some(trigger.id)
    .or_else(|| {
      state
        .evaluated_by_deduplication
        .get(&trigger.deduplication_key())
        .copied()
    })
}

fn require_matching_intent(existing: TriggerIntentDigest, requested: TriggerIntentDigest) -> Result<(), StoreError> {
  if existing == requested {
    Ok(())
  } else {
    Err(StoreError::Conflict {
      entity: EntityKind::Trigger,
    })
  }
}

fn record_replay_evidence(state: &mut MemoryState, identity: String) -> Result<bool, StoreError> {
  let evidence_count = [
    state.idempotency_outcomes.contains(&identity),
    state.audit_facts.contains(&identity),
    state.outbox_entries.contains(&identity),
  ]
  .into_iter()
  .filter(|recorded| *recorded)
  .count();
  match evidence_count {
    0 => {
      record_evidence(state, identity);
      Ok(true)
    }
    3 => Ok(false),
    _ => Err(StoreError::Unavailable),
  }
}
