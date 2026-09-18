//! Reusable behavioral contract for authoritative store adapters.

use std::sync::Arc;

use octacity_server_domain::{AgentId, EntityKind, PoolId, TriggerIdentity};
use serde_json::json;

use crate::test_support::{id, run_ready, time};
use crate::testing::{
  InMemoryStore, MutationEvidenceCounts, MutationEvidenceProbe, authoritative_store_contract_fixture, trigger_request,
};
use crate::{
  AcceptTrigger, AppendJobEvents, AuthoritativeStore, DurableJobEvent, EventSequence, JobClaim, JobClaimOutcome,
  JobCompletion, JobCompletionKind, JobEventKind, LeaseAccess, LeaseFence, MutationDisposition, RegistrationEpoch,
  StoreError, StoreInputError, StoreOperation, TriggerCausality, TriggerCause,
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

  let accepted = store.accept_trigger(request.clone()).await.unwrap();
  assert_eq!(accepted.disposition, MutationDisposition::Applied);
  assert_eq!(accepted.ready_jobs, [root_job]);
  let mut trigger_replay = request.clone();
  trigger_replay.accepted_at = time(501);
  let replayed = store.accept_trigger(trigger_replay).await.unwrap();
  assert_eq!(replayed.disposition, MutationDisposition::Replayed);
  assert_eq!(replayed.build_id, accepted.build_id);

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
  let replayed_completion = JobCompletion {
    completed_at: time(3_001),
    ..completion
  };
  assert_eq!(
    store.complete_job(replayed_completion).await.unwrap().disposition,
    MutationDisposition::Replayed
  );

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

  let conflicting = trigger_request(10, 300, allowed_pool);
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
    conflicting.accepted_at,
  )
  .unwrap();
  assert_eq!(
    store.accept_trigger(independent).await.unwrap().disposition,
    MutationDisposition::Applied,
    "a rejected conflicting transaction must leave no partial Build graph"
  );
  assert_eq!(
    evidence.mutation_evidence_counts().await,
    MutationEvidenceCounts {
      idempotency: 8,
      audit: 8,
      outbox: 8,
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
