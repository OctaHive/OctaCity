//! Installing a body reader must not replace an already reviewed optional omission.
use super::*;

#[test]
fn issue_material_snapshot_keeps_a_legacy_optional_omission_when_readers_become_available() {
  run_ready(async {
    let mut f = MaterialFixture::new(true).await;
    let id = FactoryConfigurationId::generate();
    crate::factory::admission::tests::factory_admission_tests::seed_permitted_configuration(
      &f.factory,
      PublishedFactoryConfiguration {
        configuration: material_configuration(
          f.issue.project_id,
          id,
          FlowIssueMaterialSource::Selected { max_entries: 3 },
          false,
        ),
        published_at: at(46),
      },
    )
    .unwrap();
    f.command.intent.target.configuration_id = id;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let before_installation = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueExecutionHandlers::new(f.issues.clone(), f.factory.clone(), policy)),
    );
    let request = before_installation
      .handle_command(&context(), f.command.clone())
      .await
      .unwrap()
      .request;
    let published = f
      .publish(&request, f.readers(Arc::new(TrustedNetworkManagementPolicy)))
      .await
      .expect("reader installation cannot replace a reviewed optional omission");
    let run = f
      .factory
      .factory_run_snapshot(published.association.unwrap().run_id)
      .await
      .unwrap();
    assert_eq!(
      run.flow.data.incoming[0].payload().value(),
      &serde_json::json!({"title":"Manual feature"})
    );
    let documents = f
      .factory
      .project_text_documents(
        f.issue.project_id,
        &context().security_scope().to_store(),
        &request.admission_key(),
      )
      .await
      .unwrap()
      .unwrap();
    let snapshot = documents.issue_snapshot().unwrap();
    assert_eq!(snapshot.digest().unwrap(), request.snapshot_digest.unwrap());
    assert!(snapshot.materials.is_empty());
    assert_eq!(
      snapshot.omissions.get(&"context".parse().unwrap()),
      Some(&IssueSnapshotOmission::Unavailable)
    );
  });
}

#[path = "issue_execution_snapshot_material_compatibility_size.rs"]
mod encoded_size;
