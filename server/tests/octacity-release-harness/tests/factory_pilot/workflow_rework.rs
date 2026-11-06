//! One same-cycle retry with the exact prior candidate and independent review finding.
use super::*;
use octacity_protocol::{CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_OUTPUT};
use octacity_server_application::{FactoryCodexCapturePolicy, FactoryCodexUsagePolicy};
use octacity_server_factory::*;
use std::collections::BTreeMap;

pub(super) const PAUSED: &str = "rework-paused";
const INITIAL: &str = "initial-implementation";
const REWORK: &str = "implementation-rework-entry";
const CLOSED: &str = "rework-closed";

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}

fn text(max_bytes: usize) -> FlowValueSchema {
  FlowValueSchema::String {
    min_bytes: 1,
    max_bytes,
  }
}

fn object(fields: impl IntoIterator<Item = (&'static str, FlowValueSchema)>) -> FlowValueSchema {
  FlowValueSchema::Object {
    fields: fields
      .into_iter()
      .map(|(name, shape)| (key(name), FlowFieldSchema::required(shape)))
      .collect(),
  }
}

pub(super) fn configure_coder(model: &mut models::InstalledModel) {
  let profile = &mut model.profile;
  let mut instructions: Value = serde_json::from_str(profile.instructions.as_str()).unwrap();
  instructions["files"].as_array_mut().unwrap().push(json!({
    "path":"/workspace/source/public/explanation.txt",
    "text":{"$input":"/payload/value/explanation"}
  }));
  profile.instructions = FactoryText::new(instructions.to_string()).unwrap();
  profile.toolchain.model = ImmutableReference::new(
    profile.toolchain.model.identity().clone(),
    profile.toolchain.model.version().clone(),
    FactoryDigest::content_sha256(profile.instructions.as_str().as_bytes()),
  );
  let FactoryCodexUsagePolicy::TokenRates { model, .. } = &mut profile.usage_policy else {
    unreachable!("finite coder uses fixture token rates")
  };
  *model = profile.toolchain.model.clone();
  let capture = profile.capture.as_ref().unwrap();
  let mut instruction = capture.instruction().clone();
  instruction.policy.max_changed_paths = 2;
  profile.candidate_capture_tool = Some(capture.tool().clone());
  profile.capture = Some(FactoryCodexCapturePolicy::new(capture.tool().clone(), instruction).unwrap());
}

pub(super) fn extend(
  draft: &mut Value,
  author: &str,
  coder: &str,
  reviewer: &str,
  decision: &str,
  handoff: &FlowDataSchema,
) {
  let nodes = draft["root"]["nodes"].as_array_mut().unwrap();
  let artifact = object([
    ("artifact_id", text(36)),
    ("content_digest", text(64)),
    (
      "encoded_size",
      FlowValueSchema::Integer {
        minimum: 1,
        maximum: i64::MAX,
      },
    ),
  ]);
  let input = FlowDataSchema::new(
    key("pilot.rework-implementation"),
    key("v1"),
    FlowValueSchema::Object {
      fields: BTreeMap::from([
        (key("handoff"), FlowFieldSchema::required(handoff.shape().clone())),
        (key("explanation"), FlowFieldSchema::required(text(512))),
        (
          key("parent"),
          FlowFieldSchema::optional(object([
            ("revision", text(64)),
            ("bundle", artifact.clone()),
            ("manifest", artifact),
          ])),
        ),
      ]),
    },
  )
  .unwrap();
  let gate_budget = BudgetLimit::new(1, 60_000, 0, 0, 1024 * 1024).unwrap();
  let permissions = FactoryPermissionSet::deny_all();
  let pointer = |path: &str| json!({"mapping":"pointer","path":path});
  let predecessor = json!({"source":"predecessor","view":"payload","path":""});
  let record = |node: &str, outcome: &str, view: &str, path: &str| json!({"source":"record","node":node,"outcome":outcome,"view":view,"path":path});
  let gate = |name: &str, sources: Value, fields: Value| {
    let mut node = workflow::node(name, "deterministic_gate", &input, gate_budget, &permissions, "ready");
    node["input_binding"] = json!({"sources":sources,"mapping":{"value":{"mapping":"object","fields":fields}}});
    node["gate"] = json!(
      FlowGateProgram::new(vec![], key("ready"))
        .unwrap()
        .with_default_output_mapping(FlowDataMapping::new(FlowValueMapping::Pointer { path: String::new() }).unwrap())
        .unwrap()
        .binding()
        .unwrap()
    );
    node
  };
  let initial = gate(
    INITIAL,
    json!({"handoff":predecessor}),
    json!({
      "handoff":pointer("/handoff"),
      "explanation":{"mapping":"constant","value":"Initial placeholder."},
    }),
  );
  let rework = gate(
    REWORK,
    json!({
      "handoff":record(author,"authored","payload",""),
      "finding":record(reviewer,"assessed","payload","/findings/0/remediation"),
      "revision":record(coder,"implemented","metadata",&format!("/build/outputs/{}/data/payload/value/candidate_revision",models::CANDIDATE_FACTS)),
      "bundle":record(coder,"implemented","metadata",&format!("/build/outputs/{CHANGE_SET_BUNDLE_OUTPUT}/artifact")),
      "manifest":record(coder,"implemented","metadata",&format!("/build/outputs/{CHANGE_SET_MANIFEST_OUTPUT}/artifact")),
    }),
    json!({
      "handoff":pointer("/handoff"),
      "explanation":pointer("/finding"),
      "parent":{"mapping":"object","fields":{"revision":pointer("/revision"),"bundle":pointer("/bundle"),"manifest":pointer("/manifest")}},
    }),
  );
  let implementation = nodes.iter_mut().find(|node| node["key"] == coder).unwrap();
  implementation["input_schema"] = json!(input.reference());
  implementation["input_binding"] =
    json!({"sources":{"candidate":predecessor},"mapping":{"value":pointer("/candidate")}});
  let context = &mut implementation["context_binding"];
  context["candidate_revision"] = json!({"value":pointer("/parent/revision")});
  context["candidate_when_present"] = json!("/parent");
  for (name, path) in [("bundle", "/parent/bundle"), ("manifest", "/parent/manifest")] {
    context["entries"][name] = json!({"source_kind":"evidence","when_present":"/parent","source":{"value":{
      "mapping":"object","fields":{"kind":{"mapping":"constant","value":"artifact"},"value":pointer(path)}
    }}});
  }
  let operation = FlowHumanGateBinding::new(
    FlowHumanOperation::Escalation,
    BTreeMap::from([(FlowHumanDisposition::Abort, key("closed"))]),
  )
  .unwrap();
  let response = FlowDataSchema::new(
    key("pilot.rework-closure"),
    key("v1"),
    operation.response_shape(FlowHumanDisposition::Abort).unwrap(),
  )
  .unwrap();
  let mut paused = workflow::node(PAUSED, "human_gate", &response, gate_budget, &permissions, "closed");
  paused["input_schema"] = json!(input.reference());
  paused["input_binding"] = json!({"sources":{"review":predecessor},"mapping":{"value":pointer("/review")}});
  paused["operator_requests"] = json!(operation);
  nodes.extend([initial, rework, paused]);

  let edges = draft["root"]["transitions"].as_array_mut().unwrap();
  for (producer, outcome, target) in [(author, "authored", INITIAL), (decision, "rework", REWORK)] {
    *edges
      .iter_mut()
      .find(|edge| edge["predecessor"] == producer && edge["outcome"] == outcome)
      .unwrap() = workflow::edge(producer, outcome, "node", target);
  }
  let mut repeat = workflow::edge(REWORK, "ready", "node", coder);
  repeat["max_repeats"] = json!(1);
  repeat["exhausted_node"] = json!(PAUSED);
  edges.extend([
    workflow::edge(INITIAL, "ready", "node", coder),
    repeat,
    workflow::edge(PAUSED, "closed", "terminal", CLOSED),
  ]);
  draft["root"]["terminals"]
    .as_array_mut()
    .unwrap()
    .push(json!({"key":CLOSED,"schema":response.reference()}));
  draft["data_schemas"]
    .as_array_mut()
    .unwrap()
    .extend([json!(input), json!(response)]);
  draft["limits"]["max_expanded_nodes"] = json!(64);
}
