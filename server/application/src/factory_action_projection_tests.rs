//! Projection persistence is exercised through the same public store port as workers.
use super::*;

#[test]
fn action_projection_persists_exact_targets_and_reports_unfinished_backfill() {
  crate::factory_flow_tests::run_ready(async {
    let (store, snapshot) = pending(human_fixture()).await;
    let query = || FactoryActionProjectionQuery::new(snapshot.run.id(), at(12), None, 1).unwrap();
    let missing = store.read_factory_action_projection(query()).await.unwrap();
    assert_eq!(missing.coverage, FactoryActionProjectionCoverage::Incomplete);
    assert!(missing.items.is_empty());
    let batch = store.factory_action_projection_work(1).await.unwrap();
    assert_eq!(batch.items.len(), 1);
    assert_eq!(batch.items[0].run_id, snapshot.run.id());
    assert_eq!(batch.items[0].source_version, snapshot.run.version());
    store
      .refresh_factory_action_projection(batch.items[0], at(12))
      .await
      .unwrap();
    let restarted = store.clone();
    let page = restarted.read_factory_action_projection(query()).await.unwrap();
    assert_eq!(page.coverage, FactoryActionProjectionCoverage::Complete);
    assert_eq!(page.observed_at, Some(at(12)));
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].node_attempt_id, snapshot.flow.attempts[0].id());
    assert_eq!(page.items[0].contract, snapshot.flow.runs[0].definition());
    assert_eq!(page.items[0].run_version, snapshot.run.version());
    assert!(
      restarted
        .factory_action_projection_work(1)
        .await
        .unwrap()
        .items
        .is_empty()
    );
    assert_eq!(
      restarted.factory_run_snapshot(snapshot.run.id()).await.unwrap(),
      snapshot,
      "index maintenance cannot progress execution"
    );
  });
}

#[test]
fn action_projection_worker_resumes_bounded_backfill_without_execution_ownership() {
  crate::factory_flow_tests::run_ready(async {
    let (store, snapshot) = pending(human_fixture()).await;
    let second = human_fixture();
    store
      .seed_factory_run(
        second.clone(),
        FactoryDigest::from_bytes([9; 32]),
        testing::management_mutation(()).audit(),
      )
      .unwrap();
    let worker = crate::FactoryActionProjectionWorker::new(store.clone());
    assert!(worker.run_once(at(20), 0).await.is_err());
    assert!(worker.run_once(at(20), 101).await.is_err());
    let first = worker.run_once(at(20), 1).await.unwrap();
    assert_eq!(first.completed, 1);
    assert!(first.has_more);
    let restarted = crate::FactoryActionProjectionWorker::new(store.clone());
    let last = restarted.run_once(at(21), 1).await.unwrap();
    assert_eq!(last.completed, 1);
    assert!(!last.has_more);
    assert_eq!(restarted.run_once(at(22), 1).await.unwrap().completed, 0);
    for id in [snapshot.run.id(), second.run.id()] {
      let query = FactoryActionProjectionQuery::new(id, at(22), None, 1).unwrap();
      assert_eq!(
        store.read_factory_action_projection(query).await.unwrap().coverage,
        FactoryActionProjectionCoverage::Complete
      );
    }
    assert_eq!(store.factory_run_snapshot(snapshot.run.id()).await.unwrap(), snapshot);
    assert_eq!(
      store.factory_run_snapshot(second.run.id()).await.unwrap().run,
      second.run
    );
  });
}

pub(super) async fn assert_materialized_actions(
  store: &testing::InMemoryFactoryConfigurationStore,
  snapshot: &FactoryRunSnapshot,
  observed: i64,
  expected: &[NodeAttemptId],
) {
  let work = store
    .factory_action_projection_work(100)
    .await
    .unwrap()
    .items
    .into_iter()
    .find(|row| row.run_id == snapshot.run.id())
    .unwrap();
  assert_eq!(work.source_version, snapshot.run.version());
  store
    .refresh_factory_action_projection(work, at(observed))
    .await
    .unwrap();
  let query = FactoryActionProjectionQuery::new(snapshot.run.id(), at(observed), None, 1).unwrap();
  let page = store.read_factory_action_projection(query).await.unwrap();
  assert_eq!(page.coverage, FactoryActionProjectionCoverage::Complete);
  assert_eq!(page.run_version, snapshot.run.version());
  assert_eq!(
    page.items.iter().map(|row| row.node_attempt_id).collect::<Vec<_>>(),
    expected
  );
  assert_eq!(page.next, None, "retired historical targets cannot affect continuation");
}

#[test]
fn action_projection_revalidates_expiry_and_rejects_forged_read_bounds() {
  crate::factory_flow_tests::run_ready(async {
    let (store, snapshot) = pending(human_fixture()).await;
    let work = store.factory_action_projection_work(1).await.unwrap().items[0];
    store.refresh_factory_action_projection(work, at(12)).await.unwrap();
    let deadline = snapshot.flow.attempts[0].deadline().unix_millis();
    for (time, coverage, count) in [
      (11, FactoryActionProjectionCoverage::Incomplete, 0),
      (deadline, FactoryActionProjectionCoverage::Complete, 1),
      (deadline + 1, FactoryActionProjectionCoverage::Complete, 0),
    ] {
      let page = store
        .read_factory_action_projection(
          FactoryActionProjectionQuery::new(snapshot.run.id(), at(time), None, 1).unwrap(),
        )
        .await
        .unwrap();
      assert_eq!(page.coverage, coverage);
      assert_eq!(page.items.len(), count);
      assert_eq!(page.next, None);
    }
    let after = store
      .read_factory_action_projection(
        FactoryActionProjectionQuery::new(snapshot.run.id(), at(12), Some(snapshot.flow.attempts[0].id()), 1).unwrap(),
      )
      .await
      .unwrap();
    assert!(after.items.is_empty());
    for limit in [0, 101] {
      assert!(store.factory_action_projection_work(limit).await.is_err());
      assert!(
        store
          .read_factory_action_projection(FactoryActionProjectionQuery {
            run_id: snapshot.run.id(),
            observed_at: at(12),
            after: None,
            limit
          })
          .await
          .is_err()
      );
    }
    assert!(matches!(
      store.refresh_factory_action_projection(work, at(11)).await,
      Err(StoreError::Conflict { .. })
    ));
    assert_eq!(store.factory_run_snapshot(snapshot.run.id()).await.unwrap(), snapshot);
  });
}

#[test]
fn action_response_projection_publishes_only_its_pinned_response_shapes() {
  crate::factory_flow_tests::run_ready(async {
    let (store, snapshot) = pending(human_fixture()).await;
    let work = store.factory_action_projection_work(1).await.unwrap().items[0];
    store.refresh_factory_action_projection(work, at(12)).await.unwrap();
    let page = store
      .read_factory_action_projection(FactoryActionProjectionQuery::new(snapshot.run.id(), at(12), None, 1).unwrap())
      .await
      .unwrap();
    let description = serde_json::to_value(&page.items[0]).unwrap();
    assert_eq!(description["kind"]["schemas"].as_array().map(Vec::len), Some(1));
    assert_eq!(
      description["kind"]["schemas"][0]["reference"]["identity"],
      "operator.response"
    );
    assert_eq!(description["kind"]["schemas"][0]["shape"]["type"], "boolean");
    assert!(
      !description.to_string().contains("operator.confirmation"),
      "input schemas/bodies are not response contracts"
    );
  });
}

#[test]
fn exact_action_observation_separates_recorded_intent_from_owner_consumption_and_replay() {
  crate::factory_flow_tests::run_ready(async {
    let (store, snapshot) = pending(human_fixture()).await;
    let attempt = snapshot.flow.attempts[0].id();
    struct Clock(std::sync::atomic::AtomicI64);
    impl crate::FactoryExecutionClock for Clock {
      fn now(&self) -> Result<octacity_server_domain::Timestamp, crate::ApplicationError> {
        Ok(at(self.0.load(std::sync::atomic::Ordering::SeqCst)))
      }
    }
    let clock = Arc::new(Clock(std::sync::atomic::AtomicI64::new(12)));
    let queries = crate::AuthorizedQueryHandler::new(
      Arc::new(crate::TrustedNetworkManagementPolicy),
      Arc::new(crate::FactoryActionHandlers::new(
        store.clone(),
        Arc::new(crate::TrustedNetworkManagementPolicy),
        clock.clone(),
      )),
    );
    let query = crate::GetFactoryNodeActionQuery {
      run_id: snapshot.run.id(),
      node_attempt_id: attempt,
      project_id: None,
      issue_id: None,
    };
    let waiting = queries.handle_query(&context(), query.clone()).await.unwrap();
    assert_eq!(serde_json::to_value(waiting.state).unwrap(), "waiting");
    assert_eq!(waiting.description.unwrap().node_attempt_id, attempt);
    let controls = FactoryControlHandlers::new(store.clone());
    let command = crate::SubmitFactoryNodeResponseCommand {
      run_id: snapshot.run.id(),
      node_attempt_id: attempt,
      outcome: key("approved"),
      value: serde_json::json!(true),
      expected_version: snapshot.run.version(),
      idempotency_key: "observed-answer".parse().unwrap(),
      requested_at: at(12),
    };
    let accepted = controls
      .execute_management_command(
        &context(),
        &ManagementAuthorizationGrant::new(ManagementVisibility::all()),
        command.clone(),
      )
      .await
      .unwrap();
    let recorded = queries.handle_query(&context(), query.clone()).await.unwrap();
    assert_eq!(serde_json::to_value(recorded.state).unwrap(), "recorded");
    assert_eq!(recorded.recorded_version, Some(accepted.version));
    assert!(recorded.description.is_none());
    let source = store.factory_run_snapshot(snapshot.run.id()).await.unwrap();
    assert!(source.flow.completions.is_empty());
    let ownership = claim(store.as_ref(), &source, 13).await;
    let owner = FactoryFlowRunner::new(
      store.clone(),
      Arc::new(NoExternalEffects),
      Arc::new(Inputs),
      crate::factory_node_test_support::clock(),
    );
    for time in 14..=16 {
      owner
        .reconcile_flow(snapshot.run.id(), ownership.id, at(time))
        .await
        .unwrap();
    }
    clock.0.store(17, std::sync::atomic::Ordering::SeqCst);
    let consumed = queries.handle_query(&context(), query).await.unwrap();
    assert_eq!(serde_json::to_value(consumed.state).unwrap(), "consumed");
    assert_eq!(consumed.recorded_version, Some(accepted.version));
    let replay = controls
      .execute_management_command(
        &context(),
        &ManagementAuthorizationGrant::new(ManagementVisibility::all()),
        command,
      )
      .await
      .unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.version, accepted.version);
  });
}

#[test]
fn issue_action_observation_checks_original_association_before_offering_a_response() {
  crate::factory_flow_tests::run_ready(async {
    let admission = human_fixture();
    let (store, snapshot) = pending(admission.clone()).await;
    let project = snapshot.work.subject().project_id();
    let issues = crate::issue_test_support::manual_issue_store(project).await;
    let mut cards = Vec::new();
    for key in ["first-card", "other-card"] {
      cards.push(
        issues
          .create_issue(testing::management_mutation(CreateIssue {
            assignees: Default::default(),
            id: octacity_server_domain::IssueId::generate(),
            intent: CreateIssueIntent {
              project_id: project,
              planning: Default::default(),
              content: IssueContent {
                title: key.into(),
                description: String::new(),
                acceptance: None,
                priority: None,
              },
            },
            idempotency_key: key.parse().unwrap(),
            created_at: at(2),
          }))
          .await
          .unwrap()
          .issue,
      );
    }
    issues
      .associate_issue_work(testing::management_mutation(AssociateIssueWork {
        project_id: project,
        issue_id: cards[0].id,
        request_id: None,
        admission,
      }))
      .await
      .unwrap();
    struct Clock;
    impl crate::FactoryExecutionClock for Clock {
      fn now(&self) -> Result<octacity_server_domain::Timestamp, crate::ApplicationError> {
        Ok(at(12))
      }
    }
    let handlers = crate::FactoryActionHandlers::new(
      store.clone(),
      Arc::new(crate::TrustedNetworkManagementPolicy),
      Arc::new(Clock),
    )
    .with_issue_associations(issues.clone());
    let query = crate::GetFactoryNodeActionQuery {
      run_id: snapshot.run.id(),
      node_attempt_id: snapshot.flow.attempts[0].id(),
      project_id: Some(project),
      issue_id: Some(cards[0].id),
    };
    let queries =
      crate::AuthorizedQueryHandler::new(Arc::new(crate::TrustedNetworkManagementPolicy), Arc::new(handlers));
    let observed = queries.handle_query(&context(), query.clone()).await.unwrap();
    assert_eq!(serde_json::to_value(observed.state).unwrap(), "waiting");
    assert!(
      !observed.accepted_candidate,
      "a declared gate is not proof of candidate acceptance"
    );
    let mut foreign = query;
    foreign.issue_id = Some(cards[1].id);
    assert!(
      queries.handle_query(&context(), foreign).await.is_err(),
      "another visible card cannot offer this execution's response"
    );
    assert_eq!(store.factory_run_snapshot(snapshot.run.id()).await.unwrap(), snapshot);
    assert_eq!(issues.issue(project, cards[0].id, None).await.unwrap(), cards[0]);
  });
}
