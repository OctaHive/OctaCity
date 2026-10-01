use std::{collections::VecDeque, time::Duration};

use super::*;
use octacity_server_application::{
  ManagementActor, ManagementAuthorizationGrant, ManagementClientKind, ManagementCommandUseCase, ManagementIngress,
  ManagementRequestAttributes, ManagementRequestContext, ManagementRequestId, ManagementSecurityScope,
  ManagementVisibility,
};

#[test]
fn durable_manual_trigger_work_is_isolated_by_management_security_scope() {
  run(async {
    let fixture = fixture();
    let work = Arc::new(ReservationProbe::default());
    let evaluator = Arc::new(ManualTriggerService::new(
      Arc::new(RecordingStore::default()),
      Arc::new(StaticContext(fixture.context)),
      Arc::new(RecordingResolver::succeed("unused")),
    ));
    let service = DurableManualTriggerService::new(
      evaluator,
      work.clone(),
      DurableRetryPolicy::new(3, 10, 100).unwrap(),
      Duration::from_millis(100),
    );
    let command = AcceptManualTriggerCommand {
      trigger: fixture.command,
      accepted_at: time(200),
    };

    assert!(
      management_command_in_scope(&service, command.clone(), "operator:first", 1)
        .await
        .is_err()
    );
    assert!(
      management_command_in_scope(&service, command, "operator:second", 2)
        .await
        .is_err()
    );
    let occurrences = work.occurrences.lock().unwrap();
    assert_eq!(occurrences.len(), 2);
    assert_ne!(occurrences[0], occurrences[1]);
  });
}

async fn management_command_in_scope(
  service: &DurableManualTriggerService,
  command: AcceptManualTriggerCommand,
  scope: &str,
  request_id: u128,
) -> Result<ManualTriggerOutcome, ManualTriggerError> {
  let context = ManagementRequestContext::new(
    ManagementActor::authenticated(scope).unwrap(),
    ManagementSecurityScope::new(scope).unwrap(),
    ManagementRequestId::new(uuid::Uuid::from_u128(request_id)).unwrap(),
    ManagementRequestAttributes::new(
      ManagementIngress::VerifiedIdentity,
      Some(ManagementClientKind::Automation),
    ),
  )
  .unwrap();
  service
    .execute_management_command(
      &context,
      &ManagementAuthorizationGrant::new(ManagementVisibility::all()),
      command,
    )
    .await
}

#[derive(Default)]
struct ReservationProbe {
  occurrences: Mutex<Vec<TriggerOccurrenceId>>,
}

#[async_trait]
impl TriggerEvaluationWorkStore for ReservationProbe {
  async fn reserve_trigger_evaluation(
    &self,
    request: ManagementMutation<ReserveTriggerEvaluation>,
  ) -> Result<TriggerEvaluationReservation, StoreError> {
    self.occurrences.lock().unwrap().push(request.mutation().occurrence_id);
    Ok(TriggerEvaluationReservation::Pending)
  }

  async fn claim_trigger_evaluations(
    &self,
    _request: ClaimTriggerEvaluations,
  ) -> Result<Vec<TriggerEvaluationClaim>, StoreError> {
    Ok(Vec::new())
  }

  async fn record_trigger_evaluation_revision(
    &self,
    _request: octacity_server_store::RecordTriggerEvaluationRevision,
  ) -> Result<(), StoreError> {
    Err(StoreError::Unavailable)
  }

  async fn complete_trigger_evaluation(&self, _request: CompleteTriggerEvaluation) -> Result<(), StoreError> {
    Err(StoreError::Unavailable)
  }

  async fn fail_trigger_evaluation(&self, _request: FailTriggerEvaluation) -> Result<(), StoreError> {
    Err(StoreError::Unavailable)
  }
}

#[test]
fn transient_vcs_failure_is_durable_and_recovered_without_duplicate_evaluation() {
  run(async {
    let fixture = fixture();
    let builds = Arc::new(RecordingStore::default());
    let work = Arc::new(RecordingEvaluationWork::default());
    let resolver = Arc::new(SequencedResolver::new([
      Err(RevisionResolutionError::Unavailable),
      Ok(ImmutableRevision::new("0123456789abcdef").unwrap()),
    ]));
    let evaluator = Arc::new(ManualTriggerService::new(
      builds.clone(),
      Arc::new(StaticContext(fixture.context)),
      resolver.clone(),
    ));
    let policy = DurableRetryPolicy::new(3, 10, 100).unwrap();
    let service = DurableManualTriggerService::new(evaluator.clone(), work.clone(), policy, Duration::from_millis(100));
    let command = AcceptManualTriggerCommand {
      trigger: fixture.command,
      accepted_at: time(200),
    };

    assert!(matches!(
      management_command(&service, command.clone()).await,
      Err(ManualTriggerError::Revision(RevisionResolutionError::Unavailable))
    ));
    assert_eq!(
      work.snapshot(),
      WorkSnapshot::Pending {
        attempts: 1,
        due_at: time(210)
      }
    );

    let mut replay = command;
    replay.accepted_at = time(205);
    assert!(matches!(
      management_command(&service, replay).await,
      Err(error) if error.classification() == ApplicationFailure::Unavailable
    ));
    assert_eq!(resolver.calls(), 1);
    assert_eq!(work.payload()["schema_version"], 2);
    assert_eq!(work.payload()["security_scope"], "trusted-network");

    let worker = ManualTriggerRetryWorker::new(evaluator, work.clone(), policy);
    let outcome = worker
      .run_once(owner("retry-worker"), time(210), time(400), 4)
      .await
      .unwrap();
    assert_eq!(outcome.claimed, 1);
    assert_eq!(outcome.completed, 1);
    assert_eq!(resolver.calls(), 2);
    assert_eq!(builds.requests.lock().unwrap().len(), 1);
    let reservation_audits = work.management_audits.lock().unwrap();
    let trigger_audits = builds.management_audits.lock().unwrap();
    assert_eq!(reservation_audits.len(), 1);
    assert_eq!(trigger_audits.len(), 1);
    for audit in [&reservation_audits[0], &trigger_audits[0]] {
      assert_eq!(
        audit.actor().kind,
        octacity_server_store::AuditActorKind::UnauthenticatedManagement
      );
      assert_eq!(audit.actor().identity, None);
      assert_eq!(audit.security_scope().as_str(), "trusted-network");
      assert_eq!(audit.request_identity(), "74eb362d-4264-4d3a-88b8-556974a3b017");
    }
    drop(trigger_audits);
    drop(reservation_audits);
    assert_eq!(work.snapshot(), WorkSnapshot::Completed { attempts: 2 });
  });
}

#[test]
fn pending_legacy_payloads_are_retried_with_stable_trusted_network_attribution() {
  run(async {
    verify_legacy_payload(LegacyPayload::AuditedV1).await;
    verify_legacy_payload(LegacyPayload::Unaudited).await;
  });
}

#[derive(Clone, Copy)]
enum LegacyPayload {
  AuditedV1,
  Unaudited,
}

async fn verify_legacy_payload(format: LegacyPayload) {
  let fixture = fixture();
  let builds = Arc::new(RecordingStore::default());
  let work = Arc::new(RecordingEvaluationWork::default());
  let resolver = Arc::new(SequencedResolver::new([
    Err(RevisionResolutionError::Unavailable),
    Ok(ImmutableRevision::new("0123456789abcdef").unwrap()),
  ]));
  let evaluator = Arc::new(ManualTriggerService::new(
    builds.clone(),
    Arc::new(StaticContext(fixture.context)),
    resolver,
  ));
  let policy = DurableRetryPolicy::new(3, 10, 100).unwrap();
  let service = DurableManualTriggerService::new(evaluator.clone(), work.clone(), policy, Duration::from_millis(100));
  let command = AcceptManualTriggerCommand {
    trigger: fixture.command,
    accepted_at: time(200),
  };

  assert!(management_command(&service, command.clone()).await.is_err());
  match format {
    LegacyPayload::AuditedV1 => {
      let mut payload = work.payload();
      let object = payload.as_object_mut().unwrap();
      object.insert("schema_version".to_owned(), json!(1));
      object.remove("security_scope");
      work.replace_payload(payload);
    }
    LegacyPayload::Unaudited => work.replace_payload(serde_json::to_value(&command).unwrap()),
  }

  let outcome = ManualTriggerRetryWorker::new(evaluator, work.clone(), policy)
    .run_once(owner("retry-worker"), time(210), time(400), 4)
    .await
    .unwrap();

  assert_eq!(outcome.completed, 1);
  assert_eq!(work.snapshot(), WorkSnapshot::Completed { attempts: 2 });
  let requests = builds.requests.lock().unwrap();
  let occurrence_id = requests[0].trigger.id;
  drop(requests);
  let audits = builds.management_audits.lock().unwrap();
  assert_eq!(audits.len(), 1);
  assert_eq!(
    audits[0].actor().kind,
    octacity_server_store::AuditActorKind::UnauthenticatedManagement
  );
  assert_eq!(audits[0].actor().identity, None);
  assert_eq!(audits[0].security_scope().as_str(), "trusted-network");
  if matches!(format, LegacyPayload::Unaudited) {
    assert_eq!(
      audits[0].request_identity(),
      format!("legacy-trigger-evaluation:{occurrence_id}")
    );
  } else {
    assert_eq!(audits[0].request_identity(), "74eb362d-4264-4d3a-88b8-556974a3b017");
  }
}

#[test]
fn exhausted_vcs_retries_become_a_dead_letter() {
  run(async {
    let fixture = fixture();
    let work = Arc::new(RecordingEvaluationWork::default());
    let resolver = Arc::new(SequencedResolver::new([
      Err(RevisionResolutionError::Unavailable),
      Err(RevisionResolutionError::Unavailable),
    ]));
    let evaluator = Arc::new(ManualTriggerService::new(
      Arc::new(RecordingStore::default()),
      Arc::new(StaticContext(fixture.context)),
      resolver.clone(),
    ));
    let policy = DurableRetryPolicy::new(2, 10, 100).unwrap();
    let service = DurableManualTriggerService::new(evaluator.clone(), work.clone(), policy, Duration::from_millis(100));

    assert!(
      management_command(
        &service,
        AcceptManualTriggerCommand {
          trigger: fixture.command,
          accepted_at: time(200),
        }
      )
      .await
      .is_err()
    );
    let worker = ManualTriggerRetryWorker::new(evaluator, work.clone(), policy);
    let outcome = worker
      .run_once(owner("retry-worker"), time(210), time(400), 4)
      .await
      .unwrap();
    assert_eq!(outcome.dead_letters, 1);
    assert_eq!(work.snapshot(), WorkSnapshot::DeadLetter { attempts: 2 });
    assert_eq!(
      worker
        .run_once(owner("other-worker"), time(500), time(600), 4)
        .await
        .unwrap()
        .claimed,
      0
    );
    assert_eq!(resolver.calls(), 2);
  });
}

#[test]
fn checkpointed_revision_is_reused_after_the_build_transaction_fails() {
  run(async {
    let fixture = fixture();
    let builds = Arc::new(FailOnceStore::default());
    let work = Arc::new(RecordingEvaluationWork::default());
    let resolver = Arc::new(SequencedResolver::new([Ok(
      ImmutableRevision::new("0123456789abcdef").unwrap(),
    )]));
    let evaluator = Arc::new(ManualTriggerService::new(
      builds.clone(),
      Arc::new(StaticContext(fixture.context)),
      resolver.clone(),
    ));
    let policy = DurableRetryPolicy::new(3, 10, 100).unwrap();
    let service = DurableManualTriggerService::new(evaluator.clone(), work.clone(), policy, Duration::from_millis(100));

    assert!(matches!(
      management_command(
        &service,
        AcceptManualTriggerCommand {
          trigger: fixture.command,
          accepted_at: time(200),
        }
      )
      .await,
      Err(ManualTriggerError::Store(StoreError::Unavailable))
    ));
    assert_eq!(work.resolved_revision().unwrap().as_str(), "0123456789abcdef");

    let outcome = ManualTriggerRetryWorker::new(evaluator, work, policy)
      .run_once(owner("retry-worker"), time(210), time(400), 4)
      .await
      .unwrap();
    assert_eq!(outcome.completed, 1);
    assert_eq!(resolver.calls(), 1);
    assert_eq!(builds.inner.requests.lock().unwrap().len(), 1);
  });
}

fn owner(value: &str) -> WorkerOwner {
  WorkerOwner::new(value.to_owned()).unwrap()
}

struct SequencedResolver {
  outcomes: Mutex<VecDeque<Result<ImmutableRevision, RevisionResolutionError>>>,
  calls: AtomicUsize,
}

#[derive(Default)]
struct FailOnceStore {
  inner: RecordingStore,
  accept_calls: AtomicUsize,
}

#[async_trait]
impl octacity_server_store::TriggerAcceptanceStore for FailOnceStore {
  async fn replay_trigger_acceptance(
    &self,
    request: TriggerAcceptanceProbe,
  ) -> Result<Option<TriggerEvaluationOutcome>, StoreError> {
    self.inner.replay_trigger_acceptance(request).await
  }

  async fn accept_trigger(&self, request: AcceptTrigger) -> Result<AcceptTriggerOutcome, StoreError> {
    if self.accept_calls.fetch_add(1, Ordering::SeqCst) == 0 {
      Err(StoreError::Unavailable)
    } else {
      self.inner.accept_trigger(request).await
    }
  }

  async fn accept_management_trigger(
    &self,
    request: ManagementMutation<AcceptTrigger>,
  ) -> Result<AcceptTriggerOutcome, StoreError> {
    if self.accept_calls.fetch_add(1, Ordering::SeqCst) == 0 {
      Err(StoreError::Unavailable)
    } else {
      self.inner.accept_management_trigger(request).await
    }
  }

  async fn suppress_trigger(&self, request: SuppressTrigger) -> Result<SuppressTriggerOutcome, StoreError> {
    self.inner.suppress_trigger(request).await
  }

  async fn suppress_management_trigger(
    &self,
    request: ManagementMutation<SuppressTrigger>,
  ) -> Result<SuppressTriggerOutcome, StoreError> {
    self.inner.suppress_management_trigger(request).await
  }
}

impl SequencedResolver {
  fn new(outcomes: impl IntoIterator<Item = Result<ImmutableRevision, RevisionResolutionError>>) -> Self {
    Self {
      outcomes: Mutex::new(outcomes.into_iter().collect()),
      calls: AtomicUsize::new(0),
    }
  }

  fn calls(&self) -> usize {
    self.calls.load(Ordering::SeqCst)
  }
}

#[async_trait]
impl RevisionResolver for SequencedResolver {
  async fn resolve(&self, _request: RevisionResolutionRequest) -> Result<ImmutableRevision, RevisionResolutionError> {
    self.calls.fetch_add(1, Ordering::SeqCst);
    self.outcomes.lock().unwrap().pop_front().unwrap()
  }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum WorkSnapshot {
  Empty,
  Pending { attempts: u16, due_at: Timestamp },
  Completed { attempts: u16 },
  DeadLetter { attempts: u16 },
}

#[derive(Default)]
struct RecordingEvaluationWork {
  entry: Mutex<Option<WorkEntry>>,
  management_audits: Mutex<Vec<MutationAuditContext>>,
}

struct WorkEntry {
  occurrence_id: TriggerOccurrenceId,
  digest: [u8; 32],
  payload: serde_json::Value,
  resolved_revision: Option<ImmutableRevision>,
  attempts: u16,
  state: WorkState,
  due_at: Timestamp,
  claim: Option<(WorkerOwner, Timestamp)>,
}

#[derive(Clone, Copy)]
enum WorkState {
  Pending,
  Completed,
  DeadLetter,
}

impl RecordingEvaluationWork {
  fn snapshot(&self) -> WorkSnapshot {
    let entry = self.entry.lock().unwrap();
    let Some(entry) = entry.as_ref() else {
      return WorkSnapshot::Empty;
    };
    match entry.state {
      WorkState::Pending => WorkSnapshot::Pending {
        attempts: entry.attempts,
        due_at: entry.due_at,
      },
      WorkState::Completed => WorkSnapshot::Completed {
        attempts: entry.attempts,
      },
      WorkState::DeadLetter => WorkSnapshot::DeadLetter {
        attempts: entry.attempts,
      },
    }
  }

  fn resolved_revision(&self) -> Option<ImmutableRevision> {
    self
      .entry
      .lock()
      .unwrap()
      .as_ref()
      .and_then(|entry| entry.resolved_revision.clone())
  }

  fn payload(&self) -> serde_json::Value {
    self.entry.lock().unwrap().as_ref().unwrap().payload.clone()
  }

  fn replace_payload(&self, payload: serde_json::Value) {
    self.entry.lock().unwrap().as_mut().unwrap().payload = payload;
  }
}

#[async_trait]
impl TriggerEvaluationWorkStore for RecordingEvaluationWork {
  async fn reserve_trigger_evaluation(
    &self,
    request: ManagementMutation<ReserveTriggerEvaluation>,
  ) -> Result<TriggerEvaluationReservation, StoreError> {
    let (request, audit) = request.into_parts();
    request.validate()?;
    let mut slot = self.entry.lock().unwrap();
    if slot.is_none() {
      self.management_audits.lock().unwrap().push(audit);
      *slot = Some(WorkEntry {
        occurrence_id: request.occurrence_id,
        digest: request.intent_digest,
        payload: request.payload.clone(),
        resolved_revision: None,
        attempts: 1,
        state: WorkState::Pending,
        due_at: request.requested_at,
        claim: Some((request.owner.clone(), request.claim_expires_at)),
      });
      return Ok(TriggerEvaluationReservation::Claimed(TriggerEvaluationClaim {
        occurrence_id: request.occurrence_id,
        payload: request.payload,
        resolved_revision: None,
        attempt: 1,
        owner: request.owner,
        claim_expires_at: request.claim_expires_at,
      }));
    }
    let entry = slot.as_mut().unwrap();
    if entry.occurrence_id != request.occurrence_id || entry.digest != request.intent_digest {
      return Err(conflict());
    }
    Ok(match entry.state {
      WorkState::Completed => TriggerEvaluationReservation::Completed,
      WorkState::DeadLetter => TriggerEvaluationReservation::DeadLetter,
      WorkState::Pending
        if entry.due_at <= request.requested_at
          && entry
            .claim
            .as_ref()
            .is_none_or(|(_, expires_at)| *expires_at <= request.requested_at) =>
      {
        entry.attempts += 1;
        entry.claim = Some((request.owner.clone(), request.claim_expires_at));
        TriggerEvaluationReservation::Claimed(TriggerEvaluationClaim {
          occurrence_id: entry.occurrence_id,
          payload: entry.payload.clone(),
          resolved_revision: entry.resolved_revision.clone(),
          attempt: entry.attempts,
          owner: request.owner,
          claim_expires_at: request.claim_expires_at,
        })
      }
      WorkState::Pending => TriggerEvaluationReservation::Pending,
    })
  }

  async fn claim_trigger_evaluations(
    &self,
    request: ClaimTriggerEvaluations,
  ) -> Result<Vec<TriggerEvaluationClaim>, StoreError> {
    let mut slot = self.entry.lock().unwrap();
    let Some(entry) = slot.as_mut() else {
      return Ok(Vec::new());
    };
    if !matches!(entry.state, WorkState::Pending)
      || entry.due_at > request.observed_at
      || entry
        .claim
        .as_ref()
        .is_some_and(|(_, expires_at)| *expires_at > request.observed_at)
    {
      return Ok(Vec::new());
    }
    entry.attempts += 1;
    entry.claim = Some((request.owner.clone(), request.claim_expires_at));
    Ok(vec![TriggerEvaluationClaim {
      occurrence_id: entry.occurrence_id,
      payload: entry.payload.clone(),
      resolved_revision: entry.resolved_revision.clone(),
      attempt: entry.attempts,
      owner: request.owner,
      claim_expires_at: request.claim_expires_at,
    }])
  }

  async fn record_trigger_evaluation_revision(
    &self,
    request: octacity_server_store::RecordTriggerEvaluationRevision,
  ) -> Result<(), StoreError> {
    let mut slot = self.entry.lock().unwrap();
    let entry = slot.as_mut().ok_or_else(conflict)?;
    require_owner(entry, &request.owner, request.recorded_at)?;
    if entry
      .resolved_revision
      .as_ref()
      .is_some_and(|revision| revision != &request.resolved_revision)
    {
      return Err(conflict());
    }
    entry.resolved_revision = Some(request.resolved_revision);
    Ok(())
  }

  async fn complete_trigger_evaluation(&self, request: CompleteTriggerEvaluation) -> Result<(), StoreError> {
    let mut slot = self.entry.lock().unwrap();
    let entry = slot.as_mut().ok_or_else(conflict)?;
    require_owner(entry, &request.owner, request.completed_at)?;
    entry.state = WorkState::Completed;
    entry.claim = None;
    Ok(())
  }

  async fn fail_trigger_evaluation(&self, request: FailTriggerEvaluation) -> Result<(), StoreError> {
    request.validate()?;
    let mut slot = self.entry.lock().unwrap();
    let entry = slot.as_mut().ok_or_else(conflict)?;
    require_owner(entry, &request.owner, request.failed_at)?;
    entry.claim = None;
    match request.retry_at {
      Some(retry_at) => entry.due_at = retry_at,
      None => entry.state = WorkState::DeadLetter,
    }
    Ok(())
  }
}

fn require_owner(entry: &WorkEntry, owner: &WorkerOwner, observed_at: Timestamp) -> Result<(), StoreError> {
  if entry
    .claim
    .as_ref()
    .is_some_and(|(actual, expires_at)| actual == owner && *expires_at > observed_at)
  {
    Ok(())
  } else {
    Err(conflict())
  }
}

fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Trigger,
  }
}
