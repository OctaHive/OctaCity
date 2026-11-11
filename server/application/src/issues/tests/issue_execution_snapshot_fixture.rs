use super::*;

pub(super) async fn snapshot_fixture() -> Fixture {
  let (store, factory, issue, mut request) = fixture_with_content(IssueContent {
    title: "Manual feature".into(),
    description: "Original requirements".into(),
    acceptance: Some("Original expected behavior".into()),
    priority: Some(IssuePriority::Choice("high".into())),
  })
  .await;
  let issue = store
    .edit_issue_planning(management_mutation(EditIssuePlanning {
      intent: EditIssuePlanningIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: issue.version,
        planning: IssuePlanning {
          size: Some(IssueSize::Small),
          ..issue.planning
        },
      },
      assignees: Default::default(),
      idempotency_key: key("manual-size"),
      edited_at: at(30),
    }))
    .await
    .unwrap()
    .issue;
  let id = FactoryConfigurationId::generate();
  let configuration = snapshot_configuration(issue.project_id, id);
  crate::factory::admission::tests::factory_admission_tests::seed_permitted_configuration(
    &factory,
    PublishedFactoryConfiguration {
      configuration,
      published_at: at(31),
    },
  )
  .unwrap();
  request.intent.target.configuration_id = id;
  request.intent.expected_version = issue.version;
  request.requested_at = at(40);
  (store, factory, issue, request)
}

fn snapshot_configuration(project: ProjectId, id: FactoryConfigurationId) -> FactoryConfiguration {
  snapshot_configuration_with_component(project, id, false)
}
pub(super) fn snapshot_configuration_with_component(
  project: ProjectId,
  id: FactoryConfigurationId,
  component: bool,
) -> FactoryConfiguration {
  let mut fields: std::collections::BTreeMap<_, _> = [
    ("title", FlowIssueFieldSource::Title),
    ("size", FlowIssueFieldSource::Size),
    ("priority", FlowIssueFieldSource::Priority),
  ]
  .into_iter()
  .map(|(alias, source)| {
    (
      alias.parse().unwrap(),
      FlowIssueFieldSelection { source, required: true },
    )
  })
  .collect();
  if component {
    fields.insert(
      "component".parse().unwrap(),
      FlowIssueFieldSelection {
        source: FlowIssueFieldSource::Component,
        required: false,
      },
    );
  }
  snapshot_configuration_with_text_fields(project, id, 0, fields)
}

/// Consumer strings deliberately accept the Project's own finite choice vocabulary.
pub(super) fn snapshot_configuration_with_text_fields(
  project: ProjectId,
  id: FactoryConfigurationId,
  definition_version: u64,
  fields: std::collections::BTreeMap<FactoryKey, FlowIssueFieldSelection>,
) -> FactoryConfiguration {
  let shape = FlowValueSchema::Object {
    fields: fields
      .iter()
      .map(|(alias, selection)| {
        let text = FlowValueSchema::String {
          min_bytes: 1,
          max_bytes: 256,
        };
        (
          alias.clone(),
          if selection.required {
            FlowFieldSchema::required(text)
          } else {
            FlowFieldSchema::optional(text)
          },
        )
      })
      .collect(),
  };
  snapshot_configuration_with_schema(project, id, definition_version, fields, shape)
}

/// Uses ordinary scalar consumer schemas; Project catalog bounds remain authoritative.
pub(super) fn snapshot_configuration_with_schema(
  project: ProjectId,
  id: FactoryConfigurationId,
  definition_version: u64,
  fields: std::collections::BTreeMap<FactoryKey, FlowIssueFieldSelection>,
  shape: FlowValueSchema,
) -> FactoryConfiguration {
  use crate::factory::admission::tests::factory_admission_tests::{configuration, configuration_with_flow};
  let ordinary = configuration(project, id);
  let original = ordinary.flow();
  let root = original.closure.definition(original.closure.root()).unwrap();
  let schema = FlowDataSchema::new("issue.snapshot".parse().unwrap(), "v1".parse().unwrap(), shape).unwrap();
  let mut nodes = root.nodes().to_vec();
  let index = nodes.iter().position(|node| node.key() == root.entry()).unwrap();
  nodes[index] = nodes[index]
    .clone()
    .with_input_binding(
      FlowInputBinding::new(
        [(
          "issue".parse().unwrap(),
          FlowInputSource::Incoming {
            schema: schema.reference().clone(),
            path: String::new(),
          },
        )]
        .into(),
        FlowDataMapping::new(FlowValueMapping::Constant {
          value: serde_json::json!(true),
        })
        .unwrap(),
      )
      .unwrap(),
    )
    .unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: root.reference().id(),
    version: root.reference().version(),
    input_schema: root.input_schema().cloned(),
    entry: root.entry().clone(),
    nodes,
    transitions: root.transitions().to_vec(),
    terminals: root.terminals().to_vec(),
    context_projections: root.context_projections().to_vec(),
    data_projections: root.data_projections().to_vec(),
    execution: root.execution().clone(),
  })
  .unwrap();
  let mut schemas = original.data_schemas.clone();
  schemas.push(schema.clone());
  let flow = FactoryFlowConfiguration::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    original.limits.clone(),
    schemas,
    original.pools.clone(),
  )
  .unwrap()
  .with_issue_snapshot(FlowIssueSnapshotInput {
    admission: None,
    version: 1,
    schema: schema.reference().clone(),
    definition_version,
    fields,
    materials: Default::default(),
    evidence: Default::default(),
  })
  .unwrap();
  configuration_with_flow(project, id, Some(flow))
}

pub(super) async fn publish_snapshot_request(
  issues: Arc<octacity_server_store::testing::InMemoryProjectStore>,
  factory: Arc<octacity_server_store::testing::InMemoryFactoryConfigurationStore>,
  request: &IssueExecutionRequest,
) -> IssueExecutionRequest {
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let publisher = AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(IssueExecutionPublisher::new(
      issues,
      factory,
      Arc::new(crate::management::tests::project_text_intake_tests::DocumentBytes::default()),
      Arc::new(crate::factory::admission::tests::factory_admission_tests::RecordingResolver::default()),
      policy,
      crate::factory_node_test_support::clock(),
    )),
  );
  let classification = crate::factory::admission::tests::factory_admission_tests::command(
    request.intent.project_id,
    request.intent.target.configuration_id,
    request.intent.target.repository_id,
  )
  .classification;
  publisher
    .handle_command(
      &context(),
      PublishIssueExecutionCommand {
        project_id: request.intent.project_id,
        issue_id: request.intent.issue_id,
        request_id: request.id,
        classification,
      },
    )
    .await
    .expect("a qualified snapshot publishes through ordinary intake")
}
