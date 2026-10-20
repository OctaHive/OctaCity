//! Manual Issue delivery through the same verified process, outbox and native ordering boundary.
use super::*;
use crate::runtime::postgres_test_support::TestDatabase;
use octacity_server_store::testing::management_mutation;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn manual_issue_push_recovers_unknown_response_without_a_run_or_retargeting() {
  let (fixture, database, store) = standalone("opaque-tracker").await;
  let project = fixture.input.project_id();
  let (_, profile, command) = manual_issue(&store, &fixture).await;
  let accepted = store
    .transition_issue(management_mutation(command.clone()))
    .await
    .unwrap();
  let make_worker = |store| issue_worker(&fixture, store, vec![profile.clone()]);
  let mode = fixture.root.path().join("opaque-tracker/reporter-mode.json");
  std::fs::write(&mode, r#"{"lose_project_response":true}"#).unwrap();
  fixture.clock.0.store(120, Ordering::SeqCst);
  assert_eq!(
    make_worker(store.clone()).run_once(time(120)).await.unwrap().retrying,
    1
  );
  // A newer reviewed mapping affects future intents, never the already accepted effect.
  replace_with_removal(store.as_ref(), &accepted.issue, accepted.issue.status_id, 150).await;
  std::fs::write(&mode, "{}").unwrap();
  fixture.clock.0.store(220, Ordering::SeqCst);
  let restarted = Arc::new(octacity_server_store_postgres::PostgresStore::new(
    database.pool.clone(),
  ));
  assert_eq!(
    make_worker(restarted.clone())
      .run_once(time(220))
      .await
      .unwrap()
      .delivered,
    1
  );
  assert_eq!(
    restarted
      .transition_issue(management_mutation(command))
      .await
      .unwrap()
      .issue,
    accepted.issue
  );
  assert_eq!(
    make_worker(restarted.clone())
      .run_once(time(221))
      .await
      .unwrap()
      .claimed,
    0
  );
  let calls = std::fs::read_to_string(fixture.root.path().join("opaque-tracker/reporter-requests.jsonl")).unwrap();
  let calls: Vec<serde_json::Value> = calls.lines().map(|s| serde_json::from_str(s).unwrap()).collect();
  assert_eq!(
    calls
      .iter()
      .filter(|c| c["message"]["command"]["operation"] == "apply")
      .count(),
    1
  );
  let original = original_intent(&calls[0]);
  for call in &calls {
    assert_eq!(original_intent(call), original);
  }
  assert_eq!(original["item_id"], "TASK-native");
  assert_eq!(
    original["display_id"], "TASK-7",
    "Reporter routing must use the original verified display locator"
  );
  assert_eq!(original["effects"]["add"], serde_json::json!(["custom-ready"]));
  assert!(
    restarted
      .list_factory_runs(
        ListFactoryRuns::new(
          Some(project),
          Default::default(),
          None,
          8,
          FactoryRunListVisibility::all()
        )
        .unwrap()
      )
      .await
      .unwrap()
      .items
      .is_empty()
  );
  database.cleanup().await;
}

async fn replace_with_removal(
  store: &impl IssueSourceMappingStore,
  issue: &IssueRevision,
  target: IssueStatusId,
  at: i64,
) {
  let previous = store
    .issue_source_mapping(
      issue.project_id,
      issue.id,
      &octacity_server_store::ManagementSecurityScope::trusted_network(),
      None,
    )
    .await
    .unwrap()
    .unwrap();
  let mut definition = previous.definition.clone();
  let effects = &mut definition.status.as_mut().unwrap().push.as_mut().unwrap().statuses[0];
  effects.status = target.to_string();
  effects.add.clear();
  effects.remove = vec!["custom-ready".into()];
  store
    .publish_issue_source_mapping(management_mutation(PublishIssueSourceMapping {
      intent: PublishIssueSourceMappingIntent {
        project_id: issue.project_id,
        issue_id: issue.id,
        expected_issue_version: issue.version,
        expected_mapping: Some(previous.reference()),
        definition,
      },
      native_resource: ExternalWorkIdentity::new("native-container").unwrap(),
      readable: WorkSourceReportingPolicy {
        labels: vec![],
        states: vec![],
      },
      writable: Some(WorkSourceReportingPolicy {
        labels: vec!["custom-ready".into()],
        states: vec![],
      }),
      idempotency_key: IdempotencyKey::new(format!("replace-{at}")).unwrap(),
      published_at: time(at),
    }))
    .await
    .unwrap();
}

async fn standalone(
  provider: &str,
) -> (
  Fixture,
  TestDatabase,
  Arc<octacity_server_store_postgres::PostgresStore>,
) {
  let database = TestDatabase::migrated().await;
  let (fixture, _) = fixture_with_reporting(provider, None, true).await;
  let store = Arc::new(octacity_server_store_postgres::PostgresStore::new(
    database.pool.clone(),
  ));
  store
    .create_project(management_mutation(CreateProject {
      id: fixture.input.project_id(),
      parent_id: None,
      name: ProjectName::new("Manual reporting").unwrap(),
      idempotency_key: IdempotencyKey::new("project").unwrap(),
      created_at: time(0),
    }))
    .await
    .unwrap();
  (fixture, database, store)
}

async fn manual_issue<S>(
  store: &octacity_server_store_postgres::PostgresStore,
  fixture: &Fixture<S>,
) -> (IssueRevision, FactoryReporterProfile, TransitionIssue) {
  let project = fixture.input.project_id();
  let issue = store
    .create_issue(management_mutation(CreateIssue {
      id: IssueId::generate(),
      intent: CreateIssueIntent {
        project_id: project,
        planning: Default::default(),
        content: IssueContent {
          title: "Manual requirements".into(),
          description: "No Factory target".into(),
          acceptance: None,
          priority: None,
        },
      },
      assignees: Default::default(),
      idempotency_key: IdempotencyKey::new("manual").unwrap(),
      created_at: time(1),
    }))
    .await
    .unwrap()
    .issue;
  let source = fixture.input.definition().source.clone();
  let profile = FactoryReporterProfile::new(
    key("issue-writer"),
    key("v1"),
    FactoryReporterProfileDraft {
      issue_delivery: Some(IssueReporterDeliveryPolicy {
        retry: FactoryReporterRetryPolicy {
          max_attempts: 3,
          retry_delay_millis: 100,
        },
        window_millis: 30000,
        budget: BudgetLimit::new(1, 5000, 0, 0, 10000).unwrap(),
      }),
      reporting_policy: Some(WorkSourceReportingPolicy {
        labels: vec!["custom-ready".into()],
        states: vec![],
      }),
      project_id: project,
      installation: source.installation.clone(),
      capability: capability(),
      resource: source.resource.clone(),
      plugin: fixture.profile.read.plugin.clone(),
      provider_configuration: FactoryDigest::from_bytes([0xdd; 32]),
      access: ResourceAccess {
        resource_id: "native-container".into(),
        resource_locator: "private-reporter-container".into(),
        credential: CredentialHandle {
          class: CredentialClass::WorkReporterWrite,
          handle: "private-reporter-grant".into(),
        },
      },
      mapping: ConfigurationReference {
        identity: "issue-effects".into(),
        sha256: "c".repeat(64),
      },
      credential_profile: key("writer"),
      network_hosts: vec!["tracker.invalid".into()],
    },
  )
  .unwrap();
  store
    .attach_issue_source(management_mutation(AttachIssueSource {
      intent: AttachIssueSourceIntent {
        selection: IssueSourceSelection {
          project_id: project,
          source: source.clone(),
          item: WorkSourceItem {
            resource: ExternalWorkIdentity::new("native-container").unwrap(),
            item: ExternalWorkIdentity::new("TASK-native").unwrap(),
          },
        },
        issue_id: issue.id,
        expected_version: issue.version,
        expected_field_policy: Default::default(),
      },
      observation: IssueSourceObservation {
        display_id: Some("TASK-7".into()),
        source: source.clone(),
        plugin: fixture.profile.read.plugin.clone(),
        provider_configuration: FactoryDigest::from_bytes([1; 32]),
        selection_digest: FactoryDigest::from_bytes([2; 32]),
        snapshot_digest: FactoryDigest::from_bytes([3; 32]),
        observed_at: time(2),
        expires_at: time(100),
      },
      idempotency_key: IdempotencyKey::new("attach").unwrap(),
      attached_at: time(2),
    }))
    .await
    .unwrap();
  let edge = store
    .default_issue_workflow(project)
    .await
    .unwrap()
    .workflow
    .definition
    .transitions[0]
    .clone();
  store
    .publish_issue_source_mapping(management_mutation(PublishIssueSourceMapping {
      intent: PublishIssueSourceMappingIntent {
        project_id: project,
        issue_id: issue.id,
        expected_issue_version: issue.version,
        expected_mapping: None,
        definition: IssueSourceMappingDefinition {
          source,
          reporter: Some(profile.binding().clone()),
          push_enabled: true,
          fields: [(IssueField::Status, IssueSourceFieldAuthority::OctacityOwnedPush)].into(),
          status: Some(IssueSourceStatusMapping {
            workflow: IssueWorkflowPin {
              id: issue.workflow_id,
              version: issue.workflow_version,
            },
            pull: vec![],
            push: Some(WorkSourceStatusMapping {
              statuses: vec![WorkSourceStatusEffects {
                status: edge.to.to_string(),
                add: vec!["custom-ready".into()],
                remove: vec![],
                state: None,
              }],
            }),
          }),
        },
      },
      native_resource: ExternalWorkIdentity::new("native-container").unwrap(),
      readable: WorkSourceReportingPolicy {
        labels: vec![],
        states: vec![],
      },
      writable: Some(WorkSourceReportingPolicy {
        labels: vec!["custom-ready".into()],
        states: vec![],
      }),
      idempotency_key: IdempotencyKey::new("mapping").unwrap(),
      published_at: time(3),
    }))
    .await
    .unwrap();
  let command = TransitionIssue::manual(
    TransitionIssueIntent {
      project_id: project,
      id: issue.id,
      expected_version: issue.version,
      workflow_id: issue.workflow_id,
      workflow_version: issue.workflow_version,
      expected_status_id: issue.status_id,
      transition_id: edge.id,
      reason: None,
    },
    IdempotencyKey::new("move").unwrap(),
    time(100),
  );

  (issue, profile, command)
}
fn issue_worker<S>(
  fixture: &Fixture<S>,
  store: Arc<octacity_server_store_postgres::PostgresStore>,
  profiles: Vec<FactoryReporterProfile>,
) -> FactoryWorkStatusOutboxWorker<octacity_server_store_postgres::PostgresStore> {
  let projector = Arc::new(
    HostedFactoryWorkStatusProjector::new(
      store.clone(),
      Arc::new(octacity_server_work_source::WorkAdapterRegistry::discover(fixture.root.path()).unwrap()),
      profiles,
      fixture.clock.clone(),
      Duration::from_secs(2),
      Duration::from_millis(20),
      CancellationToken::new(),
    )
    .unwrap(),
  );
  FactoryWorkStatusOutboxWorker::new(
    store,
    std::collections::BTreeMap::from([(capability(), projector as Arc<dyn FactoryWorkStatusProjector>)]),
    FactoryReporterWorkerSettings::new(key("issue-reporter"), 10000, 4).unwrap(),
    fixture.clock.clone(),
  )
  .unwrap()
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_reporter_upgrade_recovers_only_original_desired_material_once() {
  let (fixture, database, store) = standalone("upgrade-tracker").await;
  let (_, profile, command) = manual_issue(&store, &fixture).await;
  let accepted = store
    .transition_issue(management_mutation(command.clone()))
    .await
    .unwrap();
  // Restore only the previous technical schema; the genuine accepted mutation and desired state stay intact.
  sqlx::query("DELETE FROM factory_outbox_records WHERE run_id IS NULL")
    .execute(&database.pool)
    .await
    .unwrap();
  sqlx::query("DELETE FROM factory_source_projection_intents")
    .execute(&database.pool)
    .await
    .unwrap();
  sqlx::query("DELETE FROM factory_source_projection_journals")
    .execute(&database.pool)
    .await
    .unwrap();
  sqlx::query("ALTER TABLE factory_outbox_records DROP CONSTRAINT factory_outbox_subject, DROP COLUMN issue, ALTER COLUMN run_id SET NOT NULL").execute(&database.pool).await.unwrap();
  sqlx::query("DELETE FROM _sqlx_migrations WHERE version=85")
    .execute(&database.pool)
    .await
    .unwrap();
  restart_before_publication_recovery(&database.pool).await;
  assert_eq!(
    octacity_server_store_postgres::migration_status(&database.pool)
      .await
      .unwrap(),
    octacity_server_store_postgres::MigrationStatus::Pending,
    "readiness must resume frozen publications even when SQL migration history is current"
  );
  octacity_server_store_postgres::migrate(&database.pool).await.unwrap();
  octacity_server_store_postgres::migrate(&database.pool).await.unwrap();
  assert_eq!(
    octacity_server_store_postgres::migration_status(&database.pool)
      .await
      .unwrap(),
    octacity_server_store_postgres::MigrationStatus::Current
  );
  fixture.clock.0.store(120, Ordering::SeqCst);
  assert_eq!(
    issue_worker(&fixture, store.clone(), vec![profile.clone()])
      .run_once(time(120))
      .await
      .unwrap()
      .delivered,
    1
  );
  assert_eq!(
    issue_worker(&fixture, store.clone(), vec![profile])
      .run_once(time(121))
      .await
      .unwrap()
      .claimed,
    0
  );
  assert_eq!(
    store
      .transition_issue(management_mutation(command))
      .await
      .unwrap()
      .issue,
    accepted.issue
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_and_factory_effects_share_native_order_and_preserve_independent_fields() {
  for scope in ["independent", "overlap", "partial"] {
    let (memory, work_profile) = fixture("mixed-tracker").await;
    let (fixture, database) = super::ordering::postgres::postgres_fixture(memory).await;
    let work_projector = hosted(&fixture, vec![work_profile.clone()]);
    let run = queue(&fixture, work_projector).await;
    let before = fixture.store.factory_run_snapshot(run).await.unwrap();
    let (issue, profile, mut command) = manual_issue(&fixture.store, &fixture).await;
    command.moved_at = time(115);
    let accepted = fixture
      .store
      .transition_issue(management_mutation(command))
      .await
      .unwrap();
    let directory = fixture.root.path().join("mixed-tracker");
    let field = if scope == "overlap" {
      "a".repeat(64)
    } else {
      "b".repeat(64)
    };
    std::fs::write(
      directory.join("reporter-mode.json"),
      serde_json::to_vec(&serde_json::json!({"fieldsets":{"issue-effects":[field],
        "original-effects":if scope=="partial" { vec!["a".repeat(64),"b".repeat(64)] } else { vec!["a".repeat(64)] }},
        "split_steps":scope=="partial"}))
      .unwrap(),
    )
    .unwrap();
    std::fs::write(
      directory.join("reporter-state.json"),
      serde_json::to_vec(&serde_json::json!({"c".repeat(64):"human-owned"})).unwrap(),
    )
    .unwrap();
    fixture.clock.0.store(120, Ordering::SeqCst);
    let worker = issue_worker(&fixture, fixture.store.clone(), vec![work_profile, profile]);
    let first = worker.run_once(time(120)).await.unwrap();
    assert_eq!((first.retrying, first.delivered), (1, 1));
    fixture.clock.0.store(220, Ordering::SeqCst);
    let final_pass = worker.run_once(time(220)).await.unwrap();
    assert_eq!(
      (final_pass.delivered, final_pass.failed),
      if scope != "independent" { (0, 1) } else { (1, 0) }
    );
    let state: serde_json::Value =
      serde_json::from_slice(&std::fs::read(directory.join("reporter-state.json")).unwrap()).unwrap();
    assert_eq!(state[&field], "issue-effects");
    assert_eq!(state["c".repeat(64)], "human-owned");
    if scope != "overlap" {
      assert_eq!(state["a".repeat(64)], "original-effects");
    }
    let after = fixture.store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(before.flow, after.flow);
    assert_eq!(before.run, after.run);
    assert_eq!(
      fixture.store.issue(issue.project_id, issue.id, None).await.unwrap(),
      accepted.issue
    );
    database.cleanup().await;
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn factory_only_dispatcher_does_not_consume_issue_projection_events() {
  let (fixture, database, store) = standalone("isolated-tracker").await;
  let (_, profile, command) = manual_issue(&store, &fixture).await;
  store.transition_issue(management_mutation(command)).await.unwrap();
  fixture.clock.0.store(120, Ordering::SeqCst);
  struct WorkOnly;
  #[async_trait::async_trait]
  impl FactoryWorkStatusProjector for WorkOnly {
    async fn set_status(&self, _: FactoryWorkStatusRequest) -> Result<(FactoryDigest, BudgetUsage), ApplicationError> {
      unreachable!("no Work event was published")
    }
    async fn cancel_status(
      &self,
      _: FactoryWorkStatusRequest,
      _: Timestamp,
    ) -> Result<Option<BudgetUsage>, ApplicationError> {
      unreachable!()
    }
  }
  let work_only = FactoryWorkStatusOutboxWorker::new(
    store.clone(),
    std::collections::BTreeMap::from([(capability(), Arc::new(WorkOnly) as Arc<dyn FactoryWorkStatusProjector>)]),
    FactoryReporterWorkerSettings::new(key("work-only"), 1000, 4).unwrap(),
    fixture.clock.clone(),
  )
  .unwrap();
  assert_eq!(work_only.run_once(time(120)).await.unwrap().claimed, 0);
  fixture.clock.0.store(121, Ordering::SeqCst);
  assert_eq!(
    issue_worker(&fixture, store, vec![profile])
      .run_once(time(121))
      .await
      .unwrap()
      .delivered,
    1
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_reporter_material_read_outage_keeps_the_claim_recoverable() {
  let (fixture, database, store) = standalone("outage-tracker").await;
  let (_, profile, command) = manual_issue(&store, &fixture).await;
  let accepted = store
    .transition_issue(management_mutation(command.clone()))
    .await
    .unwrap();
  // Inject unavailable immutable material after publication, while the outbox itself stays operational.
  sqlx::query("ALTER TABLE outbox_entries RENAME COLUMN payload TO interrupted_payload")
    .execute(&database.pool)
    .await
    .unwrap();
  fixture.clock.0.store(120, Ordering::SeqCst);
  let worker = issue_worker(&fixture, store.clone(), vec![profile]);
  let interrupted = worker.run_once(time(120)).await;
  sqlx::query("ALTER TABLE outbox_entries RENAME COLUMN interrupted_payload TO payload")
    .execute(&database.pool)
    .await
    .unwrap();
  assert!(
    interrupted.is_err(),
    "a material read outage must not terminally settle an accepted effect"
  );
  fixture.clock.0.store(10120, Ordering::SeqCst);
  assert_eq!(worker.run_once(time(10120)).await.unwrap().delivered, 1);
  assert_eq!(
    store
      .transition_issue(management_mutation(command))
      .await
      .unwrap()
      .issue,
    accepted.issue
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_reporter_never_guesses_a_missing_legacy_native_locator() {
  let (fixture, database, store) = standalone("missing-locator-tracker").await;
  let (issue, profile, command) = manual_issue(&store, &fixture).await;
  sqlx::query("UPDATE issue_source_links SET document=document-'display_id' WHERE issue_id=$1")
    .bind(issue.id.as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  let accepted = store
    .transition_issue(management_mutation(command.clone()))
    .await
    .unwrap();
  fixture.clock.0.store(120, Ordering::SeqCst);
  assert_eq!(
    issue_worker(&fixture, store.clone(), vec![profile])
      .run_once(time(120))
      .await
      .unwrap()
      .failed,
    1
  );
  assert!(
    !fixture
      .root
      .path()
      .join("missing-locator-tracker/reporter-requests.jsonl")
      .exists()
  );
  assert_eq!(
    store
      .transition_issue(management_mutation(command))
      .await
      .unwrap()
      .issue,
    accepted.issue
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_reporter_restart_cannot_renew_the_original_delivery_window() {
  let (fixture, database, store) = standalone("expiry-tracker").await;
  let (_, profile, command) = manual_issue(&store, &fixture).await;
  let accepted = store
    .transition_issue(management_mutation(command.clone()))
    .await
    .unwrap();
  let directory = fixture.root.path().join("expiry-tracker");
  std::fs::write(
    directory.join("reporter-mode.json"),
    r#"{"failure":{"class":"transient","retry_after_millis":null}}"#,
  )
  .unwrap();
  fixture.clock.0.store(120, Ordering::SeqCst);
  assert_eq!(
    issue_worker(&fixture, store.clone(), vec![profile.clone()])
      .run_once(time(120))
      .await
      .unwrap()
      .retrying,
    1
  );
  std::fs::write(directory.join("reporter-mode.json"), "{}").unwrap();
  fixture.clock.0.store(40000, Ordering::SeqCst);
  let restarted = Arc::new(octacity_server_store_postgres::PostgresStore::new(
    database.pool.clone(),
  ));
  assert_eq!(
    issue_worker(&fixture, restarted, vec![profile])
      .run_once(time(40000))
      .await
      .unwrap()
      .failed,
    1
  );
  assert_eq!(
    std::fs::read_to_string(directory.join("reporter-requests.jsonl"))
      .unwrap()
      .lines()
      .count(),
    1
  );
  assert_eq!(
    store
      .transition_issue(management_mutation(command))
      .await
      .unwrap()
      .issue,
    accepted.issue
  );
  database.cleanup().await;
}

// Positive PostgreSQL lock observation makes this a controlled dependency barrier,
// not an assertion that a task happened to take more than a small wall-clock delay.
async fn backend_is_waiting(pool: &sqlx::PgPool, name: &str) {
  tokio::time::timeout(TEST_WAIT,async {
    loop {
      let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND application_name=$1 AND wait_event_type='Lock')")
        .bind(name).fetch_one(pool).await.unwrap();
      if waiting { break; }
      tokio::time::sleep(Duration::from_millis(5)).await;
    }
  }).await.expect("owned PostgreSQL operation must reach its dependency barrier");
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_reporter_takeover_waits_for_the_original_projection_permit_publication() {
  let (fixture, database, store) = standalone("permit-race-tracker").await;
  let (issue, _, command) = manual_issue(&store, &fixture).await;
  store.transition_issue(management_mutation(command)).await.unwrap();
  let original = store
    .claim_factory_outbox(
      ClaimFactoryOutbox::new(key("original"), time(120), time(220), 1)
        .unwrap()
        .only_kind(key("issue.status.projection")),
    )
    .await
    .unwrap()
    .remove(0)
    .record;
  let mut request = SourceProjectionRequest {
    project_id: issue.project_id,
    work_id: None,
    operation_id: original.operation_id,
    claim: original.claim.unwrap(),
    observed_at: time(120),
    fields: vec![digest(1)],
    action: SourceProjectionAction::Register,
  };
  store.coordinate_source_projection(request.clone()).await.unwrap();
  request.action = SourceProjectionAction::Begin {
    step_id: digest(7),
    fields: request.fields.clone(),
  };
  let options = database.pool.connect_options();
  let coordinate_pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(1)
    .connect_with(options.as_ref().clone().application_name("issue-permit-coordinate"))
    .await
    .unwrap();
  let claim_pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(1)
    .connect_with(options.as_ref().clone().application_name("issue-permit-takeover"))
    .await
    .unwrap();
  let gate = (uuid::Uuid::new_v4().as_u128() & i64::MAX as u128) as i64;
  let mut gate_connection = database.pool.acquire().await.unwrap();
  sqlx::query("SELECT pg_advisory_lock($1)")
    .bind(gate)
    .execute(&mut *gate_connection)
    .await
    .unwrap();
  // Pause the existing journal update after its ownership check, before commit.
  // The numeric barrier identity is generated by this test, never external text.
  sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION pause_issue_permit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_advisory_xact_lock({gate}); RETURN NEW; END $$; CREATE TRIGGER pause_issue_permit BEFORE UPDATE ON factory_source_projection_journals FOR EACH ROW EXECUTE FUNCTION pause_issue_permit()")))
    .execute(&database.pool).await.unwrap();
  let coordinate_store = octacity_server_store_postgres::PostgresStore::new(coordinate_pool.clone());
  let coordinator = tokio::spawn(async move { coordinate_store.coordinate_source_projection(request).await });
  backend_is_waiting(&database.pool, "issue-permit-coordinate").await;
  let claim_store = octacity_server_store_postgres::PostgresStore::new(claim_pool.clone());
  let mut takeover = tokio::spawn(async move {
    claim_store
      .claim_factory_outbox(
        ClaimFactoryOutbox::new(key("replacement"), time(220), time(320), 1)
          .unwrap()
          .only_kind(key("issue.status.projection")),
      )
      .await
  });
  let early = tokio::select! {
    result=&mut takeover => Some(result.unwrap().unwrap()),
    _=backend_is_waiting(&database.pool,"issue-permit-takeover") => None,
  };
  let took_over_before_permit = early.is_some();
  sqlx::query("SELECT pg_advisory_unlock($1)")
    .bind(gate)
    .execute(&mut *gate_connection)
    .await
    .unwrap();
  drop(gate_connection);
  assert!(matches!(
    coordinator.await.unwrap().unwrap(),
    SourceProjectionState::Granted { .. }
  ));
  let replacement = if let Some(result) = early {
    result
  } else {
    takeover.await.unwrap().unwrap()
  };
  assert_eq!(replacement.len(), 1);
  coordinate_pool.close().await;
  claim_pool.close().await;
  database.cleanup().await;
  assert!(
    !took_over_before_permit,
    "claim takeover must not commit before the old permit publication releases ownership"
  );
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_reporter_recovery_cannot_order_an_old_revision_after_a_newer_accepted_effect() {
  let (fixture, database, store) = standalone("old-revision-tracker").await;
  let (issue, _, command) = manual_issue(&store, &fixture).await;
  let first = store
    .transition_issue(management_mutation(command))
    .await
    .unwrap()
    .issue;
  let old = store
    .claim_factory_outbox(
      ClaimFactoryOutbox::new(key("old-reader"), time(120), time(1120), 1)
        .unwrap()
        .only_kind(key("issue.status.projection")),
    )
    .await
    .unwrap()
    .remove(0)
    .record;
  let edge = store
    .default_issue_workflow(issue.project_id)
    .await
    .unwrap()
    .workflow
    .definition
    .transitions
    .into_iter()
    .find(|e| e.from == first.status_id)
    .unwrap();
  replace_with_removal(store.as_ref(), &first, edge.to, 150).await;
  let newest = store
    .transition_issue(management_mutation(TransitionIssue::manual(
      TransitionIssueIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: first.version,
        workflow_id: first.workflow_id,
        workflow_version: first.workflow_version,
        expected_status_id: first.status_id,
        transition_id: edge.id,
        reason: None,
      },
      IdempotencyKey::new("newer-move").unwrap(),
      time(200),
    )))
    .await
    .unwrap()
    .issue;
  // Only the older technical publication is missing; both genuine accepted mutation events remain.
  sqlx::query("DELETE FROM factory_outbox_records WHERE operation_id=$1")
    .bind(old.operation_id.as_bytes().to_vec())
    .execute(&database.pool)
    .await
    .unwrap();
  sqlx::query("DELETE FROM factory_source_projection_intents WHERE operation_id=$1")
    .bind(old.operation_id.as_bytes().to_vec())
    .execute(&database.pool)
    .await
    .unwrap();
  restart_before_publication_recovery(&database.pool).await;
  assert!(
    matches!(
      store
        .claim_factory_outbox(
          ClaimFactoryOutbox::new(key("partial-recovery"), time(210), time(1210), 4)
            .unwrap()
            .only_kind(key("issue.status.projection")),
        )
        .await,
      Err(octacity_server_store::StoreError::Unavailable)
    ),
    "Issue delivery must wait for the complete retained publication phase"
  );
  octacity_server_store_postgres::migrate(&database.pool).await.unwrap();
  let rows = store
    .claim_factory_outbox(
      ClaimFactoryOutbox::new(key("recovery"), time(220), time(1220), 4)
        .unwrap()
        .only_kind(key("issue.status.projection")),
    )
    .await
    .unwrap();
  let older = rows.iter().find(|r| r.record.operation_id == old.operation_id).unwrap();
  let current = rows.iter().find(|r| r.record.operation_id != old.operation_id).unwrap();
  let request = |row: &FactoryOutboxRecord| SourceProjectionRequest {
    project_id: issue.project_id,
    work_id: None,
    operation_id: row.operation_id,
    claim: row.claim.unwrap(),
    observed_at: time(220),
    fields: vec![digest(1)],
    action: SourceProjectionAction::Register,
  };
  assert!(
    store
      .coordinate_source_projection(request(&older.record))
      .await
      .is_err(),
    "legacy recovery cannot supersede an already accepted newer desired state"
  );
  assert!(matches!(
    store
      .coordinate_source_projection(request(&current.record))
      .await
      .unwrap(),
    SourceProjectionState::Ready { .. }
  ));
  assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), newest);
  database.cleanup().await;
}

// Simulate restart after SQL commit, with the one-time data phase still pending.
async fn restart_before_publication_recovery(pool: &sqlx::PgPool) {
  sqlx::query("DROP TABLE issue_reporter_recovery")
    .execute(pool)
    .await
    .unwrap();
  sqlx::query("DELETE FROM _sqlx_migrations WHERE version=86")
    .execute(pool)
    .await
    .unwrap();
  octacity_server_store_postgres::run_migrator(pool, &octacity_server_store_postgres::MIGRATOR)
    .await
    .unwrap();
}
