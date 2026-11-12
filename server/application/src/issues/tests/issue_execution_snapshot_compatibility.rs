//! A format upgrade preserves the sealed digest of an unprepared reviewed request.
use super::*;

#[test]
fn snapshot_recovers_a_legacy_optional_empty_dependency_omission() {
  run_ready(async {
    for (required, omission, compatible) in [
      (false, IssueSnapshotOmission::Inapplicable, true),
      (false, IssueSnapshotOmission::Missing, false),
      (true, IssueSnapshotOmission::Inapplicable, false),
    ] {
      let (store, factory, issue, mut command) = snapshot_fixture().await;
      let id = FactoryConfigurationId::generate();
      let string = |min_bytes, max_bytes| FlowFieldSchema::required(FlowValueSchema::String { min_bytes, max_bytes });
      let configuration = snapshot_configuration_with_schema(
        issue.project_id,
        id,
        0,
        [(
          "dependencies".parse().unwrap(),
          FlowIssueFieldSelection {
            source: FlowIssueFieldSource::Dependencies,
            required,
          },
        )]
        .into(),
        FlowValueSchema::Object {
          fields: [(
            "dependencies".parse().unwrap(),
            FlowFieldSchema::optional(FlowValueSchema::Array {
              min_items: 0,
              max_items: 64,
              item: Box::new(FlowValueSchema::Object {
                fields: [
                  ("work_id".parse().unwrap(), string(36, 36)),
                  ("run_id".parse().unwrap(), string(36, 36)),
                  ("work_digest".parse().unwrap(), string(64, 64)),
                  ("terminal".parse().unwrap(), string(1, 128)),
                ]
                .into(),
              }),
            }),
          )]
          .into(),
        },
      );
      crate::factory::admission::tests::factory_admission_tests::seed_permitted_configuration(
        &factory,
        PublishedFactoryConfiguration {
          configuration,
          published_at: at(31),
        },
      )
      .unwrap();
      command.intent.target.configuration_id = id;
      let policy = Arc::new(TrustedNetworkManagementPolicy);
      let request = AuthorizedCommandHandler::new(
        policy.clone(),
        Arc::new(IssueExecutionHandlers::new(store.clone(), factory.clone(), policy)),
      )
      .handle_command(&context(), command)
      .await
      .unwrap()
      .request;
      publish_snapshot_request(store.clone(), factory.clone(), &request).await;
      let documents = factory
        .project_text_documents(
          issue.project_id,
          &context().security_scope().to_store(),
          &request.admission_key(),
        )
        .await
        .unwrap()
        .unwrap();
      let mut legacy = documents.issue_snapshot().unwrap().clone();
      // Before 5.3, an empty optional dependency list had no assessment and was omitted.
      legacy.values.remove(&"dependencies".parse().unwrap());
      legacy.omissions.insert("dependencies".parse().unwrap(), omission);
      let digest = legacy.digest().unwrap();
      let retained = store
        .request_issue_execution(management_mutation(RequestIssueExecution {
          id: octacity_server_domain::IssueExecutionRequestId::generate(),
          snapshot_digest: Some(digest),
          admission_classification: None,
          intent: request.intent.clone(),
          idempotency_key: key("legacy-empty-dependencies"),
          requested_at: at(100),
        }))
        .await
        .unwrap()
        .request;
      let before = factory.mutation_evidence_counts().await;
      let policy = Arc::new(TrustedNetworkManagementPolicy);
      let result = AuthorizedCommandHandler::new(
        policy.clone(),
        Arc::new(IssueExecutionPublisher::new(
          store.clone(),
          factory.clone(),
          Arc::new(crate::management::tests::project_text_intake_tests::DocumentBytes::default()),
          Arc::new(crate::factory::admission::tests::factory_admission_tests::RecordingResolver::default()),
          policy,
          crate::factory_node_test_support::clock(),
        )),
      )
      .handle_command(
        &context(),
        PublishIssueExecutionCommand {
          project_id: issue.project_id,
          issue_id: issue.id,
          request_id: retained.id,
          classification: crate::factory::admission::tests::factory_admission_tests::command(
            issue.project_id,
            id,
            retained.intent.target.repository_id,
          )
          .classification,
        },
      )
      .await;
      if !compatible {
        let error = result.expect_err("a required field or unrecognized omission cannot be reconstructed");
        assert!(matches!(
          error.application(),
          Some(ApplicationError::Store(StoreError::Conflict {
            entity: octacity_server_domain::EntityKind::Issue
          }))
        ));
        assert_eq!(factory.mutation_evidence_counts().await, before);
        continue;
      }
      let published = result.expect("the exact legacy omission remains recoverable");
      assert_eq!(published.snapshot_digest, Some(digest));
      let frozen = factory
        .project_text_documents(
          issue.project_id,
          &context().security_scope().to_store(),
          &retained.admission_key(),
        )
        .await
        .unwrap()
        .unwrap();
      assert_eq!(frozen.issue_snapshot().unwrap(), &legacy);
      let run = factory
        .factory_run_snapshot(published.association.as_ref().unwrap().run_id)
        .await
        .unwrap();
      assert_eq!(run.flow.data.incoming[0].payload().value(), &serde_json::json!({}));
      assert_eq!(run.flow.data.incoming[0].issue_snapshot().unwrap().digest, digest);
      assert_eq!(publish_snapshot_request(store, factory, &retained).await, published);
    }
  });
}

#[test]
fn snapshot_publishes_a_retained_full_catalog_request_without_changing_its_digest() {
  run_ready(async {
    let (store, factory, issue, command) = snapshot_fixture().await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let requests = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueExecutionHandlers::new(store.clone(), factory.clone(), policy)),
    );
    let original = requests.handle_command(&context(), command).await.unwrap().request;
    publish_snapshot_request(store.clone(), factory.clone(), &original).await;
    let documents = factory
      .project_text_documents(
        issue.project_id,
        &context().security_scope().to_store(),
        &original.admission_key(),
      )
      .await
      .unwrap()
      .unwrap();
    let mut legacy: IssueExecutionSnapshot =
      serde_json::from_slice(&serde_json::to_vec(documents.issue_snapshot().unwrap()).unwrap()).unwrap();
    let catalog = store.issue_properties(issue.project_id, Some(0)).await.unwrap();
    // The retained full-catalog wire predates the reviewed catalog-head fence.
    legacy.definition_head = None;
    legacy.definitions = [(0, IssueSnapshotDefinitions::Legacy(catalog))].into();
    let digest = legacy.digest().unwrap();
    // Simulate a pre-upgrade reservation whose managed byte preparation has not begun.
    let retained = store
      .request_issue_execution(management_mutation(RequestIssueExecution {
        admission_classification: None,
        id: octacity_server_domain::IssueExecutionRequestId::generate(),
        snapshot_digest: Some(digest),
        intent: original.intent.clone(),
        idempotency_key: key("legacy-unprepared-snapshot"),
        requested_at: at(100),
      }))
      .await
      .unwrap()
      .request;
    assert!(
      factory
        .project_text_preparation(
          issue.project_id,
          &context().security_scope().to_store(),
          &retained.admission_key()
        )
        .await
        .unwrap()
        .is_none()
    );
    let published = publish_snapshot_request(store.clone(), factory.clone(), &retained).await;
    assert_eq!(published.snapshot_digest, Some(digest));
    let documents = factory
      .project_text_documents(
        issue.project_id,
        &context().security_scope().to_store(),
        &retained.admission_key(),
      )
      .await
      .unwrap()
      .unwrap();
    let restored: ProjectTextDocuments = serde_json::from_slice(&serde_json::to_vec(&documents).unwrap()).unwrap();
    assert_eq!(restored.snapshot_digest().unwrap(), Some(digest));
    assert!(matches!(
      restored.issue_snapshot().unwrap().definitions[&0],
      IssueSnapshotDefinitions::Legacy(_)
    ));
    let run = factory
      .factory_run_snapshot(published.association.as_ref().unwrap().run_id)
      .await
      .unwrap();
    assert_eq!(run.flow.data.incoming[0].issue_snapshot().unwrap().digest, digest);
    let replay = publish_snapshot_request(store, factory, &retained).await;
    assert_eq!(replay, published);
  });
}
