//! Reusable behavioral contract for authoritative store adapters.

use std::sync::Arc;

use octacity_server_domain::{AgentId, EntityKind, PoolId, TriggerIdentity};
use octacity_server_job::JobFailureClass;
use octacity_server_orchestrator::{AttemptState, BuildState};
use serde_json::json;

use crate::test_support::{id, run_ready, time};
use crate::testing::{
  InMemoryStore, MutationEvidenceCounts, MutationEvidenceProbe, authoritative_store_contract_fixture, retry_request,
  trigger_request,
};
use crate::{
  AcceptTrigger, AppendJobEvents, AuthoritativeStore, CancelBuild, DurableJobEvent, EventSequence, IdempotencyKey,
  JobClaim, JobClaimOutcome, JobCompletion, JobCompletionKind, JobEventKind, LeaseAccess, LeaseFence,
  MutationDisposition, RegistrationEpoch, StoreError, StoreInputError, StoreOperation, SuppressTrigger,
  TriggerAcceptanceProbe, TriggerCausality, TriggerCause, TriggerEvaluationOutcome, TriggerIntentDigest,
};

const CONTRACT_LEASE_EXPIRY_MILLIS: i64 = 253_402_300_799_000;

/// Runs the reusable backend-neutral behavioral contract against one empty adapter.
pub async fn verify_authoritative_store_contract<S, P>(store: Arc<S>, evidence: Arc<P>)
where
  S: AuthoritativeStore + 'static,
  P: MutationEvidenceProbe + 'static,
{
  let fixture = authoritative_store_contract_fixture();
  let allowed_pool = fixture.allowed_pool;
  let request = fixture.request;
  let root_job = request.jobs[0].id;
  let child_job = request.jobs[1].id;

  let mut mismatched_target = request.clone();
  mismatched_target.trigger.target.configuration_id = id(997);
  assert_eq!(
    store.accept_trigger(mismatched_target).await.unwrap_err(),
    StoreError::InvalidInput {
      operation: StoreOperation::AcceptTrigger,
      source: StoreInputError::TriggerTargetMismatch,
    },
    "the normalized occurrence must carry the exact Build Configuration target"
  );
  let mut wrong_kind = request.clone();
  wrong_kind.trigger.cause = TriggerCause::Scheduled {};
  wrong_kind.trigger.deduplication_identity = TriggerIdentity::new("schedule:wrong-kind").unwrap();
  assert_eq!(
    store.accept_trigger(wrong_kind).await.unwrap_err(),
    StoreError::NotFound {
      entity: EntityKind::Trigger
    },
    "the normalized origin must match the persisted Trigger definition"
  );

  let mut missing_reference = request.clone();
  missing_reference.build.repository_version = missing_reference.build.repository_version.next().unwrap();
  assert_eq!(
    store.accept_trigger(missing_reference).await.unwrap_err(),
    StoreError::NotFound {
      entity: EntityKind::Trigger
    },
    "an adapter must reject a Trigger whose immutable prerequisites do not exist"
  );
  let mut missing_pool = request.clone();
  let unknown_pool = id::<PoolId>(998);
  for job in &mut missing_pool.jobs {
    job.allowed_pools = vec![unknown_pool];
  }
  assert_eq!(
    store.accept_trigger(missing_pool).await.unwrap_err(),
    StoreError::NotFound {
      entity: EntityKind::Pool
    },
    "an adapter must reject a materialized Job that references an unknown Pool"
  );
  let mut mismatched_job_spec = request.clone();
  mismatched_job_spec.jobs[0].job_spec_template = request.jobs[1].job_spec_template.clone();
  assert_eq!(
    store.accept_trigger(mismatched_job_spec).await.unwrap_err(),
    StoreError::InvalidInput {
      operation: StoreOperation::AcceptTrigger,
      source: StoreInputError::JobSpecTemplateBindingMismatch,
    },
    "materialization must require execution intent derived for the exact Pipeline node"
  );

  let accepted = store.accept_trigger(request.clone()).await.unwrap();
  assert_eq!(accepted.disposition, MutationDisposition::Applied);
  assert_eq!(accepted.ready_jobs, [root_job]);
  let mut trigger_replay = request.clone();
  trigger_replay.accepted_at = time(501);
  trigger_replay.trigger.source_time = time(502);
  let replayed = store.accept_trigger(trigger_replay).await.unwrap();
  assert_eq!(replayed.disposition, MutationDisposition::Replayed);
  assert_eq!(replayed.trigger_occurrence_id, request.trigger.id);
  assert_eq!(replayed.build_id, accepted.build_id);

  let mut equivalent_occurrence = request.trigger.clone();
  equivalent_occurrence.id = id(10_001);
  equivalent_occurrence.causality = TriggerCausality::root(equivalent_occurrence.id);
  equivalent_occurrence.source_time = time(503);
  let replayed = store
    .replay_trigger_acceptance(TriggerAcceptanceProbe {
      trigger: equivalent_occurrence.clone(),
      intent_digest: request.intent_digest,
    })
    .await
    .unwrap()
    .expect("the stable Trigger deduplication key must find the accepted Build");
  let TriggerEvaluationOutcome::Accepted(replayed) = replayed else {
    panic!("an accepted Trigger must replay its Build outcome");
  };
  assert_eq!(replayed.disposition, MutationDisposition::Replayed);
  assert_eq!(replayed.trigger_occurrence_id, request.trigger.id);
  assert_eq!(replayed.build_id, accepted.build_id);
  assert_eq!(replayed.attempt_id, accepted.attempt_id);

  let mut suppressed_occurrence = request.trigger.clone();
  suppressed_occurrence.id = id(10_002);
  suppressed_occurrence.causality = TriggerCausality::root(suppressed_occurrence.id);
  suppressed_occurrence.deduplication_identity = TriggerIdentity::new("manual:suppressed").unwrap();
  let suppressed_intent = TriggerIntentDigest::from_bytes([42; 32]);
  let suppressed = store
    .suppress_trigger(SuppressTrigger::new(suppressed_occurrence.clone(), suppressed_intent, time(504)).unwrap())
    .await
    .unwrap();
  assert_eq!(suppressed.disposition, MutationDisposition::Applied);
  let original_suppressed_occurrence = suppressed.trigger_occurrence_id;
  suppressed_occurrence.id = id(10_003);
  suppressed_occurrence.causality = TriggerCausality::root(suppressed_occurrence.id);
  let replayed = store
    .replay_trigger_acceptance(TriggerAcceptanceProbe {
      trigger: suppressed_occurrence,
      intent_digest: suppressed_intent,
    })
    .await
    .unwrap()
    .expect("suppressed Trigger must be replayable before external resolution");
  let TriggerEvaluationOutcome::Suppressed(replayed) = replayed else {
    panic!("suppressed Trigger must replay a suppressed outcome");
  };
  assert_eq!(replayed.disposition, MutationDisposition::Replayed);
  assert_eq!(replayed.trigger_occurrence_id, original_suppressed_occurrence);

  let mut duplicate_acceptance = trigger_request(10_001, 20_000, allowed_pool);
  duplicate_acceptance.trigger.deduplication_identity = request.trigger.deduplication_identity.clone();
  duplicate_acceptance.intent_digest = request.intent_digest;
  let duplicate_candidate_build = duplicate_acceptance.build.id;
  let replayed = store.accept_trigger(duplicate_acceptance.clone()).await.unwrap();
  assert_eq!(replayed.disposition, MutationDisposition::Replayed);
  assert_eq!(replayed.trigger_occurrence_id, request.trigger.id);
  assert_eq!(replayed.build_id, accepted.build_id);
  assert_ne!(replayed.build_id, duplicate_candidate_build);
  assert_eq!(replayed.attempt_id, accepted.attempt_id);
  let replayed_again = store.accept_trigger(duplicate_acceptance).await.unwrap();
  assert_eq!(replayed_again.trigger_occurrence_id, request.trigger.id);

  assert_eq!(
    store
      .replay_trigger_acceptance(TriggerAcceptanceProbe {
        trigger: equivalent_occurrence,
        intent_digest: TriggerIntentDigest::from_bytes([99; 32]),
      })
      .await
      .unwrap_err(),
    StoreError::Conflict {
      entity: EntityKind::Trigger
    },
    "deduplication-key reuse with different stable intent must conflict before external resolution"
  );

  assert_eq!(
    store
      .claim_ready_job(claim(
        199,
        id::<AgentId>(999),
        fixture.registration_epoch,
        allowed_pool,
        1_000,
        CONTRACT_LEASE_EXPIRY_MILLIS,
      ))
      .await
      .unwrap(),
    JobClaimOutcome::Empty,
    "an unknown Agent registration must never claim queued work"
  );

  let incompatible = claim(
    200,
    fixture.other_agent_id,
    fixture.registration_epoch,
    fixture.other_pool,
    1_000,
    CONTRACT_LEASE_EXPIRY_MILLIS,
  );
  assert_eq!(
    store.claim_ready_job(incompatible).await.unwrap(),
    JobClaimOutcome::Empty
  );

  for (lease, agent_id, pool_id, reason) in [
    (204, fixture.disabled_agent_id, fixture.disabled_pool, "disabled Pool"),
    (205, fixture.draining_agent_id, fixture.draining_pool, "draining Pool"),
    (
      206,
      fixture.expired_agent_id,
      fixture.allowed_pool,
      "expired registration",
    ),
    (
      207,
      fixture.revoked_agent_id,
      fixture.allowed_pool,
      "revoked registration",
    ),
  ] {
    assert_eq!(
      store
        .claim_ready_job(claim(
          lease,
          agent_id,
          fixture.registration_epoch,
          pool_id,
          1_000,
          CONTRACT_LEASE_EXPIRY_MILLIS,
        ))
        .await
        .unwrap(),
      JobClaimOutcome::Empty,
      "an Agent with a {reason} must never claim queued work"
    );
  }

  let root_claim = claim(
    201,
    fixture.agent_id,
    fixture.registration_epoch,
    allowed_pool,
    1_000,
    CONTRACT_LEASE_EXPIRY_MILLIS,
  );
  let grant = match store.claim_ready_job(root_claim).await.unwrap() {
    JobClaimOutcome::Claimed(grant) => grant,
    JobClaimOutcome::Empty => panic!("root job must be claimable"),
  };
  assert_eq!(grant.job_id, root_job);
  let claim_replay = JobClaim {
    claimed_at: time(1_001),
    ..root_claim
  };
  assert_eq!(
    store.claim_ready_job(claim_replay).await.unwrap(),
    JobClaimOutcome::Claimed(grant),
    "a claim replay must ignore a newly observed server claim time"
  );
  let mismatched_claim = JobClaim {
    expires_at: time(CONTRACT_LEASE_EXPIRY_MILLIS - 1),
    ..root_claim
  };
  assert_eq!(
    store.claim_ready_job(mismatched_claim).await.unwrap_err(),
    StoreError::Conflict {
      entity: EntityKind::Lease
    },
    "one idempotency identity cannot be reused for different content"
  );
  assert_eq!(
    store
      .claim_ready_job(claim(
        202,
        fixture.agent_id,
        fixture.registration_epoch,
        allowed_pool,
        1_000,
        CONTRACT_LEASE_EXPIRY_MILLIS,
      ))
      .await
      .unwrap(),
    JobClaimOutcome::Empty
  );

  let access = LeaseAccess {
    lease_id: grant.lease_id,
    fence: grant.fence,
    agent_id: grant.agent_id,
    registration_epoch: grant.registration_epoch,
  };
  let fenced_access = LeaseAccess {
    fence: LeaseFence::from_bytes([0xff; 32]),
    ..access
  };
  assert_eq!(
    store
      .append_job_events(append(fenced_access, &[(1, 1)]))
      .await
      .unwrap_err(),
    StoreError::Fenced { lease: grant.lease_id }
  );
  let first_batch = append(access, &[(1, 1), (2, 2)]);
  let appended = store.append_job_events(first_batch.clone()).await.unwrap();
  assert_eq!(appended.acknowledged_through.get(), 2);
  assert_eq!(appended.inserted, 2);
  let mut replayed_batch = first_batch;
  replayed_batch.accepted_at = time(2_600);
  assert_eq!(
    store.append_job_events(replayed_batch).await.unwrap(),
    appended,
    "a replay must ignore a newly observed server acceptance time"
  );

  let conflict = store.append_job_events(append(access, &[(2, 9)])).await.unwrap_err();
  assert_eq!(
    conflict,
    StoreError::Conflict {
      entity: EntityKind::Job
    }
  );
  let gap = store.append_job_events(append(access, &[(4, 4)])).await.unwrap_err();
  assert_eq!(
    gap,
    StoreError::EventGap {
      job: root_job,
      expected: 3,
      actual: 4
    }
  );
  assert_eq!(
    store
      .append_job_events(append(access, &[(3, 3)]))
      .await
      .unwrap()
      .inserted,
    1
  );
  let historical = store.append_job_events(append(access, &[(1, 1)])).await.unwrap();
  assert_eq!(historical.acknowledged_through.get(), 3);
  assert_eq!(historical.inserted, 0);

  let premature = JobCompletion {
    lease: access,
    final_sequence: Some(EventSequence::new(4).unwrap()),
    kind: JobCompletionKind::Succeeded,
    completed_at: time(3_000),
  };
  assert_eq!(
    store.complete_job(premature).await.unwrap_err(),
    StoreError::EventsMissing {
      job: root_job,
      durable_through: 3,
      required: 4
    }
  );
  let completion = JobCompletion {
    lease: access,
    final_sequence: Some(EventSequence::new(3).unwrap()),
    kind: JobCompletionKind::Succeeded,
    completed_at: time(3_000),
  };
  let completed = store.complete_job(completion).await.unwrap();
  assert_eq!(completed.disposition, MutationDisposition::Applied);
  assert_eq!(completed.ready_jobs, [child_job]);
  assert!(completed.skipped_jobs.is_empty());
  assert_eq!(completed.attempt_state, AttemptState::Running);
  assert_eq!(completed.build_state, BuildState::Running);
  let replayed_completion = JobCompletion {
    completed_at: time(3_001),
    ..completion
  };
  let mut expected_replay = completed.clone();
  expected_replay.disposition = MutationDisposition::Replayed;
  assert_eq!(store.complete_job(replayed_completion).await.unwrap(), expected_replay);

  let child_grant = match store
    .claim_ready_job(claim(
      203,
      fixture.agent_id,
      fixture.registration_epoch,
      allowed_pool,
      3_001,
      CONTRACT_LEASE_EXPIRY_MILLIS,
    ))
    .await
    .unwrap()
  {
    JobClaimOutcome::Claimed(grant) => grant,
    JobClaimOutcome::Empty => panic!("successful completion must enqueue the child atomically"),
  };
  assert_eq!(child_grant.job_id, child_job);
  let mut expired_append = append(
    LeaseAccess {
      lease_id: child_grant.lease_id,
      fence: child_grant.fence,
      agent_id: child_grant.agent_id,
      registration_epoch: child_grant.registration_epoch,
    },
    &[(1, 1)],
  );
  expired_append.accepted_at = time(CONTRACT_LEASE_EXPIRY_MILLIS);
  assert_eq!(
    store.append_job_events(expired_append).await.unwrap_err(),
    StoreError::Expired {
      lease: child_grant.lease_id
    },
    "an expired Lease cannot append events"
  );

  let cancellation = CancelBuild {
    build_id: request.build.id,
    idempotency_key: IdempotencyKey::new("cancel-main-build").unwrap(),
    requested_at: time(3_500),
  };
  let cancelled = store.cancel_build(cancellation.clone()).await.unwrap();
  assert!(cancelled.cancelled_jobs.is_empty());
  assert_eq!(cancelled.cancelling_jobs, [child_job]);
  assert_eq!(cancelled.attempt_state, AttemptState::Running);
  assert_eq!(cancelled.build_state, BuildState::Running);
  let replayed_cancellation = store
    .cancel_build(CancelBuild {
      requested_at: time(3_501),
      ..cancellation
    })
    .await
    .unwrap();
  let mut expected_cancellation_replay = cancelled;
  expected_cancellation_replay.disposition = MutationDisposition::Replayed;
  assert_eq!(replayed_cancellation, expected_cancellation_replay);

  let child_access = LeaseAccess {
    lease_id: child_grant.lease_id,
    fence: child_grant.fence,
    agent_id: child_grant.agent_id,
    registration_epoch: child_grant.registration_epoch,
  };
  let cancelled_completion = store
    .complete_job(JobCompletion {
      lease: child_access,
      final_sequence: None,
      kind: JobCompletionKind::Failed(JobFailureClass::Execution),
      completed_at: time(3_600),
    })
    .await
    .unwrap();
  assert_eq!(cancelled_completion.attempt_state, AttemptState::Cancelled);
  assert_eq!(cancelled_completion.build_state, BuildState::Cancelled);

  let mut conflicting = trigger_request(10, 300, allowed_pool);
  conflicting.intent_digest = TriggerIntentDigest::from_bytes([99; 32]);
  assert_eq!(
    store.accept_trigger(conflicting.clone()).await.unwrap_err(),
    StoreError::Conflict {
      entity: EntityKind::Trigger
    }
  );
  let duplicate_graph = trigger_request(12, 400, allowed_pool);
  let mut duplicate_identity_trigger = duplicate_graph.trigger.clone();
  duplicate_identity_trigger.deduplication_identity = request.trigger.deduplication_identity.clone();
  let duplicate_identity = AcceptTrigger::new(
    duplicate_identity_trigger,
    duplicate_graph.build,
    duplicate_graph.attempt_id,
    duplicate_graph.attempt_number,
    duplicate_graph.jobs,
    duplicate_graph.intent_digest,
    duplicate_graph.accepted_at,
  )
  .unwrap();
  assert_eq!(
    store.accept_trigger(duplicate_identity).await.unwrap_err(),
    StoreError::Conflict {
      entity: EntityKind::Trigger
    },
    "one stable Trigger deduplication identity cannot create a second Build"
  );
  let mut independent_trigger = conflicting.trigger.clone();
  independent_trigger.id = id(11);
  independent_trigger.deduplication_identity = TriggerIdentity::new("manual:11").unwrap();
  independent_trigger.causality = TriggerCausality::root(independent_trigger.id);
  let independent = AcceptTrigger::new(
    independent_trigger,
    conflicting.build,
    conflicting.attempt_id,
    conflicting.attempt_number,
    conflicting.jobs,
    conflicting.intent_digest,
    conflicting.accepted_at,
  )
  .unwrap();
  let independent_root = independent.jobs[0].id;
  let independent_child = independent.jobs[1].id;
  assert_eq!(
    store.accept_trigger(independent.clone()).await.unwrap().disposition,
    MutationDisposition::Applied,
    "a rejected conflicting transaction must leave no partial Build graph"
  );
  let failed_grant = match store
    .claim_ready_job(claim(
      208,
      fixture.agent_id,
      fixture.registration_epoch,
      allowed_pool,
      4_000,
      CONTRACT_LEASE_EXPIRY_MILLIS,
    ))
    .await
    .unwrap()
  {
    JobClaimOutcome::Claimed(grant) => grant,
    JobClaimOutcome::Empty => panic!("the independent root must be claimable"),
  };
  assert_eq!(failed_grant.job_id, independent_root);
  let failed_access = LeaseAccess {
    lease_id: failed_grant.lease_id,
    fence: failed_grant.fence,
    agent_id: failed_grant.agent_id,
    registration_epoch: failed_grant.registration_epoch,
  };
  let failure = JobCompletion {
    lease: failed_access,
    final_sequence: None,
    kind: JobCompletionKind::Failed(JobFailureClass::Infrastructure),
    completed_at: time(4_100),
  };
  let failed = store.complete_job(failure).await.unwrap();
  assert!(failed.ready_jobs.is_empty());
  assert_eq!(failed.skipped_jobs, [independent_child]);
  assert_eq!(failed.failure_class, Some(JobFailureClass::Infrastructure));
  assert_eq!(failed.attempt_state, AttemptState::Failed);
  assert_eq!(failed.build_state, BuildState::Failed);
  let replayed_failure = store
    .complete_job(JobCompletion {
      completed_at: time(4_101),
      ..failure
    })
    .await
    .unwrap();
  let mut expected_failure_replay = failed;
  expected_failure_replay.disposition = MutationDisposition::Replayed;
  assert_eq!(replayed_failure, expected_failure_replay);

  let retry = retry_request(&independent, 600, "retry-independent-build", time(4_200));
  let retry_root = retry.jobs[0].id;
  let mut immutable_mismatch = retry.clone();
  immutable_mismatch.idempotency_key = IdempotencyKey::new("retry-with-mutated-requirements").unwrap();
  immutable_mismatch.jobs[0].requirements.minimum_cpu_millis += 1;
  assert_eq!(
    store.retry_build(immutable_mismatch).await.unwrap_err(),
    StoreError::Conflict {
      entity: EntityKind::Attempt
    },
    "retry must compare candidate Jobs with the authoritative immutable graph"
  );
  let mut policy_mismatch = retry.clone();
  policy_mismatch.idempotency_key = IdempotencyKey::new("retry-with-mutated-root-policy").unwrap();
  policy_mismatch.jobs[0].dependency_policy = octacity_server_pipeline::DependencyPolicy::AnySucceeded;
  assert_eq!(
    store.retry_build(policy_mismatch).await.unwrap_err(),
    StoreError::Conflict {
      entity: EntityKind::Attempt
    },
    "retry must preserve dependency policy even for a root Job"
  );
  let mut intent_mismatch = retry.clone();
  intent_mismatch.idempotency_key = IdempotencyKey::new("retry-with-mutated-job-intent").unwrap();
  let mut intent = serde_json::to_value(&intent_mismatch.jobs[0].job_spec_template).unwrap();
  intent["repository_locator"] = json!("https://example.test/other-repository.git");
  intent_mismatch.jobs[0].job_spec_template = serde_json::from_value(intent).unwrap();
  assert_eq!(
    store.retry_build(intent_mismatch).await.unwrap_err(),
    StoreError::Conflict {
      entity: EntityKind::Attempt
    },
    "retry must preserve the complete server-derived execution intent"
  );
  let retried = store.retry_build(retry.clone()).await.unwrap();
  assert_eq!(retried.disposition, MutationDisposition::Applied);
  assert_eq!(retried.source_attempt_id, independent.attempt_id);
  assert_eq!(retried.attempt_number.get(), 2);
  assert_eq!(retried.ready_jobs, [retry_root]);
  let mut retry_replay = retry.clone();
  retry_replay.requested_at = time(4_201);
  let mut expected_retry_replay = retried.clone();
  expected_retry_replay.disposition = MutationDisposition::Replayed;
  assert_eq!(store.retry_build(retry_replay).await.unwrap(), expected_retry_replay);

  let mut changed_retry = retry;
  changed_retry.jobs[0].requirements.minimum_cpu_millis += 1;
  assert_eq!(
    store.retry_build(changed_retry).await.unwrap_err(),
    StoreError::Conflict {
      entity: EntityKind::Build
    },
    "one retry idempotency key cannot describe another graph"
  );
  assert_eq!(
    store
      .complete_job(JobCompletion {
        completed_at: time(4_202),
        ..failure
      })
      .await
      .unwrap(),
    expected_failure_replay,
    "retry must preserve the prior Attempt's terminal history"
  );

  let cancellable = trigger_request(13, 500, allowed_pool);
  let cancellable_jobs: Vec<_> = cancellable.jobs.iter().map(|job| job.id).collect();
  store.accept_trigger(cancellable.clone()).await.unwrap();
  let cancelled_without_owner = store
    .cancel_build(CancelBuild {
      build_id: cancellable.build.id,
      idempotency_key: IdempotencyKey::new("cancel-unowned-build").unwrap(),
      requested_at: time(4_300),
    })
    .await
    .unwrap();
  assert_eq!(cancelled_without_owner.cancelled_jobs, cancellable_jobs);
  assert!(cancelled_without_owner.cancelling_jobs.is_empty());
  assert_eq!(cancelled_without_owner.attempt_state, AttemptState::Cancelled);
  assert_eq!(cancelled_without_owner.build_state, BuildState::Cancelled);
  assert_eq!(
    evidence.mutation_evidence_counts().await,
    MutationEvidenceCounts {
      idempotency: 17,
      audit: 17,
      outbox: 17,
    },
    "every accepted mutation must atomically persist one idempotency outcome, audit fact, and outbox entry"
  );
}

/// Runs the complete contract against a new in-memory adapter without an async runtime.
pub fn verify_in_memory_store_contract() {
  let store = Arc::new(InMemoryStore::new());
  store
    .seed_authoritative_contract_prerequisites(&authoritative_store_contract_fixture())
    .unwrap();
  run_ready(
    verify_authoritative_store_contract(store.clone(), store.clone()),
    "the in-memory adapter unexpectedly yielded to an external runtime",
  );
}

fn claim(
  lease: u64,
  agent_id: AgentId,
  registration_epoch: RegistrationEpoch,
  pool: PoolId,
  claimed_at: i64,
  expires_at: i64,
) -> JobClaim {
  JobClaim::new(
    id(lease),
    LeaseFence::from_bytes([lease as u8; 32]),
    agent_id,
    registration_epoch,
    pool,
    time(claimed_at),
    time(expires_at),
  )
  .unwrap()
}

fn append(lease: LeaseAccess, events: &[(u64, u8)]) -> AppendJobEvents {
  AppendJobEvents::new(
    lease,
    events
      .iter()
      .map(|(sequence, marker)| {
        DurableJobEvent::new(
          EventSequence::new(*sequence).unwrap(),
          JobEventKind::new("progress").unwrap(),
          time(2_500),
          json!({"digest_marker": marker}),
        )
        .unwrap()
      })
      .collect(),
    time(2_500),
  )
  .unwrap()
}
