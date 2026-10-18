//! Management publication feeds the ordinary Source worker, with no Issue/card requirement.
use super::*;

#[tokio::test]
async fn managed_input_publication_uses_its_connection_before_dispatch_and_after_revocation() {
  for provider in ["github", "gitlab", "jira", "custom-tracker"] {
    let mut fixture = Fixture::enabled(provider).await;
    let (_, id) = managed_access(&fixture, &fixture.profile.binding, IntegrationPurpose::TaskRead).await;
    let mut definition = fixture.input.definition().clone();
    definition.connection = Some(WorkSourceConnectionRef {
      id,
      version: IntegrationVersion::INITIAL,
      access_version: IntegrationVersion::INITIAL,
      source_configuration: fixture.profile.binding.configuration.clone(),
      reporter_configuration: None,
    });
    let handlers = FactorySourceConfigurationHandlers::new(
      fixture.store.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
      vec![WorkSourceCapability {
        project_id: fixture.input.project_id(),
        role: WorkSourceCapabilityRole::Source,
        binding: fixture.profile.binding.clone(),
      }],
    )
    .unwrap();
    let published = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(handlers))
      .handle_command(
        &fixture.profile.context,
        PublishWorkSourceConfigurationCommand {
          intent: WorkSourceConfigurationIntent {
            project_id: fixture.input.project_id(),
            inputs: vec![WorkSourceInputIntent {
              id: fixture.input.id(),
              expected_version: Some(fixture.input.version()),
              definition,
            }],
            group: None,
          },
          idempotency_key: IdempotencyKey::new("managed-input").unwrap(),
          published_at: time(4),
        },
      )
      .await
      .unwrap();
    fixture.input = published.publication.inputs[0].input.clone();
    let directory = fixture.root.path().join(provider);
    assert_eq!(
      fixture
        .worker()
        .run_once(CancellationToken::new())
        .await
        .unwrap()
        .deferred,
      1,
      "an uncomposed managed input cannot borrow a legacy profile"
    );
    assert!(!directory.join("requests.jsonl").exists());
    fixture.clock.0.store(2000, Ordering::SeqCst);
    let worker = fixture
      .worker()
      .with_managed_connections(ManagedIntegrationConnections::new(fixture.store.clone()));
    assert_eq!(worker.run_once(CancellationToken::new()).await.unwrap().completed, 1);
    let runs = fixture.runs().await;
    assert_eq!(runs.len(), 1);
    let before = fixture.store.factory_run_snapshot(runs[0].id).await.unwrap();
    let requests = std::fs::read(directory.join("requests.jsonl")).unwrap();
    fixture
      .store
      .publish_integration_project_access(octacity_server_store::testing::management_mutation(
        PublishIntegrationProjectAccess {
          intent: IntegrationProjectAccessIntent {
            connection_id: id,
            connection_version: IntegrationVersion::INITIAL,
            project_id: fixture.input.project_id(),
            expected_current_version: Some(IntegrationVersion::INITIAL),
            resources: vec![],
          },
          idempotency_key: IdempotencyKey::new("revoke-managed-input").unwrap(),
          published_at: time(2100),
        },
      ))
      .await
      .unwrap();
    fixture.clock.0.store(4000, Ordering::SeqCst);
    assert_eq!(worker.run_once(CancellationToken::new()).await.unwrap().deferred, 1);
    assert_eq!(std::fs::read(directory.join("requests.jsonl")).unwrap(), requests);
    let after = fixture.store.factory_run_snapshot(runs[0].id).await.unwrap();
    assert_eq!(before.work, after.work);
    assert_eq!(before.run, after.run);
    assert_eq!(before.flow, after.flow);
  }
}

#[tokio::test]
async fn source_worker_applies_retained_predicates_before_selection_and_admission() {
  for provider in ["github", "custom-tracker"] {
    let fixture = Fixture::enabled(provider).await;
    let mut input = fixture.input.clone();
    for (index, label) in ["not-present", "arbitrary"].into_iter().enumerate() {
      let mut value = serde_json::to_value(&input).unwrap();
      value["version"] = serde_json::json!(input.version().get() + 1);
      value["definition"]["predicate"] = serde_json::json!({"state":null,"all_of":[label],"any_of":[],"none_of":[]});
      let replacement: WorkSourceInput = serde_json::from_value(value).unwrap();
      fixture
        .store
        .publish_work_sources(PublishWorkSources {
          project_id: input.project_id(),
          inputs: vec![WorkSourceInputChange {
            expected_version: Some(input.version()),
            input: replacement.clone(),
          }],
          group: None,
        })
        .await
        .unwrap();
      input = replacement;
      fixture.clock.0.store(100 + (index as i64) * 2000, Ordering::SeqCst);
      assert_eq!(
        fixture
          .worker()
          .run_once(CancellationToken::new())
          .await
          .unwrap()
          .completed,
        1
      );
      assert_eq!(
        fixture.runs().await.len(),
        index,
        "nonmatching native data must never become Work"
      );
    }
  }
}
