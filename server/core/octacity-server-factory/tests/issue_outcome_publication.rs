use octacity_server_factory::*;
use serde_json::{Value, json};
use uuid::Uuid;

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}

fn schema(shape: Value) -> FlowDataSchema {
  FlowDataSchema::new(key("assessment"), key("v1"), serde_json::from_value(shape).unwrap()).unwrap()
}

fn budget() -> BudgetLimit {
  BudgetLimit::new(4, 1000, 0, 0, 4096).unwrap()
}

fn node(
  schema: &FlowDataSchema,
  name: &str,
  kind: FlowNodeKind,
  outcomes: &[(&str, Option<Value>)],
) -> Result<FlowNodeDefinition, String> {
  let outcomes = outcomes
    .iter()
    .map(|(name, presentation)| {
      let mut wire = json!({"key":name,"kind":"success","schema":schema.reference()});
      if let Some(presentation) = presentation {
        wire["issue_presentation"] = presentation.clone();
      }
      serde_json::from_value(wire).map_err(|error| error.to_string())
    })
    .collect::<Result<_, _>>()?;
  Ok(
    FlowNodeDefinition::new(FlowNodeDefinitionInput {
      key: key(name),
      kind,
      input_schema: None,
      outcomes,
      budget: budget(),
      permissions: FactoryPermissionSet::deny_all(),
      required: true,
      subflow: None,
    })
    .unwrap(),
  )
}

fn route(from: &str, outcome: &str, to: &str) -> FlowTransition {
  let target = if to == "finished" {
    FlowTransitionTarget::Terminal(key(to))
  } else {
    FlowTransitionTarget::Node(key(to))
  };
  FlowTransition::new(key(from), key(outcome), target)
}

fn definition(
  schema: &FlowDataSchema,
  identity: u128,
  nodes: Vec<FlowNodeDefinition>,
  transitions: Vec<FlowTransition>,
) -> FlowDefinition {
  let terminals = transitions
    .iter()
    .flat_map(|route| {
      let terminal = match route.target() {
        FlowTransitionTarget::Terminal(terminal) => Some(terminal),
        FlowTransitionTarget::Node(_) => None,
      };
      terminal.into_iter().chain(route.exhausted_terminal())
    })
    .collect::<std::collections::BTreeSet<_>>();
  let terminals = terminals
    .into_iter()
    .map(|key| FlowTerminalDefinition::new(key.clone(), schema.reference().clone()))
    .collect();
  FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::from_uuid(Uuid::from_u128(identity)).unwrap(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: None,
    entry: nodes[0].key().clone(),
    nodes,
    transitions,
    terminals,
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(budget(), FactoryPermissionSet::deny_all(), 2).unwrap(),
  })
  .unwrap()
}

fn publish_graph(schema: FlowDataSchema, definitions: Vec<FlowDefinition>) -> Result<FactoryFlowConfiguration, String> {
  FactoryFlowConfiguration::new(
    PinnedFlowDefinitionClosure::new(definitions[0].reference(), definitions).unwrap(),
    FlowAdmissionLimits::product_defaults(2, budget(), FactoryPermissionSet::deny_all()).unwrap(),
    vec![schema],
    Default::default(),
  )
  .map_err(|error| error.to_string())
}

fn publish(shape: Value, presentation: Option<Value>) -> Result<FactoryFlowConfiguration, String> {
  let schema = schema(shape);
  let node = node(
    &schema,
    "evaluate",
    FlowNodeKind::DeterministicGate,
    &[("done", presentation)],
  )?;
  let graph = definition(&schema, 1, vec![node], vec![route("evaluate", "done", "finished")]);
  publish_graph(schema, vec![graph])
}

fn assessment() -> Value {
  json!({"type":"object","fields":{
    "conclusion":{"required":true,"value":{"type":"string","min_bytes":1,"max_bytes":512}},
    "internal_notes":{"required":false,"value":{"type":"string","min_bytes":0,"max_bytes":8192}}
  }})
}

fn custom() -> Value {
  json!({"kind":"custom_v1","title":"Assessment","conclusion_field":"conclusion"})
}

#[test]
fn publication_freezes_only_the_explicit_custom_binding_and_round_trips_legacy_flows() {
  let legacy = publish(assessment(), None).unwrap();
  let legacy_wire = serde_json::to_value(&legacy).unwrap();
  let legacy_outcome = &legacy_wire["closure"]["definitions"][0]["nodes"][0]["outcomes"][0];
  assert_eq!(
    legacy_outcome,
    &json!({"key":"done","kind":"success","schema":legacy.data_schemas[0].reference()})
  );
  assert_eq!(
    serde_json::from_value::<FactoryFlowConfiguration>(legacy_wire).unwrap(),
    legacy
  );

  let flow = publish(assessment(), Some(custom())).unwrap();
  assert_ne!(flow.closure.root().digest(), legacy.closure.root().digest());
  let wire = serde_json::to_value(&flow).unwrap();
  let outcome = &wire["closure"]["definitions"][0]["nodes"][0]["outcomes"][0];
  assert_eq!(outcome["issue_presentation"], custom());
  assert!(outcome.get("public_result").is_none());
  assert_eq!(serde_json::from_value::<FactoryFlowConfiguration>(wire).unwrap(), flow);
}

#[test]
fn publication_rejects_an_optional_unbounded_or_missing_conclusion() {
  for (field, value) in [
    ("required", json!(false)),
    ("value", json!({"type":"string","min_bytes":1,"max_bytes":8192})),
    ("value", json!({"type":"boolean"})),
  ] {
    let mut shape = assessment();
    shape["fields"]["conclusion"][field] = value;
    assert!(
      publish(shape, Some(custom()))
        .unwrap_err()
        .contains("Issue presentation")
    );
  }
  let mut binding = custom();
  binding["conclusion_field"] = json!("missing");
  assert!(
    publish(assessment(), Some(binding))
      .unwrap_err()
      .contains("Issue presentation")
  );
}

fn retained_artifact() -> Value {
  json!({"type":"object","fields":{
    "artifact_id":{"required":true,"value":{"type":"string","min_bytes":36,"max_bytes":36}},
    "content_digest":{"required":true,"value":{"type":"string","min_bytes":64,"max_bytes":64}},
    "encoded_size":{"required":true,"value":{"type":"integer","minimum":1,"maximum":4096}}
  }})
}

#[test]
fn publication_bounds_slot_analysis_by_expanded_outcomes_as_well_as_steps() {
  let schema = schema(assessment());
  let labels: Vec<_> = (0..MAX_FLOW_DEFINITION_EDGES)
    .map(|index| format!("choice-{index}"))
    .collect();
  let wide = node(
    &schema,
    "wide",
    FlowNodeKind::DeterministicGate,
    &labels.iter().map(|name| (name.as_str(), None)).collect::<Vec<_>>(),
  )
  .unwrap();
  let child = definition(
    &schema,
    2,
    vec![wide],
    labels.iter().map(|name| route("wide", name, "finished")).collect(),
  );
  let mut shared = custom();
  shared["slot"] = json!("assessment");
  let choose = node(
    &schema,
    "choose",
    FlowNodeKind::DeterministicGate,
    &[("a", Some(shared.clone())), ("b", Some(shared))],
  )
  .unwrap();
  let parent = |count: usize| {
    let calls: Vec<_> = (0..count).map(|index| format!("call-{index}")).collect();
    let mut nodes = calls
      .iter()
      .map(|name| {
        FlowNodeDefinition::new(FlowNodeDefinitionInput {
          key: key(name),
          kind: FlowNodeKind::SubflowCall,
          input_schema: None,
          outcomes: vec![FlowOutcomeDefinition::new(
            key("finished"),
            FlowOutcomeKind::Success,
            schema.reference().clone(),
          )],
          budget: budget(),
          permissions: FactoryPermissionSet::deny_all(),
          required: true,
          subflow: Some(child.reference()),
        })
        .unwrap()
      })
      .collect::<Vec<_>>();
    nodes.push(choose.clone());
    let routes = calls
      .iter()
      .enumerate()
      .map(|(index, name)| {
        FlowTransition::new(
          key(name),
          key("finished"),
          FlowTransitionTarget::Node(key(calls.get(index + 1).map_or("choose", String::as_str))),
        )
      })
      .chain([route("choose", "a", "finished"), route("choose", "b", "finished")])
      .collect();
    definition(&schema, 1, nodes, routes)
  };
  publish_graph(schema.clone(), vec![parent(1), child.clone()]).unwrap();
  let calls = MAX_FLOW_EXPANDED_NODES as usize / MAX_FLOW_DEFINITION_EDGES;
  let definitions = vec![parent(calls), child];
  let result = publish_graph(schema, definitions);
  assert!(matches!(result, Err(ref message) if message.contains("Issue presentation analysis")));
}

#[test]
fn publication_checks_built_in_fields_and_custom_references() {
  let mut triage = assessment();
  triage["fields"]["decision"] = json!({"required":true,"value":{"type":"enum","values":["eligible","review"]}});
  triage["fields"]["priority"] = json!({"required":false,"value":{"type":"integer","minimum":0,"maximum":100}});
  publish(triage.clone(), Some(json!({"kind":"triage_v1"}))).unwrap();
  triage["fields"]["priority"]["value"]["maximum"] = json!(101);
  assert!(
    publish(triage, Some(json!({"kind":"triage_v1"})))
      .unwrap_err()
      .contains("Issue presentation")
  );

  let mut specs = assessment();
  specs["fields"]["documents"] = json!({"required":true,"value":{
    "type":"array","min_items":1,"max_items":4,"item":retained_artifact()
  }});
  publish(specs.clone(), Some(json!({"kind":"specifications_v1"}))).unwrap();
  specs["fields"]["documents"]["value"]["max_items"] = json!(64);
  assert!(
    publish(specs, Some(json!({"kind":"specifications_v1"})))
      .unwrap_err()
      .contains("Issue presentation")
  );

  let mut review = assessment();
  review["fields"]["verdict"] =
    json!({"required":true,"value":{"type":"enum","values":["approved","changes_required"]}});
  review["fields"]["review"] = json!({"required":true,"value":{"type":"string","min_bytes":64,"max_bytes":64}});
  publish(review.clone(), Some(json!({"kind":"review_v1"}))).unwrap();
  review["fields"]["review"]["value"]["min_bytes"] = json!(1);
  assert!(
    publish(review, Some(json!({"kind":"review_v1"})))
      .unwrap_err()
      .contains("Issue presentation")
  );

  let mut shape = assessment();
  shape["fields"]["report"] = json!({"required":false,"value":retained_artifact()});
  let mut custom = custom();
  custom["references"] = json!([{"field":"report","kind":"artifact"}]);
  publish(shape.clone(), Some(custom.clone())).unwrap();
  custom["references"][0]["field"] = json!("internal_notes");
  assert!(publish(shape, Some(custom)).unwrap_err().contains("Issue presentation"));
}

#[test]
fn a_shared_slot_accepts_alternative_branches_but_rejects_parallel_or_sequential_producers() {
  let schema = schema(assessment());
  let mut shared = custom();
  shared["slot"] = json!("evaluation");
  let producers = |binding: Value| {
    ["left", "right"].map(|name| {
      node(
        &schema,
        name,
        FlowNodeKind::DeterministicGate,
        &[("done", Some(binding.clone()))],
      )
      .unwrap()
    })
  };
  let gate = |kind, outcomes: &[(&str, Option<Value>)]| node(&schema, "choose", kind, outcomes).unwrap();

  let [left, right] = producers(shared.clone());
  let exclusive = definition(
    &schema,
    1,
    vec![
      gate(FlowNodeKind::DeterministicGate, &[("a", None), ("b", None)]),
      left.clone(),
      right.clone(),
    ],
    vec![
      route("choose", "a", "left"),
      route("choose", "b", "right"),
      route("left", "done", "finished"),
      route("right", "done", "finished"),
    ],
  );
  publish_graph(schema.clone(), vec![exclusive]).unwrap();

  let fork = gate(FlowNodeKind::FanOut, &[("both", None)]);
  let parallel_routes = vec![
    route("choose", "both", "left"),
    route("choose", "both", "right"),
    route("left", "done", "finished"),
    route("right", "done", "finished"),
  ];
  let parallel = definition(
    &schema,
    1,
    vec![fork.clone(), left.clone(), right.clone()],
    parallel_routes.clone(),
  );
  assert!(
    publish_graph(schema.clone(), vec![parallel])
      .unwrap_err()
      .contains("Issue presentation slot")
  );

  let [default_left, default_right] = producers(custom());
  let different_slots = definition(&schema, 1, vec![fork, default_left, default_right], parallel_routes);
  publish_graph(schema.clone(), vec![different_slots]).unwrap();

  let sequential = definition(
    &schema,
    1,
    vec![left, right],
    vec![route("left", "done", "right"), route("right", "done", "finished")],
  );
  assert!(
    publish_graph(schema, vec![sequential])
      .unwrap_err()
      .contains("Issue presentation slot")
  );
}

#[test]
fn editable_templates_preserve_the_same_outcome_binding_when_compiled() {
  let template: FlowDefinitionTemplate = serde_json::from_value(json!({
    "input_schema":"assessment",
    "entry":"evaluate",
    "schemas":{"assessment":{"identity":"assessment","version":"v1","shape":assessment()}},
    "nodes":[{
      "key":"evaluate","kind":"fan_out","input_schema":"assessment","required":true,"budget":budget(),
      "input_binding":{"sources":{},"mapping":{"value":{"mapping":"constant","value":{"conclusion":"ready"}}}},
      "outcomes":[{"key":"done","kind":"success","schema":"assessment","issue_presentation":custom()}]
    }],
    "transitions":[route("evaluate", "done", "finished")],
    "terminals":{"finished":"assessment"},
    "execution":FlowExecutionPolicy::new(budget(), FactoryPermissionSet::deny_all(), 1).unwrap()
  }))
  .unwrap();
  let compiled = template
    .instantiate(
      FlowDefinitionId::generate(),
      FlowDefinitionVersion::INITIAL,
      &Default::default(),
    )
    .unwrap();
  let flow = publish_graph(compiled.schemas[0].clone(), vec![compiled.definition]).unwrap();
  assert_eq!(
    serde_json::to_value(flow).unwrap()["closure"]["definitions"][0]["nodes"][0]["outcomes"][0]["issue_presentation"],
    custom()
  );
}

#[test]
fn a_nested_outcome_and_its_caller_follow_the_same_terminal_choice() {
  let schema = schema(assessment());
  let mut binding = custom();
  binding["slot"] = json!("evaluation");
  let child = definition(
    &schema,
    2,
    vec![
      node(
        &schema,
        "evaluate",
        FlowNodeKind::DeterministicGate,
        &[("a", Some(binding.clone())), ("b", None)],
      )
      .unwrap(),
    ],
    ["a", "b"]
      .map(|name| FlowTransition::new(key("evaluate"), key(name), FlowTransitionTarget::Terminal(key(name))))
      .to_vec(),
  );
  let call = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("call"),
    kind: FlowNodeKind::SubflowCall,
    input_schema: None,
    outcomes: ["a", "b"]
      .map(|name| FlowOutcomeDefinition::new(key(name), FlowOutcomeKind::Success, schema.reference().clone()))
      .to_vec(),
    budget: budget(),
    permissions: FactoryPermissionSet::deny_all(),
    required: true,
    subflow: Some(child.reference()),
  })
  .unwrap();
  let next = node(
    &schema,
    "evaluate",
    FlowNodeKind::DeterministicGate,
    &[("done", Some(binding))],
  )
  .unwrap();
  let parent = |finished, continued| {
    definition(
      &schema,
      1,
      vec![call.clone(), next.clone()],
      vec![
        route("call", finished, "finished"),
        route("call", continued, "evaluate"),
        route("evaluate", "done", "finished"),
      ],
    )
  };
  publish_graph(schema.clone(), vec![parent("a", "b"), child.clone()]).unwrap();
  let error = publish_graph(schema.clone(), vec![parent("b", "a"), child]).unwrap_err();
  assert!(error.contains("slot evaluation"), "{error}");
  assert!(error.contains("evaluate.a"), "{error}");
  assert!(error.contains("evaluate.done"), "{error}");
}

#[test]
fn a_bounded_repeat_can_make_two_outcomes_of_one_step_conflict() {
  let schema = schema(assessment());
  let mut binding = custom();
  binding["slot"] = json!("evaluation");
  let choose = node(
    &schema,
    "choose",
    FlowNodeKind::DeterministicGate,
    &[("again", Some(binding.clone())), ("done", Some(binding))],
  )
  .unwrap();
  let exclusive = definition(
    &schema,
    1,
    vec![choose.clone()],
    vec![
      route("choose", "again", "finished"),
      route("choose", "done", "finished"),
    ],
  );
  publish_graph(schema.clone(), vec![exclusive]).unwrap();
  let retry = node(&schema, "retry", FlowNodeKind::DeterministicGate, &[("retry", None)]).unwrap();
  let repeated = definition(
    &schema,
    1,
    vec![choose, retry],
    vec![
      route("choose", "again", "retry"),
      route("choose", "done", "finished"),
      FlowTransition::repeated(
        key("retry"),
        key("retry"),
        FlowTransitionTarget::Node(key("choose")),
        1,
        key("finished"),
      )
      .unwrap(),
    ],
  );
  assert!(
    publish_graph(schema, vec![repeated])
      .unwrap_err()
      .contains("Issue presentation slot")
  );
}

#[test]
fn parallel_child_terminals_still_select_one_caller_outcome() {
  let schema = schema(assessment());
  let branches = ["a", "b"];
  let child = definition(
    &schema,
    2,
    std::iter::once(node(&schema, "fork", FlowNodeKind::FanOut, &[("both", None)]).unwrap())
      .chain(branches.map(|name| node(&schema, name, FlowNodeKind::DeterministicGate, &[("done", None)]).unwrap()))
      .collect(),
    branches
      .into_iter()
      .flat_map(|name| {
        [
          route("fork", "both", name),
          FlowTransition::new(key(name), key("done"), FlowTransitionTarget::Terminal(key(name))),
        ]
      })
      .collect(),
  );
  let mut binding = custom();
  binding["slot"] = json!("evaluation");
  let outcomes = branches.map(|name| {
    FlowOutcomeDefinition::new(key(name), FlowOutcomeKind::Success, schema.reference().clone())
      .with_issue_presentation(Some(serde_json::from_value(binding.clone()).unwrap()))
  });
  let call = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("call"),
    kind: FlowNodeKind::SubflowCall,
    input_schema: None,
    outcomes: outcomes.to_vec(),
    budget: budget(),
    permissions: FactoryPermissionSet::deny_all(),
    required: true,
    subflow: Some(child.reference()),
  })
  .unwrap();
  let parent = definition(
    &schema,
    1,
    vec![call],
    branches.map(|name| route("call", name, "finished")).to_vec(),
  );
  publish_graph(schema, vec![parent, child]).unwrap();
}

#[test]
fn issue_slot_display_distinguishes_dotted_keys_and_explicit_slots() {
  use octacity_server_factory::{FlowDefinitionId, FlowIssueSlot};
  let flow: FlowDefinitionId = "0199a6f4-d56c-7440-9aa2-6a320f862795".parse().unwrap();
  let first = FlowIssueSlot::Custom {
    flow,
    node: "review.main".parse().unwrap(),
    outcome: "accepted".parse().unwrap(),
  };
  let second = FlowIssueSlot::Custom {
    flow,
    node: "review".parse().unwrap(),
    outcome: "main.accepted".parse().unwrap(),
  };
  let named = FlowIssueSlot::Named(format!("{flow}.review.main.accepted").parse().unwrap());
  assert_ne!(first.to_string(), second.to_string());
  assert_ne!(first.to_string(), named.to_string());
}
