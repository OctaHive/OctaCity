//! Explicit Issue material selection becomes immutable input through owner-aware readers.
use super::*;

#[path = "issue_execution_snapshot_material_fixture.rs"]
mod material_fixture;
use material_fixture::*;
#[path = "issue_execution_snapshot_material_documents.rs"]
mod documents;

#[test]
fn issue_material_snapshot_prepares_selected_comment_text_without_implicit_discussion() {
  run_ready(async {
    let (store, factory, issue, command) = selected_comment_fixture().await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let readers = Arc::new(IssueSnapshotMaterialReaders::new(Arc::new(
      AuthorizedQueryHandler::new(
        policy.clone(),
        Arc::new(IssueDiscussionHandlers::new(
          store.clone(),
          policy.clone(),
          Arc::new(MaterialClock),
        )),
      ),
    )));
    let requests = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(
        IssueExecutionHandlers::new(store.clone(), factory.clone(), policy.clone()).with_materials(readers.clone()),
      ),
    );
    let request = requests
      .handle_command(&context(), command)
      .await
      .expect("an explicitly selected comment must be readable during manual preparation")
      .request;
    let publisher = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(
        IssueExecutionPublisher::new(
          store.clone(),
          factory.clone(),
          Arc::new(crate::management::tests::project_text_intake_tests::DocumentBytes::default()),
          Arc::new(crate::factory::admission::tests::factory_admission_tests::RecordingResolver::default()),
          policy,
          crate::factory_node_test_support::clock(),
        )
        .with_materials(readers),
      ),
    );
    let published = publisher
      .handle_command(
        &context(),
        PublishIssueExecutionCommand {
          project_id: issue.project_id,
          issue_id: issue.id,
          request_id: request.id,
          classification: crate::factory::admission::tests::factory_admission_tests::command(
            issue.project_id,
            request.intent.target.configuration_id,
            request.intent.target.repository_id,
          )
          .classification,
        },
      )
      .await
      .unwrap();
    let run = factory
      .factory_run_snapshot(published.association.unwrap().run_id)
      .await
      .unwrap();
    assert_eq!(
      run.flow.data.incoming[0].payload().value(),
      &serde_json::json!({"title":"Manual feature","context":["COMMENT_SELECTED\r\nёж\n"]})
    );
    let documents = factory
      .project_text_documents(
        issue.project_id,
        &context().security_scope().to_store(),
        &request.admission_key(),
      )
      .await
      .unwrap()
      .unwrap();
    let frozen = serde_json::to_value(documents.issue_snapshot().unwrap()).unwrap();
    assert_eq!(
      frozen["materials"]["context"][0]["selection"],
      serde_json::to_value(&issue.materials.entries()[0]).unwrap()
    );
    assert_eq!(
      frozen["materials"]["context"][0]["source"]["comment"]["body_digest"],
      serde_json::to_value(FactoryDigest::content_sha256("COMMENT_SELECTED\r\nёж\n".as_bytes())).unwrap()
    );
  });
}
