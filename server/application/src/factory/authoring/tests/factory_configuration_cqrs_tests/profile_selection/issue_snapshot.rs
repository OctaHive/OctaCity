//! Snapshot publication uses ordinary Project definitions and explicit Incoming projections.
use super::*;
use octacity_server_store::testing::{InMemoryProjectStore, management_mutation};
#[path = "issue_snapshot_policy.rs"]
mod admission_policy;
#[path = "issue_snapshot_assets.rs"]
mod assets;
#[path = "issue_snapshot_fields.rs"]
mod fields;
#[path = "issue_snapshot_projections.rs"]
mod projections;

#[test]
fn authoring_offers_project_issue_field_contracts_without_a_classification_writer_or_body_reads() {
  run_ready(async {
    let project = ProjectId::generate();
    let fixture = selected_fixture(project);
    let properties = properties(project).await;
    let before = properties.mutation_evidence_counts().await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = Arc::new(
      crate::FactoryRoleProfileHandlers::with_policy(
        Arc::new(PublicationChoices::for_publication(fixture.choices)),
        policy.clone(),
      )
      .with_issue_properties(properties.clone()),
    );
    let result = AuthorizedQueryHandler::new(policy, handlers)
      .handle_query(
        &context(),
        crate::GetFactoryAuthoringCapabilitiesQuery { project_id: project },
      )
      .await
      .unwrap();
    let wire = serde_json::to_value(result).unwrap();
    assert_eq!(wire["issue_fields"]["definition_version"], 1);
    assert_eq!(
      wire["issue_fields"]["classification"]["properties"]["severity"],
      "impact"
    );
    assert_eq!(wire["issue_fields"]["classification"]["component"], true);
    let fields = wire["issue_fields"]["fields"]
      .as_array()
      .expect("bounded server-derived field contracts");
    let visible = fields
      .iter()
      .find(|f| f["source"] == serde_json::json!({"kind":"property","id":"customer_visible"}))
      .unwrap();
    assert_eq!(visible["name"], "Customer visible");
    assert_eq!(visible["shape"], serde_json::json!({"type":"boolean"}));
    let severity = fields
      .iter()
      .find(|f| f["source"] == serde_json::json!({"kind":"classification","slot":"severity"}))
      .unwrap();
    assert_eq!(severity["property_id"], "impact");
    assert_eq!(severity["shape"], serde_json::json!({"type":"enum","values":["major"]}));
    assert!(fields.iter().any(|f| f["source"] == serde_json::json!({"kind":"size"})
      && f["shape"] == serde_json::json!({"type":"enum","values":["small","medium","large"]})));
    assert_eq!(properties.mutation_evidence_counts().await, before);
  });
}

fn entry_index(wire: &serde_json::Value) -> usize {
  wire["root"]["nodes"]
    .as_array()
    .unwrap()
    .iter()
    .position(|node| node["key"] == wire["root"]["entry"])
    .unwrap()
}

fn snapshot_shape(wire: &mut serde_json::Value, fields: BTreeMap<FactoryKey, FlowFieldSchema>) {
  let old = wire["issue_snapshot"]["schema"].clone();
  let schema = FlowDataSchema::new(key("issue.snapshot"), key("v1"), FlowValueSchema::Object { fields }).unwrap();
  wire["issue_snapshot"]["schema"] = serde_json::to_value(schema.reference()).unwrap();
  for node in wire["root"]["nodes"].as_array_mut().unwrap() {
    for source in node["input_binding"]["sources"].as_object_mut().unwrap().values_mut() {
      if source["schema"] == old {
        source["schema"] = serde_json::to_value(schema.reference()).unwrap();
      }
    }
  }
  for declared in wire["data_schemas"].as_array_mut().unwrap() {
    if declared["reference"] == old {
      *declared = serde_json::to_value(&schema).unwrap();
    }
  }
}

async fn properties(project: ProjectId) -> Arc<InMemoryProjectStore> {
  let store = Arc::new(InMemoryProjectStore::new());
  store
    .create_project(management_mutation(CreateProject {
      id: project,
      parent_id: None,
      name: "Snapshot publication".parse().unwrap(),
      idempotency_key: idempotency("project"),
      created_at: time(0),
    }))
    .await
    .unwrap();
  store.publish_issue_properties(management_mutation(PublishIssueProperties {
    intent: PublishIssuePropertiesIntent {
      project_id: project, expected_version: 0,
      definition: serde_json::from_value(serde_json::json!({"labels":[],
        "components":[{"id":"engine","name":"Engine","retired":false}],"properties":[
        {"id":"customer_visible","name":"Customer visible","retired":false,"kind":{"type":"boolean"}},
        {"id":"notes_text","name":"Notes","retired":false,"kind":{"type":"text","max_bytes":80}},
        {"id":"target_date","name":"Target date","retired":false,"kind":{"type":"date"}},
        {"id":"cost_points","name":"Cost","retired":false,"kind":{"type":"number","minimum":-2,"maximum":10}},
        {"id":"environment","name":"Environment","retired":false,"kind":{"type":"choice","choices":[{"id":"test","name":"Test","retired":false}]}},
        {"id":"impact","name":"Impact","retired":false,"kind":{"type":"choice","choices":[{"id":"major","name":"Major","retired":false}]}}
      ],"classification":{"severity":"impact"}})).unwrap(),
    }, idempotency_key: idempotency("catalog"), published_at: time(1),
  })).await.unwrap();
  store
}

fn snapshot_draft(project: ProjectId) -> (Fixture, serde_json::Value) {
  let fixture = selected_fixture(project);
  let flow = fixture.draft.flow.as_ref().unwrap();
  let root = flow.closure.definition(flow.closure.root()).unwrap();
  let incoming = FlowDataSchema::new(
    key("issue.snapshot"),
    key("v1"),
    FlowValueSchema::Object {
      fields: [(key("visible"), FlowFieldSchema::required(FlowValueSchema::Boolean))].into(),
    },
  )
  .unwrap();
  let mut nodes = root.nodes().to_vec();
  let entry = nodes.iter().position(|node| node.key() == root.entry()).unwrap();
  nodes[entry] = nodes[entry]
    .clone()
    .with_input_binding(
      FlowInputBinding::new(
        [(
          key("issue"),
          FlowInputSource::Incoming {
            schema: incoming.reference().clone(),
            path: "/visible".into(),
          },
        )]
        .into(),
        FlowDataMapping::new(FlowValueMapping::Pointer { path: "/issue".into() }).unwrap(),
      )
      .unwrap(),
    )
    .unwrap();
  let mut schemas = flow.data_schemas.clone();
  schemas.push(incoming.clone());
  let draft = serde_json::json!({
    "root": {"input_schema":root.input_schema(),"entry":root.entry(),"nodes":nodes,
      "transitions":root.transitions(),"terminals":root.terminals(),
      "context_projections":root.context_projections(),"data_projections":root.data_projections(),
      "execution":root.execution()},
    "limits":flow.limits,"data_schemas":schemas,"pools":flow.pools,
    "issue_snapshot": {"version":1,"schema":incoming.reference(),"definition_version":1,
      "fields":{"visible":{"source":{"kind":"property","id":"customer_visible"},"required":true}},
      "materials":{},"evidence":{}}
  });
  (fixture, draft)
}

async fn publish(
  project: ProjectId,
  fixture: Fixture,
  wire: serde_json::Value,
  properties: Option<Arc<InMemoryProjectStore>>,
) -> Result<FactoryFlowConfiguration, crate::ManagementHandlerError<crate::ApplicationError>> {
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let mut handlers = crate::FactoryFlowDefinitionHandlers::new(
    Arc::new(PublicationChoices::for_publication(fixture.choices)),
    policy.clone(),
  );
  if let Some(properties) = properties {
    handlers = handlers.with_issue_properties(properties);
  }
  AuthorizedCommandHandler::new(policy, Arc::new(handlers))
    .handle_command(
      &context(),
      crate::CreateFactoryFlowDefinitionCommand {
        id: FlowDefinitionId::generate(),
        project_id: project,
        draft: serde_json::from_value(wire).expect("snapshot authoring grammar"),
        idempotency_key: idempotency("snapshot"),
        published_at: time(2),
      },
    )
    .await
    .map(|result| result.publication.flow)
}

fn assert_invalid(
  result: Result<FactoryFlowConfiguration, crate::ManagementHandlerError<crate::ApplicationError>>,
  field: &str,
) {
  let Err(crate::ManagementHandlerError::Application(crate::ApplicationError::FactoryValidation(report))) = result
  else {
    panic!("invalid snapshot must block publication at {field}");
  };
  assert!(
    report.errors.iter().any(|error| error.field == field),
    "expected {field}: {report:?}"
  );
}

#[test]
fn flow_publication_pins_an_issue_snapshot_without_a_producer_and_preserves_step_projections() {
  run_ready(async {
    let project = ProjectId::generate();
    let properties = properties(project).await;
    let (fixture, wire) = snapshot_draft(project);
    let before = properties.mutation_evidence_counts().await;
    let published = publish(project, fixture, wire.clone(), Some(properties.clone()))
      .await
      .unwrap();
    assert!(published.accepted_predecessor.is_none());
    assert_eq!(
      serde_json::from_value::<FactoryFlowConfiguration>(serde_json::to_value(&published).unwrap()).unwrap(),
      published
    );
    assert_eq!(
      serde_json::to_value(&published).unwrap()["issue_snapshot"],
      wire["issue_snapshot"]
    );
    let nodes = published.closure.definition(published.closure.root()).unwrap().nodes();
    assert_eq!(
      serde_json::to_value(nodes[0].input_binding()).unwrap(),
      wire["root"]["nodes"][0]["input_binding"]
    );
    assert_eq!(
      serde_json::to_value(nodes[1].input_binding()).unwrap(),
      wire["root"]["nodes"][1]["input_binding"]
    );
    assert_eq!(properties.mutation_evidence_counts().await, before);
  });
}

#[test]
fn flow_publication_rejects_missing_or_incompatible_issue_field_bindings() {
  run_ready(async {
    let project = ProjectId::generate();
    let properties = properties(project).await;
    for source in [
      serde_json::json!({"kind":"property","id":"missing"}),
      serde_json::json!({"kind":"classification","slot":"risk"}),
      serde_json::json!({"kind":"size"}),
    ] {
      let (fixture, mut wire) = snapshot_draft(project);
      wire["issue_snapshot"]["fields"]["visible"]["source"] = source.clone();
      assert_invalid(
        publish(project, fixture, wire, Some(properties.clone())).await,
        "/issue_snapshot/fields/visible",
      );
    }
  });
}

#[test]
fn flow_publication_rejects_unbounded_or_competing_issue_snapshot_sources() {
  run_ready(async {
    let project = ProjectId::generate();
    let properties = properties(project).await;
    for (pointer, value, field) in [
      (
        "/issue_snapshot/version",
        serde_json::json!(2),
        "/issue_snapshot/version",
      ),
      ("/issue_snapshot/fields", serde_json::json!({}), "/issue_snapshot"),
      (
        "/issue_snapshot/fields",
        serde_json::Value::Object(
          (0..65)
            .map(|i| {
              (
                format!("field{i}"),
                serde_json::json!({"source":{"kind":"property","id":"customer_visible"},"required":true}),
              )
            })
            .collect(),
        ),
        "/issue_snapshot/fields",
      ),
      (
        "/accepted_predecessor",
        serde_json::json!({
          "definition": snapshot_draft(project).0.draft.flow.as_ref().unwrap().closure.root(),
          "terminal":"succeeded","outcomes":["succeeded"]
        }),
        "/issue_snapshot",
      ),
    ] {
      let (fixture, mut wire) = snapshot_draft(project);
      wire
        .pointer_mut(pointer)
        .map(|slot| *slot = value.clone())
        .unwrap_or_else(|| {
          wire["accepted_predecessor"] = value;
        });
      assert_invalid(publish(project, fixture, wire, Some(properties.clone())).await, field);
    }
  });
}

#[test]
fn configuration_publication_reuses_snapshot_validation_and_pins_the_original_contract() {
  run_ready(async {
    let project = ProjectId::generate();
    let properties = properties(project).await;
    let (fixture, wire) = snapshot_draft(project);
    let choices = fixture.choices.clone();
    let mut draft = fixture.draft.clone();
    draft.flow = Some(publish(project, fixture, wire, Some(properties.clone())).await.unwrap());
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    store.enable_project(project, choices).unwrap();
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let commands = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(FactoryConfigurationHandlers::with_policy(store.clone(), policy).with_issue_properties(properties)),
    );
    let command = CreateFactoryConfigurationCommand {
      id: FactoryConfigurationId::generate(),
      project_id: project,
      definition_digest: digest(20),
      draft,
      idempotency_key: idempotency("snapshot-configuration"),
      published_at: time(3),
    };
    let applied = commands.handle_command(&context(), command.clone()).await.unwrap();
    assert_eq!(
      applied.configuration.configuration.flow(),
      command.draft.flow.as_ref().unwrap()
    );
    let mut invalid = command.clone();
    invalid.id = FactoryConfigurationId::generate();
    invalid.idempotency_key = idempotency("invalid-snapshot-configuration");
    invalid
      .draft
      .flow
      .as_mut()
      .unwrap()
      .issue_snapshot
      .as_mut()
      .unwrap()
      .fields
      .get_mut(&key("visible"))
      .unwrap()
      .source = FlowIssueFieldSource::Property { id: "missing".into() };
    let before = store.mutation_evidence_counts().await;
    let error = commands.handle_command(&context(), invalid).await.unwrap_err();
    assert!(matches!(
      error,
      crate::ManagementHandlerError::Application(crate::ApplicationError::FactoryValidation(_))
    ));
    assert_eq!(store.mutation_evidence_counts().await, before);
    let replay = commands.handle_command(&context(), command).await.unwrap();
    assert_eq!(replay.disposition, crate::MutationDisposition::Replayed);
    assert_eq!(replay.configuration, applied.configuration);
  });
}
