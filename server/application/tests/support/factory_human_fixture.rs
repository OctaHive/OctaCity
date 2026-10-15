//! Shared immutable HumanGate graph fixture for public owner contracts.
use octacity_server_factory::*;
use octacity_server_store::{PublishedFactoryAdmission, testing};
fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn at(value: i64) -> octacity_server_domain::Timestamp {
  octacity_server_domain::Timestamp::from_unix_millis(value).unwrap()
}
pub(crate) fn human_fixture_with_contract(
  response: FlowDataSchema,
  binding: Option<FlowHumanGateBinding>,
  restart: bool,
) -> PublishedFactoryAdmission {
  let fixture = testing::factory_node_journal_contract_fixture();
  let work = fixture.admission.work;
  let schema = FlowDataSchema::new(key("operator.confirmation"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let node = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("legal_review"),
    kind: FlowNodeKind::HumanGate,
    input_schema: Some(schema.reference().clone()),
    outcomes: vec![FlowOutcomeDefinition::new(
      key("approved"),
      FlowOutcomeKind::Success,
      response.reference().clone(),
    )],
    budget: fixture.attempt.budget(),
    permissions: FactoryPermissionSet::deny_all(),
    required: true,
    subflow: None,
  })
  .unwrap()
  .with_input_binding(
    FlowInputBinding::new(
      Default::default(),
      FlowDataMapping::new(FlowValueMapping::Constant {
        value: serde_json::json!(true),
      })
      .unwrap(),
    )
    .unwrap(),
  )
  .unwrap();
  let node = match binding {
    Some(binding) => node.with_operator_requests(binding).unwrap(),
    None => node,
  };
  let transition = if restart {
    let mut value = serde_json::to_value(
      FlowTransition::repeated(
        key("legal_review"),
        key("approved"),
        FlowTransitionTarget::Node(key("legal_review")),
        1,
        key("reviewed"),
      )
      .unwrap(),
    )
    .unwrap();
    value["starts_cycle"] = serde_json::json!(true);
    serde_json::from_value(value).unwrap()
  } else {
    FlowTransition::new(
      key("legal_review"),
      key("approved"),
      FlowTransitionTarget::Terminal(key("reviewed")),
    )
  };
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: Some(schema.reference().clone()),
    entry: node.key().clone(),
    nodes: vec![node],
    transitions: vec![transition],
    terminals: vec![FlowTerminalDefinition::new(
      key("reviewed"),
      response.reference().clone(),
    )],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(
      fixture.admission.flow.limits().budget(),
      FactoryPermissionSet::deny_all(),
      1,
    )
    .unwrap(),
  })
  .unwrap();
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let root = FlowRun::root(&run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&root).unwrap();
  let flow = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    fixture.admission.flow.limits().clone(),
    root,
    cycle,
    octacity_server_factory::FactoryWipLimits::new(8, 16).unwrap(),
  )
  .unwrap()
  .with_data_schemas(vec![schema, response])
  .unwrap();
  PublishedFactoryAdmission {
    work,
    run,
    flow,
    admitted_at: at(1),
  }
}
