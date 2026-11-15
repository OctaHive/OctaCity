//! One selection authorizes the ordinary result form and its automatic local effect.
use super::*;
use octacity_server_store::testing as store_testing;
use octacity_server_store::*;

#[test]
fn ordinary_agent_publishes_one_project_form_without_an_apply_node_and_rejects_another_form() {
  run_ready(async {
    let fixture = store_testing::factory_issue_field_journal_fixture();
    let project = fixture.admission.work.subject().project_id();
    let definition = &fixture.admission.flow.closure().definitions()[0];
    let node = &definition.nodes()[0];
    let selected = node.outcomes()[0].issue_fields().unwrap();
    let build = node.build().unwrap();
    let writer_alias = key("writer");
    let mut references = vec![FactoryReferenceChoice {
      kind: FactoryChoiceKind::NodeCapability,
      alias: writer_alias.clone(),
      reference: selected.writer.clone(),
    }];
    for (name, reference) in [
      ("tool", build.tool.clone()),
      ("plugin", build.plugin.clone()),
      ("model", build.model_or_tool.clone()),
    ] {
      references.push(FactoryReferenceChoice {
        kind: FactoryChoiceKind::NodeCapability,
        alias: key(name),
        reference,
      });
    }
    let choices = FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
      references,
      build_configurations: vec![(key("build"), build.build_configuration.clone())],
    })
    .unwrap()
    .with_choice_metadata(
      FactoryChoiceKind::NodeCapability,
      writer_alias,
      serde_json::from_value(serde_json::json!({
        "node_kinds":["trusted_action"],"signal_purposes":[],"enforcement":[],"compatible_choices":[],
        "issue_classification":{"policy":selected.policy,"size_labels":{}}
      }))
      .unwrap(),
    )
    .unwrap();
    let properties = Arc::new(store_testing::InMemoryProjectStore::new());
    properties
      .create_project(store_testing::management_mutation(CreateProject {
        id: project,
        parent_id: None,
        name: octacity_server_domain::ProjectName::new("Form").unwrap(),
        idempotency_key: "form-project".parse().unwrap(),
        created_at: time(1),
      }))
      .await
      .unwrap();
    let catalog = properties.issue_properties(project, None).await.unwrap();
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let queries = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(
        crate::FactoryFlowDefinitionHandlers::new(
          Arc::new(super::super::profile_selection::PublicationChoices::for_validation(
            choices,
          )),
          policy,
        )
        .with_issue_properties(properties),
      ),
    );
    let mut root = serde_json::to_value(definition).unwrap();
    root.as_object_mut().unwrap().remove("reference");
    let draft:crate::FactoryFlowDefinitionDraft=serde_json::from_value(serde_json::json!({"root":root,"limits":fixture.admission.flow.limits(),"data_schemas":fixture.admission.flow.data_schemas()})).unwrap();
    let query = |draft| crate::ValidateFactoryFlowDefinitionQuery {
      id: definition.reference().id(),
      project_id: project,
      draft,
    };
    let published = queries.handle_query(&context(), query(draft.clone())).await.unwrap();
    let root = published.closure.definition(published.closure.root()).unwrap();
    assert_eq!(root.nodes().len(), 1);
    assert!(
      root.nodes()[0].action().is_none(),
      "the operator authors only an ordinary agent step"
    );
    assert_eq!(
      published
        .data_schemas
        .iter()
        .find(|s| s.reference() == root.nodes()[0].outcomes()[0].schema())
        .unwrap()
        .shape(),
      &selected.form.form_schema(&catalog).unwrap()
    );
    let mut wrong = serde_json::to_value(draft).unwrap();
    wrong["root"]["nodes"][0]["outcomes"][0]["issue_fields"]["form"]["fields"][0]["field"]["kind"] =
      serde_json::json!("size");
    assert!(
      queries
        .handle_query(&context(), query(serde_json::from_value(wrong).unwrap()))
        .await
        .is_err(),
      "another selected form cannot reuse the accepted priority contract"
    );
  });
}
