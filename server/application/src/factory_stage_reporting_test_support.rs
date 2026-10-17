//! Immutable graph fixtures shared by stage-local reporting contract tests.
use super::*;
use std::collections::BTreeMap;

fn value_schema() -> FlowDataSchema {
  FlowDataSchema::new(key("stage.value"), key("v1"), FlowValueSchema::Boolean).unwrap()
}
fn receipt_schema() -> FlowDataSchema {
  FlowDataSchema::new(
    key("stage.receipt"),
    key("v1"),
    FlowValueSchema::Object {
      fields: BTreeMap::from([(
        key("receipt"),
        FlowFieldSchema::required(FlowValueSchema::String {
          min_bytes: 64,
          max_bytes: 64,
        }),
      )]),
    },
  )
  .unwrap()
}
fn constant_input() -> FlowInputBinding {
  FlowInputBinding::new(
    BTreeMap::new(),
    FlowDataMapping::new(FlowValueMapping::Constant {
      value: serde_json::json!(true),
    })
    .unwrap(),
  )
  .unwrap()
}
fn gate(name: &str, outcomes: Vec<FlowOutcomeDefinition>, selected: &str) -> FlowNodeDefinition {
  FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key(name),
    kind: FlowNodeKind::DeterministicGate,
    input_schema: Some(value_schema().reference().clone()),
    outcomes,
    budget: BudgetLimit::new(1, 1000, 0, 0, 1000).unwrap(),
    permissions: FactoryPermissionSet::deny_all(),
    required: true,
    subflow: None,
  })
  .unwrap()
  .with_input_binding(constant_input())
  .unwrap()
  .with_gate(
    FlowGateProgram::new(vec![], key(selected))
      .unwrap()
      .with_default_output_mapping(
        FlowDataMapping::new(FlowValueMapping::Constant {
          value: serde_json::json!(true),
        })
        .unwrap(),
      )
      .unwrap()
      .binding()
      .unwrap(),
  )
  .unwrap()
}
pub(super) fn reported() -> FlowOutcomeDefinition {
  FlowOutcomeDefinition::new(
    key("queued"),
    FlowOutcomeKind::Success,
    receipt_schema().reference().clone(),
  )
}
pub(super) const ROUTES: [(&str, FlowOutcomeKind, &str); 3] = [
  ("usable", FlowOutcomeKind::Success, "ready / review"),
  ("ask-human", FlowOutcomeKind::Success, "context requested"),
  ("not-usable", FlowOutcomeKind::Failure, "repair needed"),
];
pub(super) fn stage_definition(selected: &str) -> FlowDefinition {
  let mut nodes = vec![gate(
    "inspect-ticket",
    ROUTES
      .iter()
      .map(|(name, kind, _)| FlowOutcomeDefinition::new(key(name), *kind, value_schema().reference().clone()))
      .chain(std::iter::once(FlowOutcomeDefinition::new(
        key("approved"),
        FlowOutcomeKind::Success,
        value_schema().reference().clone(),
      )))
      .collect(),
    selected,
  )];
  let mut transitions = vec![];
  for (route, _, status) in ROUTES {
    let action = format!("notify-{route}");
    nodes.push(
      FlowNodeDefinition::new(FlowNodeDefinitionInput {
        key: key(&action),
        kind: FlowNodeKind::TrustedAction,
        input_schema: Some(value_schema().reference().clone()),
        outcomes: vec![reported()],
        budget: BudgetLimit::new(1, 1000, 0, 0, 1000).unwrap(),
        permissions: permissions("reporter"),
        required: true,
        subflow: None,
      })
      .unwrap()
      .with_action(binding(
        "reporter",
        "set_status",
        serde_json::json!({
          "status":status, "outbox":{"max_attempts":2,"retry_delay_millis":5},
        }),
      ))
      .unwrap()
      .with_input_binding(
        FlowInputBinding::new(
          BTreeMap::from([(
            key("stage"),
            FlowInputSource::Predecessor {
              view: FlowRecordView::Payload,
              path: String::new(),
            },
          )]),
          FlowDataMapping::new(FlowValueMapping::Pointer { path: "/stage".into() }).unwrap(),
        )
        .unwrap(),
      )
      .unwrap(),
    );
    transitions.push(FlowTransition::new(
      key("inspect-ticket"),
      key(route),
      FlowTransitionTarget::Node(key(&action)),
    ));
    transitions.push(FlowTransition::new(
      key(&action),
      key("queued"),
      FlowTransitionTarget::Node(key("continue-work")),
    ));
  }
  transitions.push(FlowTransition::new(
    key("inspect-ticket"),
    key("approved"),
    FlowTransitionTarget::Node(key("continue-work")),
  ));
  nodes.push(gate(
    "continue-work",
    vec![FlowOutcomeDefinition::new(
      key("finished"),
      FlowOutcomeKind::Success,
      value_schema().reference().clone(),
    )],
    "finished",
  ));
  transitions.push(FlowTransition::new(
    key("continue-work"),
    key("finished"),
    FlowTransitionTarget::Terminal(key("done")),
  ));
  FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: Some(value_schema().reference().clone()),
    entry: key("inspect-ticket"),
    nodes,
    transitions,
    terminals: vec![FlowTerminalDefinition::new(
      key("done"),
      value_schema().reference().clone(),
    )],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(
      BudgetLimit::new(32, 10000, 0, 0, 10000).unwrap(),
      permissions("reporter"),
      1,
    )
    .unwrap(),
  })
  .unwrap()
}
pub(super) async fn admitted(
  definition: FlowDefinition,
) -> (
  Arc<store_testing::InMemoryFactoryConfigurationStore>,
  FactoryRunId,
  FactoryDigest,
) {
  admitted_closure(vec![definition]).await
}
pub(super) async fn admitted_closure(
  definitions: Vec<FlowDefinition>,
) -> (
  Arc<store_testing::InMemoryFactoryConfigurationStore>,
  FactoryRunId,
  FactoryDigest,
) {
  let definition = &definitions[0];
  let work = store_testing::factory_node_journal_contract_fixture().admission.work;
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let flow = FlowRun::root(&run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), definitions).unwrap(),
    FlowAdmissionLimits::product_defaults(
      1,
      BudgetLimit::new(32, 10000, 0, 0, 10000).unwrap(),
      permissions("reporter"),
    )
    .unwrap(),
    flow,
    cycle,
    FactoryWipLimits::new(8, 16).unwrap(),
  )
  .unwrap()
  .with_data_schemas(vec![value_schema(), receipt_schema()])
  .unwrap();
  let store = Arc::new(store_testing::InMemoryFactoryConfigurationStore::new());
  let audit = store_testing::management_mutation_with_request((), "stage-admission")
    .audit()
    .clone();
  store
    .seed_factory_run(
      PublishedFactoryAdmission {
        work,
        run: run.clone(),
        flow: admitted,
        admitted_at: at(1),
      },
      FactoryDigest::from_bytes([1; 32]),
      &audit,
    )
    .unwrap();
  let claim = FactoryRunClaimRecord::new(
    run.id(),
    key("stage-owner"),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([2; 32])),
      at(10),
      at(900),
    )
    .unwrap(),
  );
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: run.id(),
      expected_version: run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        run.id(),
        AuditActorKind::Worker,
        None,
        key("claimed"),
        FactoryDigest::from_bytes([1; 32]),
        key("accepted"),
        at(10),
      ),
    })
    .await
    .unwrap();
  (store, run.id(), claim.id)
}

pub(super) fn nested_definition(child: &FlowDefinition) -> FlowDefinition {
  let call = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    // Deliberately reuse a child stage key; provenance must include its Flow Run/definition.
    key: key("inspect-ticket"),
    kind: FlowNodeKind::SubflowCall,
    input_schema: Some(value_schema().reference().clone()),
    outcomes: vec![FlowOutcomeDefinition::new(
      key("done"),
      FlowOutcomeKind::Success,
      value_schema().reference().clone(),
    )],
    budget: BudgetLimit::new(32, 10000, 0, 0, 10000).unwrap(),
    permissions: permissions("reporter"),
    required: true,
    subflow: Some(child.reference()),
  })
  .unwrap()
  .with_input_binding(constant_input())
  .unwrap();
  FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: Some(value_schema().reference().clone()),
    entry: key("inspect-ticket"),
    nodes: vec![call],
    transitions: vec![FlowTransition::new(
      key("inspect-ticket"),
      key("done"),
      FlowTransitionTarget::Terminal(key("root-done")),
    )],
    terminals: vec![FlowTerminalDefinition::new(
      key("root-done"),
      value_schema().reference().clone(),
    )],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(
      BudgetLimit::new(32, 10000, 0, 0, 10000).unwrap(),
      permissions("reporter"),
      1,
    )
    .unwrap(),
  })
  .unwrap()
}
