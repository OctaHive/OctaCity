//! Absence and stale assessment are distinct operator-visible states.
use super::*;

#[test]
fn snapshot_reports_an_unset_required_size_as_missing() {
  run_ready(async {
    let (store, factory, issue, mut command) = snapshot_fixture().await;
    let issue = store
      .edit_issue_planning(management_mutation(EditIssuePlanning {
        intent: EditIssuePlanningIntent {
          project_id: issue.project_id,
          id: issue.id,
          expected_version: issue.version,
          planning: IssuePlanning {
            size: None,
            ..issue.planning
          },
        },
        assignees: Default::default(),
        idempotency_key: key("unset-size"),
        edited_at: at(35),
      }))
      .await
      .unwrap()
      .issue;
    command.intent.expected_version = issue.version;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let requests = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueExecutionHandlers::new(store.clone(), factory, policy)),
    );
    let before = store.mutation_evidence_counts().await;
    let error = requests
      .handle_command(&context(), command)
      .await
      .expect_err("missing required input blocks reservation");
    let Some(ApplicationError::FactoryValidation(report)) = error.application() else {
      panic!("expected field diagnostics: {error:?}");
    };
    assert_eq!(report.errors[0].field, "/issue_snapshot/fields/size");
    assert_eq!(report.errors[0].code, "issue_snapshot_missing_value");
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

#[test]
fn snapshot_retains_an_unset_optional_component_as_missing() {
  run_ready(async {
    let (store, factory, issue, mut command) = snapshot_fixture().await;
    let id = FactoryConfigurationId::generate();
    crate::factory::admission::tests::factory_admission_tests::seed_permitted_configuration(
      &factory,
      PublishedFactoryConfiguration {
        configuration: snapshot_configuration_with_component(issue.project_id, id, true),
        published_at: at(35),
      },
    )
    .unwrap();
    command.intent.target.configuration_id = id;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let requests = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueExecutionHandlers::new(store.clone(), factory.clone(), policy)),
    );
    let request = requests.handle_command(&context(), command).await.unwrap().request;
    let published = publish_snapshot_request(store, factory.clone(), &request).await;
    let run = factory
      .factory_run_snapshot(published.association.unwrap().run_id)
      .await
      .unwrap();
    assert!(
      !run.flow.data.incoming[0]
        .payload()
        .value()
        .as_object()
        .unwrap()
        .contains_key("component")
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
    assert_eq!(
      documents
        .issue_snapshot()
        .unwrap()
        .omissions
        .get(&"component".parse().unwrap()),
      Some(&IssueSnapshotOmission::Missing)
    );
  });
}
