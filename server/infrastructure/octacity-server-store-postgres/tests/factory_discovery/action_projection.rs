use super::*;
use octacity_server_factory::*;
use octacity_server_store::testing;
use octacity_server_store::*;
#[path = "../../../../application/tests/support/factory_human_fixture.rs"]
mod human;

async fn pending_gate(store: &PostgresStore, admission: &PublishedFactoryAdmission) -> NodeAttemptId {
  let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  let claim = FactoryRunClaimRecord::new(
    snapshot.run.id(),
    key("action.owner"),
    FactoryClaim::new(FactoryClaimFence::new(digest(2)), time(10), time(1000)).unwrap(),
  );
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: snapshot.run.id(),
      expected_version: snapshot.run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        snapshot.run.id(),
        AuditActorKind::Worker,
        None,
        key("factory.claimed"),
        digest(2),
        key("accepted"),
        time(10),
      ),
    })
    .await
    .unwrap();
  let snapshot = store.factory_run_snapshot(snapshot.run.id()).await.unwrap();
  let definition = snapshot
    .admitted_flow
    .closure()
    .definition(snapshot.admitted_flow.closure().root())
    .unwrap();
  let node = &definition.nodes()[0];
  let input = FlowNodeInput::new(
    &snapshot.work,
    &snapshot.admitted_flow,
    snapshot.admitted_flow.root_run(),
    snapshot.admitted_flow.initial_cycle(),
    node.key().clone(),
    FlowPayload::new(
      snapshot
        .admitted_flow
        .data_schema(node.input_schema().unwrap())
        .unwrap(),
      serde_json::json!(true),
    )
    .unwrap(),
  )
  .unwrap();
  let attempt = NodeAttempt::new(
    snapshot.admitted_flow.root_run(),
    snapshot.admitted_flow.initial_cycle(),
    definition,
    NodeAttemptInput {
      id: NodeAttemptId::generate(),
      node_key: node.key().clone(),
      node_kind: node.kind(),
      number: NodeAttemptNumber::INITIAL,
      input_digest: input.digest().unwrap(),
      budget: node.budget(),
      started_at: time(11),
      deadline: time(100),
      execution: NodeExecutionIdentity::BuiltIn,
      ownership: FactoryClaimOwnership::new(claim.owner.clone(), claim.claim),
    },
  )
  .unwrap();
  let id = attempt.id();
  let mut change = testing::factory_flow_contract_transition(&snapshot, &claim, 11);
  change.append.flow.data.inputs.push(input);
  change.append.flow.attempts.push(attempt);
  change.budget = FactoryBudgetRecord::new(
    snapshot.run.id(),
    change.next_run.version(),
    BudgetUsage {
      attempts: 1,
      ..Default::default()
    },
    time(11),
  );
  change.current.budget_id = change.budget.id;
  store.commit_factory_run_transition(change).await.unwrap();
  id
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn action_projection_recovers_publication_and_fences_source_versions_without_blocking_controls() {
  let database = TestDatabase::empty().await;
  octacity_server_store_postgres::MIGRATOR
    .run_to(90, &database.pool)
    .await
    .unwrap();
  let admission = human::human_fixture_with_contract(
    FlowDataSchema::new(key("operator.response"), key("v1"), FlowValueSchema::Boolean).unwrap(),
    None,
    false,
  );
  let (store, _) = seed_projection_admission(&database, &admission).await;
  let old_work = FactoryActionProjectionWork {
    run_id: admission.run.id(),
    source_version: admission.run.version(),
  };
  let attempt = pending_gate(&store, &admission).await;
  let legacy = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  for _ in 0..2 {
    octacity_server_store_postgres::migrate(&database.pool).await.unwrap();
  }
  assert_eq!(
    store.factory_run_snapshot(admission.run.id()).await.unwrap(),
    legacy,
    "upgrade and repeated migration cannot progress legacy executions"
  );
  let query = |at| FactoryActionProjectionQuery::new(admission.run.id(), time(at), None, 1).unwrap();
  assert_eq!(
    store.read_factory_action_projection(query(12)).await.unwrap().coverage,
    FactoryActionProjectionCoverage::Incomplete
  );
  assert!(matches!(
    store.refresh_factory_action_projection(old_work, time(12)).await,
    Err(StoreError::Conflict { .. })
  ));
  let work = store.factory_action_projection_work(1).await.unwrap().items[0];
  // Inject interruption after header insertion but before the action row publishes.
  sqlx::query("CREATE FUNCTION fail_action_projection_insert() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_terminate_backend(pg_backend_pid()); RETURN NEW; END $$").execute(&database.pool).await.unwrap();
  sqlx::query("CREATE TRIGGER interrupt_action_projection BEFORE INSERT ON factory_action_items FOR EACH ROW EXECUTE FUNCTION fail_action_projection_insert()").execute(&database.pool).await.unwrap();
  let before = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  assert!(store.refresh_factory_action_projection(work, time(12)).await.is_err());
  sqlx::query("DROP TRIGGER interrupt_action_projection ON factory_action_items")
    .execute(&database.pool)
    .await
    .unwrap();
  let restarted = PostgresStore::new(database.pool.clone());
  let incomplete = restarted.read_factory_action_projection(query(12)).await.unwrap();
  assert_eq!(incomplete.coverage, FactoryActionProjectionCoverage::Incomplete);
  assert!(incomplete.items.is_empty());
  assert_eq!(
    restarted.factory_run_snapshot(admission.run.id()).await.unwrap(),
    before
  );
  restarted
    .refresh_factory_action_projection(work, time(12))
    .await
    .unwrap();
  // Lost acknowledgement/repeated publication does not duplicate a derived row.
  PostgresStore::new(database.pool.clone())
    .refresh_factory_action_projection(work, time(12))
    .await
    .unwrap();
  let page = restarted.read_factory_action_projection(query(12)).await.unwrap();
  assert_eq!(page.coverage, FactoryActionProjectionCoverage::Complete);
  assert_eq!(page.items.len(), 1);
  assert_eq!(page.items[0].node_attempt_id, attempt);
  assert_eq!(page.items[0].run_id, admission.run.id());
  assert_eq!(page.items[0].work_id, admission.work.id());
  assert_eq!(page.items[0].actionable_at, time(11));
  assert_eq!(page.items[0].deadline, Some(time(100)));
  assert_eq!(
    restarted
      .read_factory_action_projection(query(100))
      .await
      .unwrap()
      .items
      .len(),
    1
  );
  assert!(
    restarted
      .read_factory_action_projection(query(101))
      .await
      .unwrap()
      .items
      .is_empty()
  );
  assert!(matches!(
    restarted.refresh_factory_action_projection(work, time(11)).await,
    Err(StoreError::Conflict { .. })
  ));

  // A stopped/unavailable index must not become a dependency of Run controls.
  let mut unavailable = database.pool.begin().await.unwrap();
  sqlx::query("LOCK TABLE factory_action_projections, factory_action_items IN ACCESS EXCLUSIVE MODE")
    .execute(&mut *unavailable)
    .await
    .unwrap();
  let control_pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(1)
    .connect_with((*database.pool.connect_options()).clone())
    .await
    .unwrap();
  sqlx::query("SET statement_timeout='2s'")
    .execute(&control_pool)
    .await
    .unwrap();
  let control_store = PostgresStore::new(control_pool.clone());
  let request = testing::management_mutation(ApplyFactoryRunControl {
    run_id: admission.run.id(),
    expected_version: before.run.version(),
    idempotency_key: idempotency("record-answer-without-index"),
    intent: FactoryRunControlIntent::ResolveNode {
      node_attempt_id: attempt,
      outcome: key("approved"),
      payload: FlowPayload::new(
        admission
          .flow
          .data_schema(
            admission
              .flow
              .closure()
              .definition(admission.flow.closure().root())
              .unwrap()
              .nodes()[0]
              .outcomes()[0]
              .schema(),
          )
          .unwrap(),
        serde_json::json!(true),
      )
      .unwrap(),
    },
    requested_at: time(20),
  });
  let accepted = control_store.apply_factory_run_control(request.clone()).await.unwrap();
  unavailable.rollback().await.unwrap();
  let invalidated = restarted.read_factory_action_projection(query(21)).await.unwrap();
  assert_eq!(invalidated.run_version, accepted.version);
  assert_eq!(invalidated.coverage, FactoryActionProjectionCoverage::Incomplete);
  assert!(
    invalidated.items.is_empty(),
    "source commit atomically invalidates the older index"
  );
  assert!(matches!(
    restarted.refresh_factory_action_projection(work, time(21)).await,
    Err(StoreError::Conflict { .. })
  ));
  let current = restarted.factory_action_projection_work(1).await.unwrap().items[0];
  restarted
    .refresh_factory_action_projection(current, time(21))
    .await
    .unwrap();
  let recorded = restarted.read_factory_action_projection(query(21)).await.unwrap();
  assert_eq!(recorded.coverage, FactoryActionProjectionCoverage::Complete);
  assert!(
    recorded.items.is_empty(),
    "recorded intent is not unanswered before owner consumption"
  );
  assert!(
    restarted
      .factory_run_snapshot(admission.run.id())
      .await
      .unwrap()
      .flow
      .completions
      .is_empty()
  );
  assert_eq!(
    control_store
      .apply_factory_run_control(request)
      .await
      .unwrap()
      .disposition,
    MutationDisposition::Replayed
  );
  control_pool.close().await;
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn shared_action_queue_filters_visibility_before_counts_cursors_and_coverage() {
  let database = TestDatabase::migrated().await;
  let mut runs = Vec::new();
  for _ in 0..3 {
    let mut admission = human::human_fixture_with_contract(
      FlowDataSchema::new(key("operator.response"), key("v1"), FlowValueSchema::Boolean).unwrap(),
      None,
      false,
    );
    let old = &admission.work;
    admission.work = WorkEnvelope::new(
      old.id(),
      old.configuration().clone(),
      ExternalWorkIdentity::new(old.id().to_string()).unwrap(),
      old.subject().clone(),
      old.artifacts().clone(),
      WorkClassification::new(old.priority(), old.risk(), old.metadata().clone()),
    )
    .unwrap();
    admission.run = FactoryRun::admitted(admission.run.id(), &admission.work);
    let (store, _) = seed_projection_admission(&database, &admission).await;
    let attempt = pending_gate(&store, &admission).await;
    runs.push((admission, attempt));
  }
  runs.sort_by_key(|(a, _)| a.run.id());
  let store = PostgresStore::new(database.pool.clone());
  // The middle Run is hidden and deliberately unindexed. It must affect neither
  // visible coverage nor a continuation spanning the two visible targets.
  for i in [0, 2] {
    let snapshot = store.factory_run_snapshot(runs[i].0.run.id()).await.unwrap();
    store
      .refresh_factory_action_projection(
        FactoryActionProjectionWork {
          run_id: snapshot.run.id(),
          source_version: snapshot.run.version(),
        },
        time(12),
      )
      .await
      .unwrap();
  }
  let mut query = ReadFactoryActionQueue {
    project_id: None,
    issue_id: None,
    after: None,
    limit: 1,
    observed_at: time(12),
    runs: FactoryRunListVisibility::restricted([runs[0].0.run.id(), runs[2].0.run.id()]).unwrap(),
    issues: IssueListVisibility::all(),
    projects: ProjectListVisibility::all(),
  };
  let first = store.read_factory_action_queue(query.clone()).await.unwrap();
  assert_eq!(first.coverage, FactoryActionProjectionCoverage::Complete);
  assert_eq!(first.total, 2);
  assert_eq!(first.items.len(), 1);
  assert_eq!(first.items[0].action.node_attempt_id, runs[0].1);
  query.after = first.next;
  let second = store.read_factory_action_queue(query.clone()).await.unwrap();
  assert_eq!(second.total, 2);
  assert_eq!(second.items.len(), 1);
  assert_eq!(second.items[0].action.node_attempt_id, runs[2].1);
  assert!(second.next.is_none());
  query.after = None;
  query.runs = FactoryRunListVisibility::all();
  assert_eq!(
    store.read_factory_action_queue(query.clone()).await.unwrap().coverage,
    FactoryActionProjectionCoverage::Incomplete
  );
  query.projects = ProjectListVisibility::restricted([
    runs[0].0.work.subject().project_id(),
    runs[2].0.work.subject().project_id(),
  ])
  .unwrap();
  assert_eq!(
    store.read_factory_action_queue(query.clone()).await.unwrap().coverage,
    FactoryActionProjectionCoverage::Complete
  );
  query.project_id = Some(runs[0].0.work.subject().project_id());
  assert_eq!(store.read_factory_action_queue(query.clone()).await.unwrap().total, 1);
  query.observed_at = time(101);
  let expired = store.read_factory_action_queue(query).await.unwrap();
  assert_eq!(expired.total, 0);
  assert!(expired.items.is_empty());
  assert!(expired.next.is_none());
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn action_response_shape_upgrade_invalidates_only_derived_rows_and_backfills_original_targets() {
  let database = TestDatabase::empty().await;
  octacity_server_store_postgres::MIGRATOR
    .run_to(91, &database.pool)
    .await
    .unwrap();
  let admission = human::human_fixture_with_contract(
    FlowDataSchema::new(key("operator.response"), key("v1"), FlowValueSchema::Boolean).unwrap(),
    None,
    false,
  );
  let (store, _) = seed_projection_admission(&database, &admission).await;
  let attempt = pending_gate(&store, &admission).await;
  let before = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  let work = store.factory_action_projection_work(1).await.unwrap().items[0];
  store.refresh_factory_action_projection(work, time(12)).await.unwrap();
  // Exact old 7.2 index format: outcome references, without response shapes.
  sqlx::query("UPDATE factory_action_items SET description=description #- '{kind,schemas}'")
    .execute(&database.pool)
    .await
    .unwrap();
  octacity_server_store_postgres::migrate(&database.pool).await.unwrap();
  let query = || FactoryActionProjectionQuery::new(admission.run.id(), time(12), None, 1).unwrap();
  let uncovered = store.read_factory_action_projection(query()).await.unwrap();
  assert_eq!(uncovered.coverage, FactoryActionProjectionCoverage::Incomplete);
  assert!(uncovered.items.is_empty());
  let resumed = store.factory_action_projection_work(1).await.unwrap().items[0];
  assert_eq!(resumed, work);
  store
    .refresh_factory_action_projection(resumed, time(12))
    .await
    .unwrap();
  let covered = store.read_factory_action_projection(query()).await.unwrap();
  assert_eq!(covered.coverage, FactoryActionProjectionCoverage::Complete);
  assert_eq!(covered.items[0].node_attempt_id, attempt);
  assert_eq!(
    serde_json::to_value(&covered.items[0]).unwrap()["kind"]["schemas"][0]["reference"]["identity"],
    "operator.response"
  );
  assert_eq!(store.factory_run_snapshot(admission.run.id()).await.unwrap(), before);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_action_queue_keeps_parallel_targets_across_edits_and_competing_response_replay() {
  let database = TestDatabase::migrated().await;
  let admission = human::human_fixture_with_contract(
    FlowDataSchema::new(key("operator.response"), key("v1"), FlowValueSchema::Boolean).unwrap(),
    None,
    false,
  );
  let (store, repository) = seed_projection_admission(&database, &admission).await;
  let first = pending_gate(&store, &admission).await;
  let old = &admission.work;
  let work = WorkEnvelope::new(
    WorkEnvelopeId::generate(),
    old.configuration().clone(),
    ExternalWorkIdentity::new("another-reviewed-execution").unwrap(),
    old.subject().clone(),
    old.artifacts().clone(),
    WorkClassification::new(old.priority(), old.risk(), old.metadata().clone()),
  )
  .unwrap();
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let root = FlowRun::root(&run, admission.flow.closure().root()).unwrap();
  let cycle = WorkflowCycle::initial(&root).unwrap();
  let flow = AdmittedFlow::new(
    admission.flow.closure().clone(),
    admission.flow.limits().clone(),
    root,
    cycle,
    admission.flow.wip_limits(),
  )
  .unwrap()
  .with_data_schemas(admission.flow.data_schemas().to_vec())
  .unwrap();
  let another = PublishedFactoryAdmission {
    work,
    run,
    flow,
    admitted_at: time(1),
  };
  store
    .admit_factory_work(testing::management_mutation(AdmitFactoryWork {
      probe: FactoryAdmissionProbe {
        source_scope: FactoryWorkSourceScope {
          source: key("manual"),
          security_scope: ManagementSecurityScope::trusted_network(),
        },
        external_identity: another.work.external_identity().clone(),
        intent_digest: digest(3),
        idempotency_key: idempotency("another-run"),
      },
      repository_version: repository.version,
      work: another.work.clone(),
      run: another.run.clone(),
      flow: another.flow.clone(),
      admitted_at: time(1),
    }))
    .await
    .unwrap();
  let second = pending_gate(&store, &another).await;
  let project = admission.work.subject().project_id();
  let issue = store
    .create_issue(testing::management_mutation(CreateIssue {
      assignees: Default::default(),
      id: octacity_server_domain::IssueId::generate(),
      intent: CreateIssueIntent {
        project_id: project,
        planning: Default::default(),
        content: IssueContent {
          title: "One persistent task".into(),
          description: String::new(),
          acceptance: None,
          priority: None,
        },
      },
      idempotency_key: idempotency("queue-card"),
      created_at: time(2),
    }))
    .await
    .unwrap()
    .issue;
  for admission in [admission.clone(), another.clone()] {
    store
      .associate_issue_work(testing::management_mutation(AssociateIssueWork {
        project_id: project,
        issue_id: issue.id,
        request_id: None,
        admission,
      }))
      .await
      .unwrap();
  }
  for row in store.factory_action_projection_work(100).await.unwrap().items {
    store.refresh_factory_action_projection(row, time(12)).await.unwrap();
  }
  let mut query = ReadFactoryActionQueue {
    project_id: Some(project),
    issue_id: Some(issue.id),
    after: None,
    limit: 100,
    observed_at: time(12),
    runs: FactoryRunListVisibility::all(),
    projects: ProjectListVisibility::all(),
    issues: IssueListVisibility::all(),
  };
  let before = store.read_factory_action_queue(query.clone()).await.unwrap();
  assert_eq!(before.total, 2);
  assert_eq!(
    before
      .items
      .iter()
      .map(|i| i.action.node_attempt_id)
      .collect::<BTreeSet<_>>(),
    [first, second].into_iter().collect()
  );
  assert!(
    before
      .items
      .iter()
      .all(|i| i.association.as_ref().unwrap().issue_id == issue.id)
  );
  let mut content = issue.content.clone();
  content.title = "Edited without retargeting".into();
  store
    .edit_issue(testing::management_mutation(EditIssue {
      intent: EditIssueIntent {
        project_id: project,
        id: issue.id,
        expected_version: issue.version,
        content,
      },
      idempotency_key: idempotency("queue-edit"),
      edited_at: time(13),
    }))
    .await
    .unwrap();
  let after = store.read_factory_action_queue(query.clone()).await.unwrap();
  assert_eq!(
    after
      .items
      .iter()
      .map(|i| (i.action.run_id, i.action.node_attempt_id, i.action.run_version))
      .collect::<Vec<_>>(),
    before
      .items
      .iter()
      .map(|i| (i.action.run_id, i.action.node_attempt_id, i.action.run_version))
      .collect::<Vec<_>>()
  );
  query.issue_id = None;
  query.issues = IssueListVisibility::none();
  let hidden = store.read_factory_action_queue(query.clone()).await.unwrap();
  assert_eq!(hidden.total, 0);
  assert!(hidden.items.is_empty());
  assert!(hidden.next.is_none());
  assert_eq!(hidden.coverage, FactoryActionProjectionCoverage::Complete);
  let source = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  let response = |name: &str, value| {
    testing::management_mutation(ApplyFactoryRunControl {
      run_id: admission.run.id(),
      expected_version: source.run.version(),
      idempotency_key: idempotency(name),
      intent: FactoryRunControlIntent::ResolveNode {
        node_attempt_id: first,
        outcome: key("approved"),
        payload: FlowPayload::new(
          admission
            .flow
            .data_schema(
              admission
                .flow
                .closure()
                .definition(admission.flow.closure().root())
                .unwrap()
                .nodes()[0]
                .outcomes()[0]
                .schema(),
            )
            .unwrap(),
          serde_json::json!(value),
        )
        .unwrap(),
      },
      requested_at: time(14),
    })
  };
  let one = response("competing-one", true);
  let two = response("competing-two", false);
  let (a, b) = tokio::join!(
    store.apply_factory_run_control(one.clone()),
    store.apply_factory_run_control(two.clone())
  );
  assert_ne!(a.is_ok(), b.is_ok(), "only one fresh response intent can win");
  let (winning, receipt) = if let Ok(receipt) = a {
    assert!(matches!(b, Err(StoreError::Conflict { .. })));
    (one, receipt)
  } else {
    (two, b.unwrap())
  };
  let accepted = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  assert!(
    accepted.flow.completions.is_empty(),
    "command acceptance is not owner consumption"
  );
  let ownership = FactoryRunClaimRecord::new(
    accepted.run.id(),
    key("recovered.owner"),
    FactoryClaim::new(FactoryClaimFence::new(digest(4)), time(15), time(1000)).unwrap(),
  );
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: accepted.run.id(),
      expected_version: accepted.run.version(),
      record: ownership.clone(),
      audit: FactoryAuditFact::new(
        accepted.run.id(),
        AuditActorKind::Worker,
        None,
        key("factory.claimed"),
        digest(4),
        key("accepted"),
        time(15),
      ),
    })
    .await
    .unwrap();
  let claimed = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  store
    .commit_factory_run_transition(testing::factory_flow_contract_transition(&claimed, &ownership, 16))
    .await
    .unwrap();
  let replay = store.apply_factory_run_control(winning).await.unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.version, receipt.version);
  query.issues = IssueListVisibility::all();
  query.issue_id = Some(issue.id);
  query.observed_at = time(17);
  let remaining = store.read_factory_action_queue(query).await.unwrap();
  assert_eq!(remaining.total, 1);
  assert_eq!(remaining.items[0].action.node_attempt_id, second);
  assert_eq!(remaining.coverage, FactoryActionProjectionCoverage::Incomplete);
  database.cleanup().await;
}
