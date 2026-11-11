//! Current values must fit the pinned Project declaration even under a broader consumer schema.
use super::*;

#[test]
fn ordinary_property_snapshot_enforces_pinned_bounds_for_required_and_optional_values() {
  run_ready(async {
    for required in [true, false] {
      for (alias, id, value) in [
        ("notes", "release_notes", IssuePropertyValue::Text("ёжx".into())),
        ("points", "cost_points", IssuePropertyValue::Number(-3)),
        ("points", "cost_points", IssuePropertyValue::Number(11)),
        (
          "environment",
          "environment",
          IssuePropertyValue::Choice("canary".into()),
        ),
      ] {
        let (store, factory, issue, mut command) = ordinary_fixture().await;
        let mut definition = store
          .issue_properties(issue.project_id, Some(1))
          .await
          .unwrap()
          .definition;
        let property = definition.properties.iter_mut().find(|p| p.id == id).unwrap();
        match &mut property.kind {
          IssuePropertyKind::Text { max_bytes } => *max_bytes = 8,
          IssuePropertyKind::Number { minimum, maximum } => {
            *minimum = -3;
            *maximum = 20;
          }
          IssuePropertyKind::Choice { choices } => choices.push(IssuePropertyChoice {
            id: "canary".into(),
            name: "Canary".into(),
            retired: false,
          }),
          _ => unreachable!(),
        }
        store
          .publish_issue_properties(management_mutation(PublishIssueProperties {
            intent: PublishIssuePropertiesIntent {
              project_id: issue.project_id,
              expected_version: 1,
              definition,
            },
            idempotency_key: key("wider-current-bounds"),
            published_at: at(34),
          }))
          .await
          .unwrap();
        let mut planning = issue.planning;
        planning.definition_version = 2;
        planning.values.insert(id.into(), value.clone());
        let issue = store
          .edit_issue_planning(management_mutation(EditIssuePlanning {
            intent: EditIssuePlanningIntent {
              project_id: issue.project_id,
              id: issue.id,
              expected_version: issue.version,
              planning,
            },
            assignees: Default::default(),
            idempotency_key: key("value-under-new-bounds"),
            edited_at: at(35),
          }))
          .await
          .unwrap()
          .issue;
        // The current Issue is valid; only the older consumer catalog disallows this value.
        assert_eq!(issue.planning.values[id], value);
        if !required {
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
        }
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
          .expect_err("a broad scalar consumer must still enforce the pinned Project bounds");
        let Some(ApplicationError::FactoryValidation(report)) = error.application() else {
          panic!("expected {alias} pinned-bound diagnostic: {error:?}")
        };
        assert_eq!(report.errors[0].field, format!("/issue_snapshot/fields/{alias}"));
        assert_eq!(report.errors[0].code, "issue_snapshot_incompatible_value");
        assert_eq!(store.mutation_evidence_counts().await, before);
        assert_eq!(factory.mutation_evidence_counts().await, factory_before);
      }
    }
  });
}
