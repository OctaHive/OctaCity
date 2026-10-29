//! Criteria are fixed by installation; reports cannot choose their own verdict.
use crate::*;
use octacity_server_factory::*;
use serde_json::json;
use std::collections::BTreeMap;

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn reference(name: &str) -> ImmutableReference {
  ImmutableReference::new(key(name), key("v1"), FactoryDigest::from_bytes([91; 32]))
}
pub(super) fn contract() -> FactoryCommandObservationContract {
  FactoryCommandObservationContract {
    environment: reference("reviewed-probe-environment"),
    criterion: reference("http-status-symptom"),
    observations: FlowDataSchema::new(key("http-probe-observations"), key("v1"),
      FlowValueSchema::Object { fields: BTreeMap::from([
        (key("probe"), FlowFieldSchema::required(FlowValueSchema::Enum { values: vec![json!("GET /health")] })),
        (key("requests"), FlowFieldSchema::required(FlowValueSchema::Integer { minimum:0, maximum:1 })),
        (key("unexpected_statuses"), FlowFieldSchema::required(FlowValueSchema::Integer { minimum:0, maximum:1 })),
      ]) }).unwrap(),
    completion: serde_json::from_value(json!({"operation":"compare","left":{"source":"path","path":"/requests"},"comparison":"equal","right":{"source":"literal","value":1}})).unwrap(),
    symptom: serde_json::from_value(json!({"operation":"compare","left":{"source":"path","path":"/unexpected_statuses"},"comparison":"equal","right":{"source":"literal","value":1}})).unwrap(),
  }
}

pub(super) fn report(intent: &FlowBuildIntent, contract: &FactoryCommandObservationContract) -> serde_json::Value {
  json!({"input_digest":intent.input().digest().unwrap(), "subject":intent.input().subject(),
    "command":intent.profile().model_or_tool,"environment":contract.environment,"criterion":contract.criterion,
    "status":"completed","observations":{"probe":"GET /health","requests":1,"unexpected_statuses":1},"finding":"matched"})
}

#[test]
fn command_observation_requires_a_complete_applicable_symptom_check() {
  let (intent, _, _) = crate::factory_command_tests::command_fixture();
  let contract = contract();
  let valid = report(&intent, &contract);
  assert_eq!(
    verify_factory_command_observation(&contract, &intent, &valid).unwrap(),
    FactoryCommandFinding::Matched
  );
  let mut negative = valid.clone();
  negative["observations"]["unexpected_statuses"] = 0.into();
  negative["finding"] = "not_matched".into();
  assert_eq!(
    verify_factory_command_observation(&contract, &intent, &negative).unwrap(),
    FactoryCommandFinding::NotMatched
  );
  for (path, replacement) in [
    ("/input_digest", json!("ab".repeat(32))),
    ("/subject/base_revision", json!("another-revision")),
    ("/command", json!(reference("another-command"))),
    ("/environment", json!(reference("another-environment"))),
    ("/criterion", json!(reference("another-symptom"))),
    ("/observations/requests", json!(0)),
    ("/observations/probe", json!("GET /unrelated")),
    ("/observations/unexpected_statuses", json!(0)),
    ("/status", json!("environment_unavailable")),
  ] {
    let mut forged = valid.clone();
    *forged.pointer_mut(path).unwrap() = replacement;
    assert!(
      verify_factory_command_observation(&contract, &intent, &forged).is_err(),
      "must reject {path}"
    );
  }
  let (_, admitted, _, input, node) =
    crate::factory_node_test_support::fixtures::execution_fixture(FlowNodeKind::Reasoning);
  let model = FlowBuildIntent::new(input, &admitted, node, Default::default(), 0).unwrap();
  let mut prose = valid;
  prose["input_digest"] = json!(model.input().digest().unwrap());
  prose["subject"] = json!(model.input().subject());
  prose["command"] = json!(model.profile().model_or_tool);
  assert!(
    verify_factory_command_observation(&contract, &model, &prose).is_err(),
    "model output is not an independent command observation"
  );
}

#[test]
fn unavailable_command_conditions_preserve_an_indeterminate_finding_without_fabricated_observations() {
  let (intent, _, _) = crate::factory_command_tests::command_fixture();
  let contract = contract();
  for status in ["environment_unavailable", "needs_input"] {
    let mut value = report(&intent, &contract);
    value["status"] = status.into();
    value["finding"] = "indeterminate".into();
    value.as_object_mut().unwrap().remove("observations");
    assert_eq!(
      verify_factory_command_observation(&contract, &intent, &value).unwrap(),
      FactoryCommandFinding::Indeterminate
    );
    for forged in ["matched", "not_matched"] {
      value["finding"] = forged.into();
      assert!(verify_factory_command_observation(&contract, &intent, &value).is_err());
    }
  }
  let mut incomplete = report(&intent, &contract);
  incomplete["observations"]["requests"] = 0.into();
  incomplete["finding"] = "indeterminate".into();
  assert_eq!(
    verify_factory_command_observation(&contract, &intent, &incomplete).unwrap(),
    FactoryCommandFinding::Indeterminate
  );
  incomplete["finding"] = "not_matched".into();
  assert!(verify_factory_command_observation(&contract, &intent, &incomplete).is_err());
}

#[test]
fn installed_observation_contract_is_part_of_the_frozen_command_identity() {
  let (intent, admitted, mut profile) = crate::factory_command_tests::command_fixture();
  let legacy = profile.task_digest();
  profile.observation_contract = Some(contract());
  let reviewed = profile.task_digest();
  assert_ne!(
    reviewed, legacy,
    "observation policy is immutable execution configuration"
  );
  assert!(
    compile_factory_command_task(&intent, &admitted, &profile).is_err(),
    "old intent cannot acquire new criteria"
  );
  profile.observation_contract.as_mut().unwrap().environment = reference("changed-environment");
  assert_ne!(profile.task_digest(), reviewed);
  profile.observation_contract = None;
  assert_eq!(
    profile.task_digest(),
    legacy,
    "unconfigured commands preserve their identity"
  );
  assert!(compile_factory_command_task(&intent, &admitted, &profile).is_ok());
}

#[test]
fn an_unbounded_observation_policy_is_rejected_before_command_dispatch() {
  let mut invalid = contract();
  invalid.completion = FlowPredicate::Any {
    predicates: vec![invalid.symptom.clone(); 128],
  };
  let (intent, admitted, profile) = crate::factory_command_tests::command_fixture_with_observation(Some(invalid));
  assert!(
    compile_factory_command_task(&intent, &admitted, &profile).is_err(),
    "invalid installed criteria must fail before dispatch"
  );
}

#[test]
fn command_observation_rejects_candidate_checkout_as_exact_base_evidence() {
  let (_, admitted, _, input, node) =
    crate::factory_node_test_support::fixtures::verification_execution_fixture(FlowNodeKind::BuildCommand);
  let intent = FlowBuildIntent::new(input, &admitted, node, Default::default(), 0).unwrap();
  let contract = contract();
  assert!(
    verify_factory_command_observation(&contract, &intent, &report(&intent, &contract)).is_err(),
    "a check executed against a candidate cannot attest to the unchanged base revision"
  );
}

#[test]
fn command_report_schema_preserves_the_selected_observation_shape_and_common_bounds() {
  let (intent, _, _) = crate::factory_command_tests::command_fixture();
  let contract = contract();
  let schema = contract.report_schema().unwrap();
  let valid = report(&intent, &contract);
  FlowPayload::new(&schema, valid.clone()).unwrap();
  for (field, value) in [
    ("context", json!("private")),
    ("producer", json!({"job_id":"invented"})),
  ] {
    let mut forged = valid.clone();
    forged[field] = value;
    assert!(FlowPayload::new(&schema, forged).is_err());
  }
  let mut wrong_probe = valid.clone();
  wrong_probe["observations"]["probe"] = "GET /unrelated".into();
  assert!(FlowPayload::new(&schema, wrong_probe).is_err());
  let mut unavailable = valid;
  unavailable["status"] = "environment_unavailable".into();
  unavailable["finding"] = "indeterminate".into();
  unavailable.as_object_mut().unwrap().remove("observations");
  FlowPayload::new(&schema, unavailable).unwrap();
  let mut shape = FlowValueSchema::Boolean;
  for _ in 0..MAX_FLOW_SCHEMA_DEPTH {
    shape = FlowValueSchema::Object {
      fields: BTreeMap::from([(key("nested"), FlowFieldSchema::required(shape))]),
    };
  }
  let mut too_deep = contract;
  too_deep.observations = FlowDataSchema::new(key("deep-observations"), key("v1"), shape).unwrap();
  assert!(
    too_deep.validate().is_err(),
    "the complete report must fit the common schema depth ceiling"
  );
}

#[test]
fn observation_command_refuses_an_incompatible_output_schema_before_dispatch() {
  let (intent, admitted, profile) =
    crate::factory_command_tests::command_fixture_with_observation_and_schema(Some(contract()), None);
  assert!(
    compile_factory_command_task(&intent, &admitted, &profile).is_err(),
    "a bounded criterion cannot qualify an unrelated declared JSON result"
  );
}

#[test]
fn observation_bindings_expose_only_frozen_report_metadata() {
  let (_, _, profile) = crate::factory_command_tests::command_fixture_with_observation(Some(contract()));
  let bindings = profile.observation_bindings().unwrap();
  assert_eq!(bindings.len(), 1);
  let binding = &bindings[0];
  assert_eq!(binding.command, profile.identity);
  assert_eq!(binding.tool, profile.executor);
  assert_eq!(binding.plugin, profile.plugin);
  assert_eq!(binding.task_digest, profile.task_digest());
  assert_eq!(binding.build_configuration, profile.build_configuration);
  assert_eq!(binding.result_output, profile.outputs[0].kind().clone());
  assert_eq!(&binding.schema, profile.outputs[0].schema());
  let value = serde_json::to_value(binding).unwrap();
  for private in ["program", "parameters", "credentials", "permissions"] {
    assert!(value.get(private).is_none());
  }
  let (_, _, legacy) = crate::factory_command_tests::command_fixture();
  assert!(
    legacy.observation_bindings().unwrap().is_empty(),
    "plain JSON reports advertise no observation capability"
  );
}

#[test]
fn triage_optional_command_publishes_exact_selected_report_before_the_two_reasoning_stages() {
  use crate::factory_node_test_support::fixtures;
  let mut template = fixtures::supplied_triage_template_variant("with-reproduction-v1.json").unwrap();
  assert!(
    !template.schemas.contains_key(&key("reproduction_report")),
    "no invented probe schema is bundled"
  );
  let (_, _, mut profile) = crate::factory_command_tests::command_fixture_with_observation(Some(contract()));
  let original = profile.outputs[0].clone();
  profile.outputs[0] = EvidenceRequirement::new(
    key("reproduction-report"),
    original.output_kind(),
    original.schema().clone(),
    original.tool().clone(),
    original.plugin().clone(),
  );
  let choice = profile.observation_bindings().unwrap().remove(0);
  let schema = profile.observation_contract.as_ref().unwrap().report_schema().unwrap();
  template.schemas.insert(
    key("reproduction_report"),
    FlowTemplateSchema {
      identity: schema.reference().identity().clone(),
      version: schema.reference().version().clone(),
      shape: schema.shape().clone(),
    },
  );
  let mut wire = serde_json::to_value(&template).unwrap();
  let analyst = wire["nodes"]
    .as_array_mut()
    .unwrap()
    .iter_mut()
    .find(|n| n["key"] == "classification")
    .unwrap();
  analyst["input_binding"]["mapping"]["value"]["fields"]["command_policy"]["value"] = json!([choice]);
  template = serde_json::from_value(wire).unwrap();
  let project = choice.build_configuration.project_id();
  let mut command = fixtures::binding(project);
  command.tool = choice.tool.clone();
  command.plugin = choice.plugin.clone();
  command.model_or_tool = choice.command.clone();
  command.task_digest = choice.task_digest;
  command.build_configuration = choice.build_configuration.clone();
  command.result_output = choice.result_output.clone();
  command.result_outcome = key("observed");
  let profiles = BTreeMap::from([
    (key("reproduction"), command.clone()),
    (key("eligibility"), fixtures::binding(project)),
    (key("classification"), fixtures::binding(project)),
  ]);
  let published = template
    .instantiate(FlowDefinitionId::generate(), FlowDefinitionVersion::INITIAL, &profiles)
    .unwrap();
  assert_eq!(published.definition.nodes().len(), 5);
  assert_eq!(
    published
      .definition
      .nodes()
      .iter()
      .filter(|n| n.kind() == FlowNodeKind::Reasoning)
      .count(),
    2
  );
  let node = published.definition.node(&key("reproduction")).unwrap();
  assert_eq!(node.kind(), FlowNodeKind::BuildCommand);
  assert_eq!(node.build(), Some(&command));
  assert_eq!(
    node.max_execution_retries(),
    1,
    "technical stops require an explicit bounded operator retry"
  );
  assert!(serde_json::to_value(node.context_binding().unwrap()).unwrap()["candidate_revision"].is_null());
  for (from, to) in [("eligibility_gate", "reproduction"), ("reproduction", "classification")] {
    assert!(
      published
        .definition
        .transitions()
        .iter()
        .any(|e| e.predecessor().as_str() == from
          && matches!(e.target(),FlowTransitionTarget::Node(target) if target.as_str()==to))
    );
  }
}
