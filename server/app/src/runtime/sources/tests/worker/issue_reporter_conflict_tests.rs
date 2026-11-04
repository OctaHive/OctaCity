//! Classified outcomes and reviewed resolution over the existing Reporter boundary.
use super::*;
use octacity_server_application::ResolveIssueSourceConflictCommand;
use octacity_server_store::FactoryOutboxFailure;
use octacity_server_store::{IssueSourceResolutionReview, IssueSourceResolutionStore, ResolveIssueSourceConflict};

struct ResolutionReader;
#[async_trait::async_trait]
impl octacity_server_application::WorkSourceObservations for ResolutionReader {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ProjectId,
    _: WorkSourceBinding,
  ) -> Result<(), ApplicationError> {
    Ok(())
  }
  async fn read(
    &self,
    _: &ManagementRequestContext,
    _: ReadWorkSourceObservations,
  ) -> Result<WorkSourceObservationPage, ApplicationError> {
    panic!("policy resolution must not call the provider")
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_reporter_resolution_rechecks_current_writer_grant_before_accepting_policy() {
  let (fixture, database, store) = standalone("resolution-authority").await;
  let (_, profile, command) = manual_issue(&store, &fixture).await;
  let accepted = store
    .transition_issue(management_mutation(command))
    .await
    .unwrap()
    .issue;
  let root = fixture.root.path().join("resolution-authority");
  std::fs::write(root.join("reporter-mode.json"), r#"{"conflict_on_apply":true}"#).unwrap();
  fixture.clock.0.store(120, Ordering::SeqCst);
  issue_worker(&fixture, store.clone(), vec![profile.clone()])
    .run_once(time(120))
    .await
    .unwrap();
  let captured = std::fs::read_to_string(root.join("reporter-requests.jsonl")).unwrap();
  let first: serde_json::Value = serde_json::from_str(captured.lines().next().unwrap()).unwrap();
  let operation = original_intent(&first)["projection_id"]
    .as_str()
    .unwrap()
    .parse()
    .unwrap();
  let original = store.issue_push_projection(operation).await.unwrap();
  let scope = octacity_server_store::ManagementSecurityScope::trusted_network();
  let mapping = store
    .issue_source_mapping(accepted.project_id, accepted.id, &scope, None)
    .await
    .unwrap()
    .unwrap();
  let (access, id) = managed_access(&fixture, profile.binding(), IntegrationPurpose::TaskUpdate).await;
  let owner = IssueSourceHandlers::new(
    store.clone(),
    Arc::new(ResolutionReader),
    Arc::new(TrustedNetworkManagementPolicy),
    fixture.clock.clone(),
  )
  .with_source_mappings(vec![InstalledIssueSourceMapping {
    project_id: accepted.project_id,
    source: mapping.definition.source.clone(),
    native_resource: ExternalWorkIdentity::new("native-container").unwrap(),
    readable: WorkSourceReportingPolicy {
      ownership: None,
      labels: vec![],
      states: vec![],
    },
    writer: Some(InstalledIssueSourceWriter {
      binding: profile.binding().clone(),
      plugin: fixture.profile.read.plugin.clone(),
      access,
      effects: WorkSourceReportingPolicy {
        ownership: None,
        labels: vec!["custom-ready".into()],
        states: vec![],
      },
    }),
  }])
  .unwrap();
  let handler = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(owner));
  let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
  let resolution = ResolveIssueSourceConflictCommand {
    intent: octacity_server_store::ResolveIssueSourceConflictIntent {
      review: IssueSourceResolutionReview {
        operation_id: operation,
        expected_record: original.record.id,
      },
      mapping: PublishIssueSourceMappingIntent {
        project_id: accepted.project_id,
        issue_id: accepted.id,
        expected_issue_version: accepted.version,
        expected_mapping: Some(mapping.reference()),
        definition: mapping.definition.clone(),
      },
    },
    idempotency_key: IdempotencyKey::new("authorized-resolution").unwrap(),
    resolved_at: time(200),
  };
  let access_command = |resources, version, key| PublishIntegrationProjectAccess {
    intent: IntegrationProjectAccessIntent {
      connection_id: id,
      connection_version: IntegrationVersion::INITIAL,
      project_id: accepted.project_id,
      expected_current_version: Some(version),
      resources,
    },
    idempotency_key: IdempotencyKey::new(key).unwrap(),
    published_at: time(150),
  };
  fixture
    .store
    .publish_integration_project_access(management_mutation(access_command(
      vec![],
      IntegrationVersion::INITIAL,
      "revoke-resolution",
    )))
    .await
    .unwrap();
  assert!(handler.handle_command(&context, resolution.clone()).await.is_err());
  assert_eq!(
    store
      .issue_source_mapping(accepted.project_id, accepted.id, &scope, None)
      .await
      .unwrap(),
    Some(mapping.clone())
  );
  fixture
    .store
    .publish_integration_project_access(management_mutation(access_command(
      vec![IntegrationResourceSelection {
        reference: profile.binding().resource.clone(),
        purposes: vec![IntegrationPurpose::TaskUpdate],
      }],
      IntegrationVersion::new(2).unwrap(),
      "restore-resolution",
    )))
    .await
    .unwrap();
  fixture.clock.0.store(200, Ordering::SeqCst);
  let resolved = handler.handle_command(&context, resolution.clone()).await.unwrap();
  assert!(resolved.projection.is_some());
  assert_eq!(
    handler
      .handle_command(&context, resolution.clone())
      .await
      .unwrap()
      .projection,
    resolved.projection
  );
  assert_eq!(
    std::fs::read_to_string(root.join("reporter-requests.jsonl")).unwrap(),
    captured
  );
  std::fs::write(root.join("reporter-mode.json"), "{}").unwrap();
  fixture.clock.0.store(220, Ordering::SeqCst);
  let unqualified = issue_worker(&fixture, store.clone(), vec![profile.clone()]);
  assert_eq!(
    unqualified.run_once(time(220)).await.unwrap().failed,
    1,
    "a managed policy cannot borrow an identical legacy writer without current grant checks"
  );
  assert_eq!(
    std::fs::read_to_string(root.join("reporter-requests.jsonl")).unwrap(),
    captured
  );
  assert_eq!(
    store
      .issue_push_projection(resolved.projection.unwrap())
      .await
      .unwrap()
      .record
      .failure,
    Some(FactoryOutboxFailure::Forbidden)
  );
  // A new reviewed repair can use the original connection through the common resolver.
  let rejected = store.issue_push_projection(resolved.projection.unwrap()).await.unwrap();
  let mut repair = resolution;
  repair.intent.mapping.expected_mapping = Some(resolved.mapping.reference());
  repair.intent.review = IssueSourceResolutionReview {
    operation_id: rejected.record.operation_id,
    expected_record: rejected.record.id,
  };
  repair.idempotency_key = IdempotencyKey::new("composed-writer-repair").unwrap();
  repair.resolved_at = time(240);
  fixture.clock.0.store(240, Ordering::SeqCst);
  let repaired = handler.handle_command(&context, repair).await.unwrap();
  let qualified = issue_worker_with_connections(
    &fixture,
    store.clone(),
    vec![profile],
    Some(ManagedIntegrationConnections::new(fixture.store.clone())),
  );
  fixture.clock.0.store(260, Ordering::SeqCst);
  assert_eq!(qualified.run_once(time(260)).await.unwrap().delivered, 1);
  assert_eq!(
    store
      .issue_push_projection(repaired.projection.unwrap())
      .await
      .unwrap()
      .record
      .state,
    octacity_server_store::FactoryOutboxState::Delivered
  );
  assert_eq!(
    store
      .issue_push_projection(rejected.record.operation_id)
      .await
      .unwrap()
      .record,
    rejected.record
  );
  assert_eq!(
    store.issue(accepted.project_id, accepted.id, None).await.unwrap(),
    accepted
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_reporter_resolution_publishes_one_reviewed_policy_intent_without_another_status_action() {
  let (fixture, database, store) = standalone("resolution-tracker").await;
  let (_, profile, command) = manual_issue(&store, &fixture).await;
  let accepted = store
    .transition_issue(management_mutation(command))
    .await
    .unwrap()
    .issue;
  let root = fixture.root.path().join("resolution-tracker");
  std::fs::write(root.join("reporter-mode.json"), r#"{"conflict_on_apply":true}"#).unwrap();
  fixture.clock.0.store(120, Ordering::SeqCst);
  let worker = issue_worker(&fixture, store.clone(), vec![profile.clone()]);
  assert_eq!(worker.run_once(time(120)).await.unwrap().failed, 1);
  let calls = std::fs::read_to_string(root.join("reporter-requests.jsonl")).unwrap();
  let first: serde_json::Value = serde_json::from_str(calls.lines().next().unwrap()).unwrap();
  let operation = original_intent(&first)["projection_id"]
    .as_str()
    .unwrap()
    .parse()
    .unwrap();
  let original = store.issue_push_projection(operation).await.unwrap();
  let scope = octacity_server_store::ManagementSecurityScope::trusted_network();
  let mapping = store
    .issue_source_mapping(accepted.project_id, accepted.id, &scope, None)
    .await
    .unwrap()
    .unwrap();
  let history = store
    .issue_status_history(accepted.project_id, accepted.id, None, 20)
    .await
    .unwrap();
  let resolution = ResolveIssueSourceConflict {
    review: IssueSourceResolutionReview {
      operation_id: operation,
      expected_record: original.record.id,
    },
    publication: PublishIssueSourceMapping {
      writer_access: None,
      intent: PublishIssueSourceMappingIntent {
        project_id: accepted.project_id,
        issue_id: accepted.id,
        expected_issue_version: accepted.version,
        expected_mapping: Some(mapping.reference()),
        definition: mapping.definition.clone(),
      },
      native_resource: ExternalWorkIdentity::new("native-container").unwrap(),
      readable: WorkSourceReportingPolicy {
        ownership: None,
        labels: vec![],
        states: vec![],
      },
      writable: Some(WorkSourceReportingPolicy {
        ownership: None,
        labels: vec!["custom-ready".into()],
        states: vec![],
      }),
      idempotency_key: IdempotencyKey::new("reviewed-resolution").unwrap(),
      published_at: time(200),
    },
  };
  let mut unsupported = resolution.clone();
  unsupported
    .publication
    .intent
    .definition
    .status
    .as_mut()
    .unwrap()
    .push
    .as_mut()
    .unwrap()
    .statuses[0]
    .add = vec!["outside-authority".into()];
  assert!(
    store
      .resolve_issue_source_conflict(management_mutation(unsupported))
      .await
      .is_err()
  );
  let resolved = store
    .resolve_issue_source_conflict(management_mutation(resolution.clone()))
    .await
    .unwrap();
  let next = resolved.projection.unwrap();
  assert_ne!(next, operation);
  assert_eq!(
    store
      .issue_push_projection(next)
      .await
      .unwrap()
      .intent
      .desired
      .unwrap()
      .command
      .resolves,
    Some(operation)
  );
  assert_eq!(
    store.issue_push_projection(operation).await.unwrap().record,
    original.record
  );
  let mut competing = resolution.clone();
  competing.publication.idempotency_key = IdempotencyKey::new("competing-resolution").unwrap();
  assert!(
    store
      .resolve_issue_source_conflict(management_mutation(competing))
      .await
      .is_err()
  );
  std::fs::write(root.join("reporter-mode.json"), "{}").unwrap();
  fixture.clock.0.store(220, Ordering::SeqCst);
  assert_eq!(worker.run_once(time(220)).await.unwrap().delivered, 1);
  let restarted = octacity_server_store_postgres::PostgresStore::new(database.pool.clone());
  let replay = restarted
    .resolve_issue_source_conflict(management_mutation(resolution))
    .await
    .unwrap();
  assert_eq!(replay.projection, Some(next));
  assert_eq!(replay.disposition, octacity_server_store::MutationDisposition::Replayed);
  assert_eq!(
    restarted.issue(accepted.project_id, accepted.id, None).await.unwrap(),
    accepted
  );
  assert_eq!(
    restarted
      .issue_status_history(accepted.project_id, accepted.id, None, 20)
      .await
      .unwrap(),
    history
  );
  assert_eq!(worker.run_once(time(221)).await.unwrap().claimed, 0);
  assert_eq!(
    restarted.issue_push_projection(operation).await.unwrap().record,
    original.record
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_reporter_revoked_or_disabled_writer_stops_calls_without_losing_local_results() {
  for disabled in [false, true] {
    let (fixture, database, store) = standalone("managed-issue-tracker").await;
    let (_, profile, command) = manual_issue(&store, &fixture).await;
    let (access, id) = managed_access(&fixture, profile.binding(), IntegrationPurpose::TaskUpdate).await;
    let worker = issue_worker(&fixture, store.clone(), vec![profile.with_managed_access(access)]);
    let accepted = store.transition_issue(management_mutation(command)).await.unwrap();
    let root = fixture.root.path().join("managed-issue-tracker");
    std::fs::write(
      root.join("reporter-mode.json"),
      r#"{"failure":{"class":"transient","retry_after_millis":null}}"#,
    )
    .unwrap();
    fixture.clock.0.store(120, Ordering::SeqCst);
    assert_eq!(worker.run_once(time(120)).await.unwrap().retrying, 1);
    let captured = std::fs::read_to_string(root.join("reporter-requests.jsonl")).unwrap();
    if disabled {
      let state = fixture.store.integration_administration(id).await.unwrap();
      let receipt = fixture
        .store
        .begin_integration_administration(management_mutation(BeginIntegrationAdministration {
          intent: IntegrationAdministrationIntent {
            id,
            expected_metadata_version: IntegrationVersion::INITIAL,
            expected_revision: state.revision,
            action: IntegrationAdministrationAction::Disable,
          },
          idempotency_key: IdempotencyKey::new("disable-writer").unwrap(),
          requested_at: time(200),
        }))
        .await
        .unwrap();
      fixture
        .store
        .complete_integration_administration(management_mutation(receipt))
        .await
        .unwrap();
    } else {
      fixture
        .store
        .publish_integration_project_access(management_mutation(PublishIntegrationProjectAccess {
          intent: IntegrationProjectAccessIntent {
            connection_id: id,
            connection_version: IntegrationVersion::INITIAL,
            project_id: fixture.input.project_id(),
            expected_current_version: Some(IntegrationVersion::INITIAL),
            resources: vec![],
          },
          idempotency_key: IdempotencyKey::new("revoke-writer").unwrap(),
          published_at: time(200),
        }))
        .await
        .unwrap();
    }
    std::fs::write(root.join("reporter-mode.json"), "{}").unwrap();
    fixture.clock.0.store(220, Ordering::SeqCst);
    assert_eq!(worker.run_once(time(220)).await.unwrap().failed, 1);
    assert_eq!(
      std::fs::read_to_string(root.join("reporter-requests.jsonl")).unwrap(),
      captured
    );
    let first: serde_json::Value = serde_json::from_str(captured.lines().next().unwrap()).unwrap();
    let operation = original_intent(&first)["projection_id"]
      .as_str()
      .unwrap()
      .parse()
      .unwrap();
    assert_eq!(
      store.issue_push_projection(operation).await.unwrap().record.failure,
      Some(FactoryOutboxFailure::Forbidden)
    );
    assert_eq!(
      store
        .issue(accepted.issue.project_id, accepted.issue.id, None)
        .await
        .unwrap(),
      accepted.issue
    );
    database.cleanup().await;
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_reporter_unsupported_is_terminal_and_provider_outage_exhausts_original_bounds() {
  for (failure, expected, attempts) in [
    ("unsupported", FactoryOutboxFailure::Unsupported, 1),
    ("transient", FactoryOutboxFailure::Exhausted, 3),
  ] {
    let (fixture, database, store) = standalone("bounded-tracker").await;
    let (_, profile, command) = manual_issue(&store, &fixture).await;
    let accepted = store.transition_issue(management_mutation(command)).await.unwrap();
    let root = fixture.root.path().join("bounded-tracker");
    std::fs::write(
      root.join("reporter-mode.json"),
      serde_json::json!({"failure":{"class":failure,"retry_after_millis":null}}).to_string(),
    )
    .unwrap();
    let worker = issue_worker(&fixture, store.clone(), vec![profile]);
    for attempt in 0..attempts {
      let now = 120 + 100 * attempt;
      fixture.clock.0.store(now, Ordering::SeqCst);
      let result = worker.run_once(time(now)).await.unwrap();
      assert_eq!(result.retrying, u16::from(attempt + 1 < attempts));
      assert_eq!(result.failed, u16::from(attempt + 1 == attempts));
    }
    let calls = std::fs::read_to_string(root.join("reporter-requests.jsonl")).unwrap();
    assert_eq!(calls.lines().count(), attempts as usize);
    let first: serde_json::Value = serde_json::from_str(calls.lines().next().unwrap()).unwrap();
    let operation = original_intent(&first)["projection_id"]
      .as_str()
      .unwrap()
      .parse()
      .unwrap();
    assert_eq!(
      store.issue_push_projection(operation).await.unwrap().record.failure,
      Some(expected)
    );
    fixture.clock.0.store(1020, Ordering::SeqCst);
    assert_eq!(worker.run_once(time(1020)).await.unwrap().claimed, 0);
    assert_eq!(
      store
        .issue(accepted.issue.project_id, accepted.issue.id, None)
        .await
        .unwrap(),
      accepted.issue
    );
    database.cleanup().await;
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn issue_reporter_conflict_is_retained_without_retry_or_local_status_rollback() {
  let (fixture, database, store) = standalone("conflict-tracker").await;
  let (_, profile, command) = manual_issue(&store, &fixture).await;
  let accepted = store.transition_issue(management_mutation(command)).await.unwrap();
  let root = fixture.root.path().join("conflict-tracker");
  std::fs::write(root.join("reporter-mode.json"), r#"{"conflict_on_apply":true}"#).unwrap();
  let human = serde_json::json!({"b".repeat(64):"human-edit"});
  std::fs::write(root.join("reporter-state.json"), human.to_string()).unwrap();
  fixture.clock.0.store(120, Ordering::SeqCst);
  let worker = issue_worker(&fixture, store.clone(), vec![profile]);
  assert_eq!(worker.run_once(time(120)).await.unwrap().failed, 1);
  let calls = std::fs::read_to_string(root.join("reporter-requests.jsonl")).unwrap();
  let first: serde_json::Value = serde_json::from_str(calls.lines().next().unwrap()).unwrap();
  let operation = original_intent(&first)["projection_id"]
    .as_str()
    .unwrap()
    .parse()
    .unwrap();
  let restarted = octacity_server_store_postgres::PostgresStore::new(database.pool.clone());
  let original = restarted.issue_push_projection(operation).await.unwrap();
  assert_eq!(original.record.failure, Some(FactoryOutboxFailure::Conflict));
  fixture.clock.0.store(220, Ordering::SeqCst);
  assert_eq!(worker.run_once(time(220)).await.unwrap().claimed, 0);
  assert_eq!(
    restarted
      .issue(accepted.issue.project_id, accepted.issue.id, None)
      .await
      .unwrap(),
    accepted.issue
  );
  assert_eq!(
    serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(root.join("reporter-state.json")).unwrap())
      .unwrap(),
    human
  );
  database.cleanup().await;
}
