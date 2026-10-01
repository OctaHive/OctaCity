use super::*;
use crate::TriggerReplayNamespace;

pub(super) async fn replay(
  store: &InMemoryStore,
  request: TriggerAcceptanceProbe,
) -> Result<Option<TriggerEvaluationOutcome>, StoreError> {
  let state = store.lock()?;
  let existing_occurrence = existing_occurrence(&state, &request.trigger, &request.namespace);
  let Some(existing_occurrence) = existing_occurrence else {
    return Ok(None);
  };
  if let Some(existing) = state
    .accepted
    .get(&existing_occurrence)
    .filter(|existing| existing.namespace == request.namespace)
  {
    require_matching_intent(existing.request.intent_digest, request.intent_digest)?;
    let mut outcome = existing.outcome.clone();
    outcome.disposition = MutationDisposition::Replayed;
    return Ok(Some(TriggerEvaluationOutcome::Accepted(outcome)));
  }
  let existing = state
    .suppressed
    .get(&existing_occurrence)
    .filter(|existing| existing.namespace == request.namespace)
    .ok_or(StoreError::Unavailable)?;
  require_matching_intent(existing.request.intent_digest, request.intent_digest)?;
  let mut outcome = existing.outcome;
  outcome.disposition = MutationDisposition::Replayed;
  Ok(Some(TriggerEvaluationOutcome::Suppressed(outcome)))
}

pub(super) async fn accept(
  store: &InMemoryStore,
  request: AcceptTrigger,
  audit: Option<MutationAuditContext>,
) -> Result<AcceptTriggerOutcome, StoreError> {
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
  let namespace = replay_namespace(audit.as_ref());
  let deduplication_key = request.trigger.deduplication_key();
  let evidence_identity = management_trigger_identity("accept-trigger", request.trigger.id, audit.as_ref());
  let existing_occurrence = existing_occurrence(&state, &request.trigger, &namespace);
  if let Some(existing_occurrence) = existing_occurrence {
    if let Some(existing) = state.accepted.get(&existing_occurrence) {
      require_matching_intent(existing.request.intent_digest, request.intent_digest)?;
      let mut outcome = existing.outcome.clone();
      outcome.disposition = MutationDisposition::Replayed;
      if record_replay_evidence(&mut state, evidence_identity)? {
        if let Some(audit) = audit.as_ref() {
          state.management_audit_facts.insert(recorded_management_audit(
            audit,
            StoreOperation::AcceptTrigger,
            EntityKind::Build,
            outcome.build_id,
          ));
        }
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
  if let Some(audit) = audit {
    state.management_audit_facts.insert(recorded_management_audit(
      &audit,
      StoreOperation::AcceptTrigger,
      EntityKind::Build,
      outcome.build_id,
    ));
  }
  state.accepted.insert(
    occurrence_id,
    AcceptedRecord {
      namespace: namespace.clone(),
      request,
      outcome: outcome.clone(),
    },
  );
  state
    .evaluated_by_deduplication
    .insert((namespace, deduplication_key), occurrence_id);
  record_evidence(&mut state, evidence_identity);
  state.commit();
  Ok(outcome)
}

pub(super) async fn suppress(
  store: &InMemoryStore,
  request: SuppressTrigger,
  audit: Option<MutationAuditContext>,
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
  let namespace = replay_namespace(audit.as_ref());
  let deduplication_key = request.trigger.deduplication_key();
  let evidence_identity = management_trigger_identity("suppress-trigger", request.trigger.id, audit.as_ref());
  if let Some(existing_occurrence) = existing_occurrence(&state, &request.trigger, &namespace) {
    if let Some(existing) = state.suppressed.get(&existing_occurrence) {
      require_matching_intent(existing.request.intent_digest, request.intent_digest)?;
      let mut outcome = existing.outcome;
      outcome.disposition = MutationDisposition::Replayed;
      if record_replay_evidence(&mut state, evidence_identity)? {
        if let Some(audit) = audit.as_ref() {
          state.management_audit_facts.insert(recorded_management_audit(
            audit,
            StoreOperation::SuppressTrigger,
            EntityKind::Trigger,
            existing_occurrence,
          ));
        }
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
  if let Some(audit) = audit {
    state.management_audit_facts.insert(recorded_management_audit(
      &audit,
      StoreOperation::SuppressTrigger,
      EntityKind::Trigger,
      occurrence_id,
    ));
  }
  state.suppressed.insert(
    occurrence_id,
    SuppressedRecord {
      namespace: namespace.clone(),
      request,
      outcome,
    },
  );
  state
    .evaluated_by_deduplication
    .insert((namespace, deduplication_key), occurrence_id);
  record_evidence(&mut state, evidence_identity);
  state.commit();
  Ok(outcome)
}

fn existing_occurrence(
  state: &MemoryState,
  trigger: &NormalizedTriggerOccurrence,
  namespace: &TriggerReplayNamespace,
) -> Option<TriggerOccurrenceId> {
  (state
    .accepted
    .get(&trigger.id)
    .is_some_and(|existing| &existing.namespace == namespace)
    || state
      .suppressed
      .get(&trigger.id)
      .is_some_and(|existing| &existing.namespace == namespace))
  .then_some(trigger.id)
  .or_else(|| {
    state
      .evaluated_by_deduplication
      .get(&(namespace.clone(), trigger.deduplication_key()))
      .copied()
  })
}

fn replay_namespace(audit: Option<&MutationAuditContext>) -> TriggerReplayNamespace {
  audit.map_or(TriggerReplayNamespace::NonManagement, |audit| {
    TriggerReplayNamespace::Management(audit.security_scope().clone())
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

fn management_trigger_identity(
  operation: &str,
  occurrence_id: TriggerOccurrenceId,
  audit: Option<&MutationAuditContext>,
) -> String {
  audit.map_or_else(
    || format!("{operation}:{occurrence_id}"),
    |audit| {
      crate::testing::scoped_management_evidence_identity(operation, audit.security_scope(), &occurrence_id.to_string())
    },
  )
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
