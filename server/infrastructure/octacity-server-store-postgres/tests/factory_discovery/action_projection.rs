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
