//! A consumer cannot reuse a property's value under another assessment meaning.
use super::*;

#[test]
fn snapshot_rejects_reproduction_reinterpreted_from_a_new_risk_binding() {
  run_ready(async {
    let (store, factory, _, command) = property_snapshot_fixture(true, "reproduced").await;
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
      .expect_err("risk cannot establish reproduction in an older consumer");
    let Some(ApplicationError::FactoryValidation(report)) = error.application() else {
      panic!("expected field diagnostics: {error:?}");
    };
    assert_eq!(report.errors[0].field, "/issue_snapshot/fields/reproduction");
    assert_eq!(report.errors[0].code, "issue_snapshot_semantic_binding_changed");
    assert_eq!(store.mutation_evidence_counts().await, before);
    assert_eq!(factory.mutation_evidence_counts().await, factory_before);
  });
}

/// Restores the same semantics in a newer catalog or deliberately rebinds them.
async fn property_snapshot_fixture(rebind: bool, value: &str) -> Fixture {
  let (store, factory, issue, mut command) = snapshot_fixture().await;
  let mut definition = store
    .issue_properties(issue.project_id, Some(0))
    .await
    .unwrap()
    .definition;
  definition.properties = ["assessment_a", "assessment_b"]
    .into_iter()
    .map(|id| IssuePropertyDefinition {
      id: id.into(),
      name: id.into(),
      retired: false,
      kind: IssuePropertyKind::Choice {
        choices: ["unknown", "reproduced"]
          .into_iter()
          .map(|id| IssuePropertyChoice {
            id: id.into(),
            name: id.into(),
            retired: false,
          })
          .collect(),
      },
    })
    .collect();
  definition.classification.reproducibility = Some("assessment_a".into());
  definition.classification.risk = Some("assessment_b".into());
  store
    .publish_issue_properties(management_mutation(PublishIssueProperties {
      intent: PublishIssuePropertiesIntent {
        project_id: issue.project_id,
        expected_version: 0,
        definition: definition.clone(),
      },
      idempotency_key: key("original-meaning"),
      published_at: at(31),
    }))
    .await
    .unwrap();
  if rebind {
    definition.classification.reproducibility = Some("assessment_b".into());
    definition.classification.risk = Some("assessment_a".into());
  } else {
    definition.properties[0].name = "Renamed assessment".into();
  }
  store
    .publish_issue_properties(management_mutation(PublishIssueProperties {
      intent: PublishIssuePropertiesIntent {
        project_id: issue.project_id,
        expected_version: 1,
        definition,
      },
      idempotency_key: key("new-meaning"),
      published_at: at(32),
    }))
    .await
    .unwrap();
  let issue = store
    .edit_issue_planning(management_mutation(EditIssuePlanning {
      intent: EditIssuePlanningIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: issue.version,
        planning: IssuePlanning {
          definition_version: 2,
          values: [("assessment_a".into(), IssuePropertyValue::Choice(value.into()))].into(),
          ..issue.planning
        },
      },
      assignees: Default::default(),
      idempotency_key: key("manual-risk"),
      edited_at: at(33),
    }))
    .await
    .unwrap()
    .issue;
  assert!(issue.classification.evidence.is_empty());
  let id = FactoryConfigurationId::generate();
  let configuration = snapshot_configuration_with_text_fields(
    issue.project_id,
    id,
    1,
    [(
      "reproduction".parse().unwrap(),
      FlowIssueFieldSelection {
        source: FlowIssueFieldSource::Classification {
          slot: "reproducibility".parse().unwrap(),
        },
        required: true,
      },
    )]
    .into(),
  );
  crate::factory::admission::tests::factory_admission_tests::seed_permitted_configuration(
    &factory,
    PublishedFactoryConfiguration {
      configuration,
      published_at: at(34),
    },
  )
  .unwrap();
  command.intent.target.configuration_id = id;
  command.intent.expected_version = issue.version;
  (store, factory, issue, command)
}

#[test]
fn snapshot_keeps_compatible_assessment_meaning_across_catalog_versions() {
  run_ready(async {
    let (store, factory, _, command) = property_snapshot_fixture(false, "unknown").await;
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
      &serde_json::json!({"reproduction":"unknown"})
    );
    let documents = factory
      .project_text_documents(
        request.intent.project_id,
        &context().security_scope().to_store(),
        &request.admission_key(),
      )
      .await
      .unwrap()
      .unwrap();
    let snapshot = documents.issue_snapshot().unwrap();
    let definition_name = |version| {
      let IssueSnapshotDefinitions::SelectedV1 { properties, .. } = &snapshot.definitions[&version] else {
        panic!("new snapshots retain selected definitions");
      };
      properties[0].definition.name.clone()
    };
    assert_eq!(definition_name(1), "assessment_a");
    assert_eq!(definition_name(2), "Renamed assessment");
    assert_eq!(
      snapshot.attribution[&"reproduction".parse().unwrap()].field,
      IssueClassificationFieldReference::Property {
        id: "assessment_a".into(),
        definition_version: 2
      }
    );
  });
}
