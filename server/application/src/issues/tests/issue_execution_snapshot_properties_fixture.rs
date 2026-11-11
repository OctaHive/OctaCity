//! Published scalar definitions and ordinary Issue edits use the existing public store contract.
use super::*;

pub(super) const FIELDS: [(&str, &str); 6] = [
  ("notes", "release_notes"),
  ("visible", "customer_visible"),
  ("points", "cost_points"),
  ("date", "target_date"),
  ("environment", "environment"),
  ("optional_context", "optional_context"),
];

pub(super) async fn ordinary_fixture() -> Fixture {
  let (store, factory, issue, mut command) = fixture_with_content(IssueContent {
    title: "Ordinary manual Issue".into(),
    description: "Original requirements".into(),
    acceptance: Some("Original expected behavior".into()),
    priority: None,
  })
  .await;
  let mut definition = store
    .issue_properties(issue.project_id, Some(0))
    .await
    .unwrap()
    .definition;
  definition.properties = serde_json::from_value(serde_json::json!([
    {"id":"release_notes","name":"Release notes","retired":false,"kind":{"type":"text","max_bytes":4}},
    {"id":"customer_visible","name":"Customer visible","retired":false,"kind":{"type":"boolean"}},
    {"id":"cost_points","name":"Cost","retired":false,"kind":{"type":"number","minimum":-2,"maximum":10}},
    {"id":"target_date","name":"Target date","retired":false,"kind":{"type":"date"}},
    {"id":"environment","name":"Environment","retired":false,"kind":{"type":"choice","choices":[
      {"id":"test","name":"Test","retired":false},{"id":"prod","name":"Production","retired":false}]}},
    {"id":"optional_context","name":"Optional context","retired":false,"kind":{"type":"text","max_bytes":12}},
    {"id":"unselected_note","name":"Unselected note","retired":false,"kind":{"type":"text","max_bytes":128}}
  ]))
  .unwrap();
  store
    .publish_issue_properties(management_mutation(PublishIssueProperties {
      intent: PublishIssuePropertiesIntent {
        project_id: issue.project_id,
        expected_version: 0,
        definition,
      },
      idempotency_key: key("ordinary-property-catalog"),
      published_at: at(31),
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
          definition_version: 1,
          values: [
            ("release_notes".into(), IssuePropertyValue::Text("ёж".into())),
            ("customer_visible".into(), IssuePropertyValue::Boolean(false)),
            ("cost_points".into(), IssuePropertyValue::Number(-2)),
            ("target_date".into(), IssuePropertyValue::Date("2028-02-29".into())),
            ("environment".into(), IssuePropertyValue::Choice("test".into())),
            (
              "unselected_note".into(),
              IssuePropertyValue::Text("UNSELECTED_ORDINARY_VALUE".into()),
            ),
          ]
          .into(),
          ..issue.planning
        },
      },
      assignees: Default::default(),
      idempotency_key: key("ordinary-property-values"),
      edited_at: at(32),
    }))
    .await
    .unwrap()
    .issue;
  let id = FactoryConfigurationId::generate();
  crate::factory::admission::tests::factory_admission_tests::seed_permitted_configuration(
    &factory,
    PublishedFactoryConfiguration {
      configuration: ordinary_configuration(issue.project_id, id, true),
      published_at: at(33),
    },
  )
  .unwrap();
  command.intent.target.configuration_id = id;
  command.intent.expected_version = issue.version;
  command.requested_at = at(40);
  (store, factory, issue, command)
}

pub(super) fn ordinary_configuration(
  project: ProjectId,
  id: FactoryConfigurationId,
  required: bool,
) -> FactoryConfiguration {
  let fields = FIELDS
    .into_iter()
    .map(|(alias, id)| {
      (
        alias.parse().unwrap(),
        FlowIssueFieldSelection {
          source: FlowIssueFieldSource::Property { id: id.into() },
          required: required && alias != "optional_context",
        },
      )
    })
    .collect();
  // Broad scalar consumers do not copy the Project's choice vocabulary or bounds.
  let shape = FlowValueSchema::Object {
    fields: FIELDS
      .into_iter()
      .map(|(alias, _)| {
        let value = match alias {
          "visible" => FlowValueSchema::Boolean,
          "points" => FlowValueSchema::Integer {
            minimum: -10,
            maximum: 100,
          },
          _ => FlowValueSchema::String {
            min_bytes: 0,
            max_bytes: 512,
          },
        };
        let field = if required && alias != "optional_context" {
          FlowFieldSchema::required(value)
        } else {
          FlowFieldSchema::optional(value)
        };
        (alias.parse().unwrap(), field)
      })
      .collect(),
  };
  snapshot_configuration_with_schema(project, id, 1, fields, shape)
}
