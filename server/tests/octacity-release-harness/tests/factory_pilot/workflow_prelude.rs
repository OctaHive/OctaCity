//! Immutable optional phases for the packaged configurable pilot.
//! This fixture owns configuration only; node names never select product behavior.
use super::*;
use octacity_server_application::FactoryCodexUsagePolicy;
use octacity_server_factory::*;
use std::collections::BTreeMap;

// Creation and observation use separate fenced owner passes and PostgreSQL
// snapshot restoration. Their bounded window must include both owner turns.
pub(super) const LOCAL_GATE_WINDOW_MILLIS: u64 = 30_000;

fn key(name: &str) -> FactoryKey {
  FactoryKey::new(name).unwrap()
}
fn field(value: FlowValueSchema) -> FlowFieldSchema {
  FlowFieldSchema::required(value)
}
fn text(max_bytes: usize) -> FlowValueSchema {
  FlowValueSchema::String {
    min_bytes: 1,
    max_bytes,
  }
}
fn pointer(path: &str) -> Value {
  json!({"mapping":"pointer","path":path})
}
fn constant(value: Value) -> Value {
  json!({"mapping":"constant","value":value})
}
fn mapping(fields: Value) -> Value {
  json!({"value":{"mapping":"object","fields":fields}})
}

fn include_schemas(draft: &mut Value, schemas: [&FlowDataSchema; 2]) {
  let catalogue = draft["data_schemas"].as_array_mut().unwrap();
  for schema in schemas {
    let encoded = json!(schema);
    if let Some(existing) = catalogue
      .iter()
      .find(|entry| entry["reference"] == encoded["reference"])
    {
      assert_eq!(existing, &encoded, "an existing immutable schema cannot be replaced");
    } else {
      catalogue.push(encoded);
    }
  }
}

fn configuration_with_prelude_schemas() -> FactoryFlowConfiguration {
  let accepted = FlowDataSchema::new(key("probe-accepted"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let observation = FlowDataSchema::new(key("probe-observation"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let budget = BudgetLimit::new(1, LOCAL_GATE_WINDOW_MILLIS, 0, 0, 16 * 1024).unwrap();
  let mut gate = workflow::node(
    "probe-gate",
    "deterministic_gate",
    &accepted,
    budget,
    &FactoryPermissionSet::deny_all(),
    "accepted",
  );
  gate["gate"] = json!(
    FlowGateProgram::new(vec![], key("accepted"))
      .unwrap()
      .binding()
      .unwrap()
  );
  let mut draft = workflow::draft(
    "probe-gate",
    vec![gate],
    vec![workflow::edge("probe-gate", "accepted", "terminal", "accepted")],
    "accepted",
    &accepted,
    budget,
    &FactoryPermissionSet::deny_all(),
  );
  include_schemas(&mut draft, [&accepted, &observation]);
  let typed: octacity_server_application::FactoryFlowDefinitionDraft = serde_json::from_value(draft).unwrap();
  let root = typed.root;
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: root.input_schema,
    entry: root.entry,
    nodes: root.nodes,
    transitions: root.transitions,
    terminals: root.terminals,
    context_projections: root.context_projections,
    data_projections: root.data_projections,
    execution: root.execution,
  })
  .unwrap();
  FactoryFlowConfiguration::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    typed.limits,
    typed.data_schemas,
    typed.pools,
  )
  .expect("adding the prelude preserves the public schema catalogue contract")
}

#[test]
fn prelude_schema_extension_preserves_an_existing_accepted_contract() {
  assert_eq!(configuration_with_prelude_schemas().data_schemas.len(), 2);
}

fn published_root(details: &Value) -> &Value {
  let root = &details["flow"]["closure"]["root"];
  details["flow"]["closure"]["definitions"]
    .as_array()
    .unwrap()
    .iter()
    .find(|definition| definition["reference"] == *root)
    .expect("the public closure contains its exact root definition")
}

#[test]
fn published_prelude_root_uses_the_public_content_addressed_reference() {
  let publication = octacity_server_store::PublishedFactoryFlowDefinition {
    project_id: octacity_server_domain::ProjectId::generate(),
    flow: configuration_with_prelude_schemas(),
    published_at: octacity_server_domain::Timestamp::from_unix_millis(31).unwrap(),
  };
  let reference = json!(publication.flow.closure.root());
  let details = serde_json::to_value(publication).unwrap();
  let definition = published_root(&details);
  assert_eq!(definition["reference"], reference);
  assert_eq!(definition["entry"], "probe-gate");
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PreludePhase {
  Research,
  Requirements,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PreludeVariant {
  pub phases: Vec<PreludePhase>,
}
impl PreludeVariant {
  pub(super) fn new(phases: Vec<PreludePhase>) -> Self {
    assert!(phases.len() <= 2);
    assert!(
      phases
        .iter()
        .enumerate()
        .all(|(index, phase)| !phases[..index].contains(phase)),
      "a finite optional phase is declared at most once"
    );
    Self { phases }
  }
}

pub(super) struct Prelude {
  pub(super) model: models::InstalledModel,
  project: String,
  observation: FlowDataSchema,
  accepted: FlowDataSchema,
  names: [String; 6],
  research_name: String,
}
impl Prelude {
  pub(super) fn new(
    directory: &Path,
    release: &octacity_release_harness::InstalledRelease,
    resources: &resources::PilotResources,
    backend: &ReleaseBackend,
  ) -> Self {
    let installation = directory.join("readonly-prelude-model");
    fs::create_dir(&installation).unwrap();
    set_mode(&installation, 0o700);
    let mut model = models::InstalledModel::observer(&installation, release, resources, backend);
    let p = &mut model.profile;
    p.instructions = FactoryText::new(
      json!({"fixture_version":1,
      "checks":[{"path":"/octacity/protected/test-inventory.json","readable":false},
        {"path":"/octacity/protected/tests/contract.rs","readable":false}],
      "result":{"$input":"/payload/value"}})
      .to_string(),
    )
    .unwrap();
    p.toolchain.model = ImmutableReference::new(
      p.toolchain.model.identity().clone(),
      p.toolchain.model.version().clone(),
      FactoryDigest::content_sha256(p.instructions.as_str().as_bytes()),
    );
    p.usage_policy = FactoryCodexUsagePolicy::TokenRates {
      model: p.toolchain.model.clone(),
      input_micro_units_per_million: 1_000_000,
      cached_input_micro_units_per_million: 1_000_000,
      output_micro_units_per_million: 1_000_000,
    };
    let artifact = FlowValueSchema::Object {
      fields: BTreeMap::from([
        (
          key("artifact_id"),
          field(FlowValueSchema::String {
            min_bytes: 36,
            max_bytes: 36,
          }),
        ),
        (
          key("content_digest"),
          field(FlowValueSchema::String {
            min_bytes: 64,
            max_bytes: 64,
          }),
        ),
        (
          key("encoded_size"),
          field(FlowValueSchema::Integer {
            minimum: 1,
            maximum: 4 * 1024 * 1024,
          }),
        ),
      ]),
    };
    let fields = BTreeMap::from([
      (key("eligible"), field(FlowValueSchema::Boolean)),
      (key("risk"), field(text(16))),
      (
        key("work_kind"),
        field(FlowValueSchema::Enum {
          values: vec![json!("defect"), json!("feature_request")],
        }),
      ),
      (
        key("size"),
        field(FlowValueSchema::Enum {
          values: vec![json!("small"), json!("medium")],
        }),
      ),
      (key("task"), field(artifact.clone())),
      (key("acceptance"), field(artifact.clone())),
      (key("path"), field(text(128))),
      (key("expected_text"), field(text(1024))),
    ]);
    let observation = FlowDataSchema::new(
      key("prelude-public-observation"),
      key("v1"),
      FlowValueSchema::Object { fields: fields.clone() },
    )
    .unwrap();
    let mut accepted_fields = fields;
    accepted_fields.insert(key("criteria_artifact"), field(artifact));
    let accepted = FlowDataSchema::new(
      key("prelude-accepted-input"),
      key("v1"),
      FlowValueSchema::Object {
        fields: accepted_fields,
      },
    )
    .unwrap();
    let suffix = Uuid::new_v4().simple().to_string();
    Self {
      model,
      project: resources.project.clone(),
      observation,
      accepted,
      research_name: format!("research-observation-{suffix}"),
      names: [
        "eligibility-observation",
        "eligibility-policy",
        "classification-observation",
        "classification-policy",
        "requirements-observation",
        "accepted-requirements",
      ]
      .map(|name| format!("{name}-{suffix}")),
    }
  }
  fn phase_name(&self, phase: PreludePhase) -> &str {
    match phase {
      PreludePhase::Research => &self.research_name,
      PreludePhase::Requirements => &self.names[4],
    }
  }
  fn model_names(&self, variant: &PreludeVariant) -> Vec<&str> {
    [&self.names[0], &self.names[2]]
      .into_iter()
      .map(String::as_str)
      .chain(variant.phases.iter().map(|phase| self.phase_name(*phase)))
      .collect()
  }
  fn execution_order(&self, variant: &PreludeVariant) -> Vec<&str> {
    self.names[..4]
      .iter()
      .map(String::as_str)
      .chain(variant.phases.iter().map(|phase| self.phase_name(*phase)))
      .chain(std::iter::once(self.names[5].as_str()))
      .collect()
  }
  fn criteria_node(&self, variant: &PreludeVariant) -> &str {
    if variant.phases.contains(&PreludePhase::Requirements) {
      &self.names[4]
    } else {
      &self.names[2]
    }
  }
  /// Exact sum of the declared ordinary Job ceilings plus three local gate ceilings.
  pub(super) fn budget(&self) -> BudgetLimit {
    self.budget_for(&PreludeVariant::new(vec![PreludePhase::Requirements]))
  }
  pub(super) fn budget_for(&self, variant: &PreludeVariant) -> BudgetLimit {
    let jobs = self.model_names(variant).len() as u64;
    BudgetLimit::new(
      (jobs + 3) as u32,
      jobs * 120_000 + 3 * LOCAL_GATE_WINDOW_MILLIS,
      jobs * 100,
      jobs * 100,
      jobs * 8 * 1024 * 1024 + 3 * 16 * 1024,
    )
    .unwrap()
  }
  pub(super) fn accepted_schema(&self) -> &FlowDataSchema {
    &self.accepted
  }
  /// The author reads this exact real requirements publication as public Policy context.
  pub(super) fn criteria_context(&self) -> Value {
    json!({"entries":{"requirements":{"source_kind":"policy","source":{"value":{
      "mapping":"object","fields":{"kind":{"mapping":"constant","value":"artifact"},
      "value":{"mapping":"pointer","path":"/criteria_artifact"}}}}}}})
  }
  pub(super) fn install(&self, catalog: &mut Value, ceiling: &mut FactoryPermissionSet) {
    let p = &self.model.profile;
    catalog["codex"].as_array_mut().unwrap().push(
      json!({"toolchain":{"plugin":p.toolchain.plugin,"executable":p.toolchain.executable,"model":p.toolchain.model},
      "build_configuration":p.build_configuration,"instructions":p.instructions,"credentials":p.credentials,
      "credential_purpose":"evaluator","usage_policy":p.usage_policy,"pipeline_node":p.pipeline_node,
      "parameters":p.parameters,"project_permissions":p.project_permissions,
      "local_permissions":{"grants":p.local_permissions.grants(),"enforceable":PermissionCategory::ALL}}),
    );
    let refs = catalog["projects"][0]["references"].as_array_mut().unwrap();
    refs.push(json!({"kind":"node_capability","alias":"prelude-model","reference":p.toolchain.model}));
    refs.push(json!({"kind":"node_capability","alias":"prelude-executable","reference":p.toolchain.executable}));
    refs.push(
      json!({"kind":"secret_profile","alias":p.credentials.profile(FactoryCredentialPurpose::Evaluator),
      "reference":p.toolchain.model}),
    );
    let mut union = serde_json::to_value(&*ceiling).unwrap();
    let additional = serde_json::to_value(&p.project_permissions).unwrap();
    for category in ["executables", "commands", "secret_profiles"] {
      let values = union[category].as_array_mut().unwrap();
      for value in additional[category].as_array().unwrap() {
        if !values.contains(value) {
          values.push(value.clone());
        }
      }
    }
    *ceiling = serde_json::from_value(union).unwrap();
  }
  fn build(&self, name: &str, budget: BudgetLimit, binding: Value) -> Value {
    let p = &self.model.profile;
    let mut node = workflow::node(
      name,
      "reasoning",
      &self.observation,
      budget,
      &p.project_permissions,
      "observed",
    );
    node["input_binding"] = binding;
    node["build"] = json!(FlowBuildBinding {
      tool: p.toolchain.executable.clone(),
      plugin: p.toolchain.plugin.clone(),
      result_tool: None,
      capture_write_paths: None,
      capture_deliverables: None,
      model_or_tool: p.toolchain.model.clone(),
      task_digest: p.task_digest(),
      build_configuration: p.build_configuration.clone(),
      result_output: key("codex-run-result"),
      result_output_kind: EvidenceOutputKind::Report,
      result_outcome: key("observed"),
      evidence: vec![]
    });
    node
  }
  fn gate(
    &self,
    name: &str,
    budget: BudgetLimit,
    finish: bool,
    extra_predicates: Vec<Value>,
    criteria_node: Option<&str>,
  ) -> Value {
    let schema = if finish { &self.accepted } else { &self.observation };
    let mut node = workflow::node(
      name,
      "deterministic_gate",
      schema,
      budget,
      &FactoryPermissionSet::deny_all(),
      "accepted",
    );
    node["input_schema"] = json!(schema.reference());
    node["outcomes"]
      .as_array_mut()
      .unwrap()
      .push(json!({"key":"rejected","kind":"failure","schema":schema.reference()}));
    let mut fields = serde_json::Map::new();
    for name in [
      "eligible",
      "risk",
      "work_kind",
      "size",
      "task",
      "acceptance",
      "path",
      "expected_text",
    ] {
      fields.insert(name.into(), pointer(&format!("/observations/{name}")));
    }
    for name in ["risk", "task", "acceptance"] {
      fields.insert(name.into(), pointer(&format!("/{name}")));
    }
    if finish {
      fields.insert("criteria_artifact".into(), pointer("/publication"));
    }
    node["input_binding"] = json!({"sources":{
      "observations":{"source":"predecessor","view":"payload","path":""},
      "risk":{"source":"work","path":"/risk"},
      "task":{"source":"work","path":"/artifacts/task"},
      "acceptance":{"source":"work","path":"/artifacts/acceptance"},
      "publication":{"source":"predecessor","view":"metadata","path":"/build/outputs/codex-run-result/artifact"}},
      "mapping":mapping(Value::Object(fields))});
    if let Some(criteria_node) = criteria_node {
      let sources = node["input_binding"]["sources"].as_object_mut().unwrap();
      sources.insert(
        "observations".into(),
        json!({"source":"record","node":criteria_node,
        "outcome":"observed","view":"payload","path":""}),
      );
      sources.insert(
        "publication".into(),
        json!({"source":"record","node":criteria_node,
        "outcome":"observed","view":"metadata","path":"/build/outputs/codex-run-result/artifact"}),
      );
      sources.insert(
        "control".into(),
        json!({"source":"predecessor","view":"metadata","path":""}),
      );
    }
    // Immutable admission risk cannot be lowered by a model observation.
    let mut predicates = vec![
      json!({"operation":"compare","left":{"source":"path","path":"/eligible"},"comparison":"equal","right":{"source":"literal","value":true}}),
      json!({"operation":"compare","left":{"source":"path","path":"/risk"},"comparison":"equal","right":{"source":"literal","value":"low"}}),
    ];
    predicates.extend(extra_predicates);
    let program: FlowGateProgram = serde_json::from_value(
      json!({"rules":[{"when":{"operation":"all","predicates":predicates},"outcome":"accepted"}],
      "fallback":"rejected","default_output":{"value":{"mapping":"pointer","path":""}}}),
    )
    .unwrap();
    node["gate"] = json!(program.binding().unwrap());
    node
  }
  pub(super) async fn extend(
    &self,
    client: &Client,
    origin: &str,
    draft: &mut Value,
    acceptance: &str,
    author: &str,
  ) -> PublishedVariant {
    self
      .extend_variant(
        client,
        origin,
        draft,
        acceptance,
        Some(author),
        PreludeVariant::new(vec![PreludePhase::Requirements]),
      )
      .await
  }
  pub(super) async fn extend_variant(
    &self,
    client: &Client,
    origin: &str,
    draft: &mut Value,
    acceptance: &str,
    author: Option<&str>,
    variant: PreludeVariant,
  ) -> PublishedVariant {
    let p = &self.model.profile;
    let budget = BudgetLimit::new(1, 120_000, 100, 100, 8 * 1024 * 1024).unwrap();
    let gate_budget = BudgetLimit::new(1, LOCAL_GATE_WINDOW_MILLIS, 0, 0, 16 * 1024).unwrap();
    let aggregate = self.budget_for(&variant);
    let original = json!({"sources":{"task":{"source":"work","path":"/artifacts/task"},
      "acceptance":{"source":"work","path":"/artifacts/acceptance"},"risk":{"source":"work","path":"/risk"}},
      "mapping":mapping(json!({"eligible":constant(json!(true)),"risk":pointer("/risk"),
        "task":pointer("/task"),"acceptance":pointer("/acceptance"),
        "work_kind":constant(json!("feature_request")),"size":constant(json!("small")),
        "path":constant(json!("public/facts.txt")),"expected_text":constant(json!("facts"))}))});
    let previous = json!({"sources":{"previous":{"source":"predecessor","view":"payload","path":""}},
      "mapping":{"value":{"mapping":"pointer","path":"/previous"}}});
    let mut nodes = vec![
      self.build(&self.names[0], budget, original.clone()),
      self.gate(&self.names[1], gate_budget, false, vec![], None),
      self.build(&self.names[2], budget, previous.clone()),
      self.gate(&self.names[3], gate_budget, false, vec![
        json!({"operation":"compare","left":{"source":"path","path":"/work_kind"},"comparison":"equal","right":{"source":"literal","value":"feature_request"}}),
        json!({"operation":"compare","left":{"source":"path","path":"/size"},"comparison":"equal","right":{"source":"literal","value":"small"}}),
      ], None),
    ];
    for phase in &variant.phases {
      nodes.push(self.build(self.phase_name(*phase), budget, previous.clone()));
    }
    nodes.push(self.gate(
      &self.names[5],
      gate_budget,
      true,
      vec![],
      Some(self.criteria_node(&variant)),
    ));
    let mut edges = vec![];
    let models = self.model_names(&variant);
    for pair in self.execution_order(&variant).windows(2) {
      let outcome = if models.contains(&pair[0]) {
        "observed"
      } else {
        "accepted"
      };
      edges.push(workflow::edge(pair[0], outcome, "node", pair[1]));
    }
    edges.push(workflow::edge(&self.names[5], "accepted", "terminal", "returned"));
    for gate in [&self.names[1], &self.names[3], &self.names[5]] {
      edges.push(workflow::edge(
        gate,
        "rejected",
        "terminal",
        if gate == &self.names[5] {
          "blocked-accepted"
        } else {
          "blocked-observation"
        },
      ));
    }
    let mut child = workflow::draft(
      &self.names[0],
      nodes,
      edges,
      "returned",
      &self.accepted,
      aggregate,
      &p.project_permissions,
    );
    child["root"]["input_schema"] = json!(self.observation.reference());
    child["root"]["terminals"].as_array_mut().unwrap().extend([
      json!({"key":"blocked-accepted","schema":self.accepted.reference()}),
      json!({"key":"blocked-observation","schema":self.observation.reference()}),
    ]);
    child["data_schemas"]
      .as_array_mut()
      .unwrap()
      .push(json!(self.observation));
    let typed: octacity_server_application::FactoryFlowDefinitionDraft =
      serde_json::from_value(child.clone()).expect("typed prelude child draft");
    private_file(
      Path::new(&self.model.executable).parent().unwrap(),
      "prelude-child.json",
      &child.to_string(),
    );
    let root = typed.root;
    let definition = FlowDefinition::new(FlowDefinitionInput {
      id: FlowDefinitionId::generate(),
      version: FlowDefinitionVersion::INITIAL,
      input_schema: root.input_schema,
      entry: root.entry,
      nodes: root.nodes,
      transitions: root.transitions,
      terminals: root.terminals,
      context_projections: root.context_projections,
      data_projections: root.data_projections,
      execution: root.execution,
    })
    .expect("prelude root satisfies the public graph contract");
    FactoryFlowConfiguration::new(
      PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
      typed.limits,
      typed.data_schemas,
      typed.pools,
    )
    .expect("prelude satisfies the public closure and schema contract");
    let child = post_management(
      client,
      origin,
      "/api/v1/flow-definitions",
      &format!("prelude-{}", Uuid::new_v4()),
      json!({"id":Uuid::new_v4(),"project_id":self.project,"definition":child}),
    )
    .await;
    let call = format!("prelude-call-{}", Uuid::new_v4().simple());
    let mut node = workflow::node(
      &call,
      "subflow_call",
      &self.accepted,
      aggregate,
      &p.project_permissions,
      "returned",
    );
    node["input_schema"] = json!(self.observation.reference());
    node["outcomes"].as_array_mut().unwrap().extend([
      json!({"key":"blocked-accepted","kind":"failure","schema":self.accepted.reference()}),
      json!({"key":"blocked-observation","kind":"failure","schema":self.observation.reference()}),
    ]);
    node["subflow"] = child["resource"]["flow"]["closure"]["root"].clone();
    node["input_binding"] = original;
    draft["root"]["entry"] = json!(call);
    draft["root"]["input_schema"] = json!(self.observation.reference());
    draft["root"]["nodes"].as_array_mut().unwrap().push(node);
    draft["root"]["transitions"]
      .as_array_mut()
      .unwrap()
      .push(workflow::edge(&call, "returned", "node", acceptance));
    for (outcome, schema) in [
      ("blocked-accepted", &self.accepted),
      ("blocked-observation", &self.observation),
    ] {
      draft["root"]["transitions"]
        .as_array_mut()
        .unwrap()
        .push(workflow::edge(&call, outcome, "terminal", outcome));
      draft["root"]["terminals"]
        .as_array_mut()
        .unwrap()
        .push(json!({"key":outcome,"schema":schema.reference()}));
    }
    for node in draft["root"]["nodes"].as_array_mut().unwrap() {
      if node["key"] == acceptance || author.is_some_and(|author| node["key"] == author) {
        node["input_schema"] = json!(self.accepted.reference());
        node["input_binding"] = json!({"sources":{"previous":{"source":"predecessor","view":"payload","path":""}},
          "mapping":{"value":{"mapping":"pointer","path":"/previous"}}});
        if node["key"] == acceptance {
          node["outcomes"][0]["schema"] = json!(self.accepted.reference());
        } else {
          node["context_binding"] = self.criteria_context();
        }
      }
    }
    include_schemas(draft, [&self.accepted, &self.observation]);
    PublishedVariant {
      call,
      child_reference: child["resource"]["flow"]["closure"]["root"].clone(),
      variant,
    }
  }
  /// Assert the actual REST-published grammar before admitting a probe Run.
  /// These are independently specified expected phases, not inferred from execution.
  pub(super) fn assert_variant_definition(&self, details: &Value, variant: &PreludeVariant) {
    let definition = published_root(details);
    let nodes = definition["nodes"].as_array().unwrap();
    let mut actual_order = vec![];
    let mut current = definition["entry"].as_str().unwrap();
    loop {
      assert!(
        !actual_order.contains(&current),
        "the finite prelude must not revisit a node"
      );
      actual_order.push(current);
      let node = nodes.iter().find(|node| node["key"] == current).unwrap();
      let outcome = if node["kind"] == "reasoning" {
        "observed"
      } else {
        "accepted"
      };
      let edge = definition["transitions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|edge| edge["predecessor"] == current && edge["outcome"] == outcome)
        .unwrap();
      if edge["target"]["kind"] == "terminal" {
        assert_eq!(edge["target"]["key"], "returned");
        break;
      }
      current = edge["target"]["key"].as_str().unwrap();
    }
    assert_eq!(actual_order.len(), 5 + variant.phases.len());
    assert_eq!(
      &actual_order[..4],
      self.names[..4].iter().map(String::as_str).collect::<Vec<_>>()
    );
    assert_eq!(actual_order.last().copied(), Some(self.names[5].as_str()));
    let actual_optional = actual_order[4..actual_order.len() - 1]
      .iter()
      .map(|name| {
        if *name == self.research_name {
          PreludePhase::Research
        } else {
          assert_eq!(*name, self.names[4]);
          PreludePhase::Requirements
        }
      })
      .collect::<Vec<_>>();
    assert_eq!(
      actual_optional, variant.phases,
      "public transitions retain the caller-declared optional order"
    );
    assert_eq!(nodes.len(), 5 + variant.phases.len());
    let models = nodes
      .iter()
      .filter(|node| node["kind"] == "reasoning")
      .collect::<Vec<_>>();
    assert_eq!(models.len(), 2 + variant.phases.len());
    for node in models {
      assert_eq!(node["build"]["task_digest"], json!(self.model.profile.task_digest()));
      assert_eq!(node["permissions"], json!(self.model.profile.project_permissions));
    }
    for phase in [PreludePhase::Research, PreludePhase::Requirements] {
      assert_eq!(
        nodes.iter().any(|node| node["key"] == self.phase_name(phase)),
        variant.phases.contains(&phase)
      );
    }
    let final_gate = nodes.iter().find(|node| node["key"] == self.names[5]).unwrap();
    // Independent expected producer: a present requirements result owns criteria,
    // otherwise phase-two model observations do; trailing research never does.
    let expected_criteria = if variant.phases.contains(&PreludePhase::Requirements) {
      self.names[4].as_str()
    } else {
      self.names[2].as_str()
    };
    for source in ["observations", "publication"] {
      assert_eq!(final_gate["input_binding"]["sources"][source]["source"], "record");
      assert_eq!(
        final_gate["input_binding"]["sources"][source]["node"],
        expected_criteria
      );
    }
  }
  /// Public lineage checks; expected criteria come from the installed fixture contract.
  pub(super) async fn verify_variant(
    &self,
    client: &Client,
    origin: &str,
    run: &str,
    accepted: &Value,
    original_artifacts: &Value,
    variant: &PreludeVariant,
  ) {
    let attempts = workflow::diagnostic(client, origin, run, "node-attempts").await;
    let completions = workflow::diagnostic(client, origin, run, "node-completions").await;
    for name in self.execution_order(variant) {
      assert!(
        attempts["items"]
          .as_array()
          .unwrap()
          .iter()
          .any(|a| a["record"]["node_key"] == name),
        "declared prelude node executed"
      );
      assert!(
        completions["items"]
          .as_array()
          .unwrap()
          .iter()
          .any(|a| a["record"]["node_key"] == name),
        "declared prelude node completed"
      );
    }
    let builds = workflow::diagnostic(client, origin, run, "builds").await;
    let mut jobs = std::collections::BTreeSet::new();
    for name in self.model_names(variant) {
      let attempt = attempts["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|attempt| &attempt["record"])
        .find(|attempt| attempt["node_key"] == name)
        .unwrap();
      let execution = builds["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| &item["record"])
        .find(|record| record["node_attempt_id"] == attempt["id"])
        .unwrap();
      let ordinary = get_json(
        client,
        format!("{origin}/api/v1/attempts/{}", string(execution, "attempt_id")),
      )
      .await;
      assert_eq!(ordinary["jobs"].as_array().unwrap().len(), 1);
      assert_eq!(ordinary["jobs"][0]["state"], "succeeded");
      assert!(
        jobs.insert(ordinary["jobs"][0]["id"].as_str().unwrap().to_owned()),
        "distinct readonly Jobs"
      );
    }
    assert_eq!(jobs.len(), self.model_names(variant).len());
    for phase in [PreludePhase::Research, PreludePhase::Requirements] {
      if !variant.phases.contains(&phase) {
        assert!(
          !attempts["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["record"]["node_key"] == self.phase_name(phase)),
          "undeclared optional role must not run"
        );
      }
    }
    // Select the criteria-producing ordinary Build independently of execution order.
    let attempt = attempts["items"]
      .as_array()
      .unwrap()
      .iter()
      .map(|row| &row["record"])
      .find(|row| row["node_key"] == self.criteria_node(variant))
      .unwrap();
    let execution = builds["items"]
      .as_array()
      .unwrap()
      .iter()
      .map(|row| &row["record"])
      .find(|row| row["node_attempt_id"] == attempt["id"])
      .unwrap();
    let published = get_json(
      client,
      format!(
        "{origin}/api/v1/builds/{}/artifacts?limit=100",
        string(execution, "build_id")
      ),
    )
    .await;
    let criterion = published["items"]
      .as_array()
      .unwrap()
      .iter()
      .find(|item| item["name"] == "codex-run-result")
      .unwrap();
    assert_eq!(
      accepted["criteria_artifact"],
      json!({"artifact_id":criterion["id"],
      "content_digest":criterion["sha256"],"encoded_size":criterion["size_bytes"]})
    );
    assert_eq!(accepted["task"], original_artifacts["task"]);
    assert_eq!(accepted["acceptance"], original_artifacts["acceptance"]);
    assert_eq!(accepted["path"], "public/facts.txt");
    assert_eq!(accepted["expected_text"], "facts");
    assert_eq!(accepted["risk"], "low");
    assert_eq!(accepted["eligible"], true);
    let artifact: &Value = &accepted["criteria_artifact"];
    let bytes = workflow_author::download_artifact(client, origin, artifact["artifact_id"].as_str().unwrap()).await;
    assert_eq!(
      FactoryDigest::content_sha256(&bytes).to_string(),
      artifact["content_digest"].as_str().unwrap()
    );
    assert_eq!(bytes.len() as u64, artifact["encoded_size"].as_u64().unwrap());
    let report: Value = serde_json::from_slice(&bytes).unwrap();
    for name in ["task", "acceptance", "path", "expected_text"] {
      assert_eq!(
        report["structured_result"][name], accepted[name],
        "accepted criteria retain the actual ordinary result"
      );
    }
  }
}

/// Returned by actual child publication; independent of any current mutable choice.
#[derive(Clone)]
pub(super) struct PublishedVariant {
  pub call: String,
  pub child_reference: Value,
  pub variant: PreludeVariant,
}
/// Snapshot of exact immutable publication bytes and the owning Run's actual refs.
#[derive(Debug, Eq, PartialEq)]
pub(super) struct CaptureVariantClosure {
  root_version: Value,
  child_version: Value,
  run_bindings: Vec<Value>,
}
impl CaptureVariantClosure {
  pub(super) async fn capture(
    client: &Client,
    origin: &str,
    run: &str,
    root_reference: &Value,
    published: &PublishedVariant,
  ) -> Self {
    let exact = |reference: &Value| {
      format!(
        "{origin}/api/v1/flow-definitions/{}/versions/{}",
        reference["id"].as_str().unwrap(),
        reference["version"].as_u64().unwrap()
      )
    };
    let root_version = get_json(client, exact(root_reference)).await;
    let child_version = get_json(client, exact(&published.child_reference)).await;
    assert_eq!(root_version["flow"]["closure"]["root"], *root_reference);
    assert_eq!(child_version["flow"]["closure"]["root"], published.child_reference);
    let flows = workflow::diagnostic(client, origin, run, "flow-runs").await;
    let run_bindings = flows["items"]
      .as_array()
      .unwrap()
      .iter()
      .map(|row| row["record"].clone())
      .filter(|row| row["definition"] == *root_reference || row["definition"] == published.child_reference)
      .collect::<Vec<_>>();
    assert_eq!(
      run_bindings.len(),
      2,
      "root and exact prelude child were actually admitted"
    );
    Self {
      root_version,
      child_version,
      run_bindings,
    }
  }
}
