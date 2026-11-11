//! Absence is explicit; false, zero and empty text remain present native values.
use super::*;

#[test]
fn ordinary_property_snapshot_reports_each_missing_required_scalar_without_reservation() {
  run_ready(async {
    for (alias, id) in FIELDS.into_iter().filter(|(alias, _)| *alias != "optional_context") {
      let (store, factory, issue, mut command) = ordinary_fixture().await;
      let mut planning = issue.planning;
      planning.values.remove(id);
      let issue = store
        .edit_issue_planning(management_mutation(EditIssuePlanning {
          intent: EditIssuePlanningIntent {
            project_id: issue.project_id,
            id: issue.id,
            expected_version: issue.version,
            planning,
          },
          assignees: Default::default(),
          idempotency_key: key("missing-ordinary-value"),
          edited_at: at(35),
        }))
        .await
        .unwrap()
        .issue;
      command.intent.expected_version = issue.version;
      let policy = Arc::new(TrustedNetworkManagementPolicy);
      let requests = AuthorizedCommandHandler::new(
        policy.clone(),
        Arc::new(IssueExecutionHandlers::new(store.clone(), factory.clone(), policy)),
      );
      let before = store.mutation_evidence_counts().await;
      let factory_before = factory.mutation_evidence_counts().await;
      let error = requests
        .handle_command(&context(), command)
        .await
        .expect_err("a required ordinary scalar must be present");
      let Some(ApplicationError::FactoryValidation(report)) = error.application() else {
        panic!("expected {alias} missing-value diagnostic: {error:?}")
      };
      assert_eq!(report.errors[0].field, format!("/issue_snapshot/fields/{alias}"));
      assert_eq!(report.errors[0].code, "issue_snapshot_missing_value");
      assert_eq!(store.mutation_evidence_counts().await, before);
      assert_eq!(factory.mutation_evidence_counts().await, factory_before);
    }
  });
}

#[test]
fn ordinary_property_snapshot_omits_missing_optional_scalars_without_inventing_defaults() {
  run_ready(async {
    let (store, factory, issue, mut command) = ordinary_fixture().await;
    let issue = store
      .edit_issue_planning(management_mutation(EditIssuePlanning {
        intent: EditIssuePlanningIntent {
          project_id: issue.project_id,
          id: issue.id,
          expected_version: issue.version,
          planning: IssuePlanning {
            values: Default::default(),
            ..issue.planning
          },
        },
        assignees: Default::default(),
        idempotency_key: key("unset-optional-values"),
        edited_at: at(35),
      }))
      .await
      .unwrap()
      .issue;
    let id = FactoryConfigurationId::generate();
    crate::factory::admission::tests::factory_admission_tests::seed_permitted_configuration(
      &factory,
      PublishedFactoryConfiguration {
        configuration: ordinary_configuration(issue.project_id, id, false),
        published_at: at(36),
      },
    )
    .unwrap();
    command.intent.target.configuration_id = id;
    command.intent.expected_version = issue.version;
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
    assert_eq!(run.flow.data.incoming[0].payload().value(), &serde_json::json!({}));
    let documents = factory
      .project_text_documents(
        issue.project_id,
        &context().security_scope().to_store(),
        &request.admission_key(),
      )
      .await
      .unwrap()
      .unwrap();
    let snapshot = documents.issue_snapshot().unwrap();
    assert_eq!(snapshot.omissions.len(), 6);
    for (alias, _) in FIELDS {
      assert_eq!(
        snapshot.omissions.get(&alias.parse().unwrap()),
        Some(&IssueSnapshotOmission::Missing)
      );
    }
    assert!(snapshot.attribution.is_empty());
  });
}

#[test]
fn ordinary_property_snapshot_preserves_present_empty_text_and_zero() {
  run_ready(async {
    let (store, factory, issue, mut command) = ordinary_fixture().await;
    let mut planning = issue.planning;
    planning
      .values
      .insert("release_notes".into(), IssuePropertyValue::Text(String::new()));
    planning
      .values
      .insert("cost_points".into(), IssuePropertyValue::Number(0));
    let issue = store
      .edit_issue_planning(management_mutation(EditIssuePlanning {
        intent: EditIssuePlanningIntent {
          project_id: issue.project_id,
          id: issue.id,
          expected_version: issue.version,
          planning,
        },
        assignees: Default::default(),
        idempotency_key: key("present-empty-values"),
        edited_at: at(35),
      }))
      .await
      .unwrap()
      .issue;
    command.intent.expected_version = issue.version;
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
    assert_eq!(
      run.flow.data.incoming[0].payload().value(),
      &serde_json::json!({
        "notes":"","visible":false,"points":0,"date":"2028-02-29","environment":"test"
      })
    );
  });
}
