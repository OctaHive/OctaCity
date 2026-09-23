use super::*;

#[test]
fn expired_duplicate_schedule_claim_replays_one_build_after_restart() {
  run(async {
    let mut fixture = fixture();
    fixture
      .context
      .configuration
      .definition
      .triggers
      .allowed
      .insert(TriggerKind::Scheduled);
    let schedules = Arc::new(InMemoryStore::new());
    let accepted = Arc::new(RecordingStore::default());
    let trigger_service = Arc::new(ManualTriggerService::new(
      accepted.clone(),
      Arc::new(StaticContext(fixture.context)),
      Arc::new(RecordingResolver::succeed("0123456789abcdef")),
    ));
    create_test_schedule(&schedules, &fixture.command).await;

    let first_owner = WorkerOwner::new("replica:first").unwrap();
    let claim = schedules
      .claim_due_schedules(ClaimDueSchedules::new(first_owner, time(1_000), time(1_100), 1).unwrap())
      .await
      .unwrap()
      .pop()
      .unwrap();
    trigger_service
      .accept_scheduled(
        ManualTriggerCommand {
          trigger: claim.schedule.trigger,
          target: claim.schedule.target,
          deduplication_identity: TriggerIdentity::new("schedule:1000").unwrap(),
          source: ManualSourceSelection::ExactRevision(ImmutableRevision::new("0123456789abcdef").unwrap()),
          parameters: fixture.command.parameters.clone(),
          priority: fixture.command.priority,
          observed_at: time(1_000),
        },
        time(1_000),
      )
      .await
      .unwrap();

    let worker = ScheduleWorker::new(schedules.clone(), trigger_service);
    let outcome = worker
      .run_once(
        WorkerOwner::new("replica:replacement").unwrap(),
        time(1_100),
        time(1_200),
        1,
      )
      .await
      .unwrap();

    assert_eq!(outcome.claimed_schedules, 1);
    assert_eq!(outcome.evaluated_occurrences, 1);
    assert_eq!(accepted.requests.lock().unwrap().len(), 1);
    assert_eq!(
      schedules
        .schedule(fixture.command.trigger.id, fixture.command.trigger.version)
        .await
        .unwrap()
        .next_occurrence_at,
      time(2_000)
    );
  });
}

#[test]
fn concurrent_replicas_cannot_own_the_same_due_schedule() {
  run(async {
    let fixture = fixture();
    let schedules = Arc::new(InMemoryStore::new());
    create_test_schedule(&schedules, &fixture.command).await;
    let barrier = Arc::new(Barrier::new(2));
    let claimed = Arc::new(AtomicUsize::new(0));
    let handles: Vec<_> = ["replica:a", "replica:b"]
      .into_iter()
      .map(|owner| {
        let schedules = schedules.clone();
        let barrier = barrier.clone();
        let claimed = claimed.clone();
        std::thread::spawn(move || {
          barrier.wait();
          run(async {
            let count = schedules
              .claim_due_schedules(
                ClaimDueSchedules::new(WorkerOwner::new(owner).unwrap(), time(1_000), time(2_000), 1).unwrap(),
              )
              .await
              .unwrap()
              .len();
            claimed.fetch_add(count, Ordering::SeqCst);
          });
        })
      })
      .collect();
    for handle in handles {
      handle.join().unwrap();
    }
    assert_eq!(claimed.load(Ordering::SeqCst), 1);
  });
}

#[test]
fn schedule_creation_replays_the_original_server_generated_identity() {
  run(async {
    let fixture = fixture();
    let schedules = InMemoryStore::new();
    let first = schedules
      .create_schedule(test_schedule_request(
        &fixture.command,
        fixture.command.trigger.id,
        time(0),
      ))
      .await
      .unwrap();
    let replay = schedules
      .create_schedule(test_schedule_request(&fixture.command, id::<TriggerId>(99), time(500)))
      .await
      .unwrap();

    assert_eq!(first.disposition, StoreDisposition::Applied);
    assert_eq!(replay.disposition, StoreDisposition::Replayed);
    assert_eq!(replay.trigger_id, first.trigger_id);
  });
}

async fn create_test_schedule(store: &InMemoryStore, command: &ManualTriggerCommand) {
  store
    .create_schedule(test_schedule_request(command, command.trigger.id, time(0)))
    .await
    .unwrap();
}

fn test_schedule_request(
  command: &ManualTriggerCommand,
  trigger_id: TriggerId,
  created_at: Timestamp,
) -> CreateSchedule {
  let schedule = ScheduleDefinition {
    expression: "* * * * * * *".to_owned(),
    timezone: "UTC".to_owned(),
    missed_run_policy: MissedRunPolicy::CatchUp { maximum_occurrences: 1 },
  };
  CreateSchedule {
    trigger: CreateTriggerDefinition {
      id: trigger_id,
      version: command.trigger.version,
      configuration_id: command.target.configuration_id,
      configuration_version: command.target.configuration_version,
      kind: TriggerKind::Scheduled,
      enabled: true,
      definition: serde_json::to_value(ScheduledBuildDefinition {
        source: ManualSourceSelection::ExactRevision(ImmutableRevision::new("0123456789abcdef").unwrap()),
        parameters: command.parameters.clone(),
        priority: command.priority,
      })
      .unwrap(),
      idempotency_key: IdempotencyKey::new("schedule:create").unwrap(),
      created_at,
    },
    next_occurrence_at: schedule.next_after(created_at).unwrap(),
    schedule,
  }
}
