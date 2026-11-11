//! A format upgrade preserves the sealed digest of an unprepared reviewed request.
use super::*;

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
    legacy.definitions = [(0, IssueSnapshotDefinitions::Legacy(catalog))].into();
    let digest = legacy.digest().unwrap();
    // Simulate a pre-upgrade reservation whose managed byte preparation has not begun.
    let retained = store
      .request_issue_execution(management_mutation(RequestIssueExecution {
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
