//! Real installed Shadow routing and a separate original-source tool Job.
use super::*;
use octacity_server_application::{
  FactoryCodexToolRiskPolicy, FactoryDecisionSignalNodeBinding, FactoryDecisionSignalRequestPlan,
};
use octacity_server_factory::*;
use std::collections::BTreeMap;

fn key(name: &str) -> FactoryKey {
  FactoryKey::new(name).unwrap()
}

fn tool_budget(policy: FactoryCodexToolRiskPolicy) -> BudgetLimit {
  let attempts = policy.signal_allowance().max_attempts().checked_add(1).unwrap();
  BudgetLimit::new(attempts, 120_000, 10000, 10000, 8 * 1024 * 1024).unwrap()
}

#[test]
fn tool_budget_retains_a_model_attempt_after_the_independent_signal_reservation() {
  let policy: FactoryCodexToolRiskPolicy = serde_json::from_value(json!({
    "policy_digest":FactoryDigest::content_sha256(b"two declared provider calls"),
    "signal_allowance":BudgetLimit::new(2, 20000, 2000, 2000, 32768).unwrap()
  })).unwrap();
  let model = policy.model_budget(tool_budget(policy)).expect("independent model share remains available");
  assert_eq!(model.max_attempts(), 1);
  assert!(model.max_tokens() > 0);
}

// Each operator question pack names its actual immutable content, independently
// of request-level canonicalization and the consuming policy identity.
fn pin_questions(plan: &mut FactoryDecisionSignalRequestPlan, identity: &str) {
  let bytes = serde_json::to_vec(&plan.questions).unwrap();
  let question_set = ImmutableReference::new(
    key(identity),
    key("v1"),
    FactoryDigest::sha256("octacity.pilot.shadow-question-pack.v1", &[&bytes]),
  );
  let old = &plan.profile;
  plan.profile = DecisionSignalProfile::new(DecisionSignalProfileDefinition {
    purpose: old.purpose(),
    provider: old.provider().clone(),
    adapter: old.adapter().clone(),
    model: old.model().clone(),
    question_set,
    policy: old.policy().clone(),
    mode: old.mode(),
    fallback: old.fallback(),
    budget: old.budget(),
    routes: old.routes().cloned(),
  })
  .unwrap();
}

pub(super) struct ShadowNodes<'a> {
  pub prelude: &'a str,
  pub acceptance: &'a str,
  pub author: &'a str,
  pub coder: &'a str,
  pub routing: &'a str,
  pub tool: &'a str,
}
pub(super) struct Shadow {
  pub model: models::InstalledModel,
  pub tools: workflow_tools::InstalledTools,
  routing: FactoryDecisionSignalNodeBinding,
  state: FlowDataSchema,
  receipt: FlowDataSchema,
  tool_result: FlowDataSchema,
}
impl Shadow {
  pub(super) async fn new(
    directory: &Path,
    release: &octacity_release_harness::InstalledRelease,
    resources: &resources::PilotResources,
    backend: &ReleaseBackend,
  ) -> Self {
    let install = directory.join("shadow-tool-model");
    fs::create_dir(&install).unwrap();
    set_mode(&install, 0o700);
    let mut model = models::InstalledModel::observer(&install, release, resources, backend);
    model.profile.credentials = FactoryCredentialProfiles::new(
      key("shadow-tool-model"),
      key("shadow-unselected-evaluator"),
      key("shadow-unselected-source"),
      key("shadow-unselected-delivery"),
    )
    .unwrap();
    let mut tools = workflow_tools::InstalledTools::install(&install, &mut model, backend).await;
    // Existing installed fixture recommends the final label. Deliberately disagree
    // with the independently authorized rooted-read baseline without controlling it.
    let choices = DecisionSignalChoices::try_new(vec![key("a-allow"), key("z-deny")]).unwrap();
    let mut tool_plan = signals::plan(
      DecisionSignalPurpose::ToolRisk,
      tools.binding.plan.state_schema.clone(),
      tools.binding.plan.state_projection.clone(),
      choices.as_slice(),
    );
    tool_plan.profile = tools.binding.plan.profile.clone(); // preserves Shadow + fallback Deny
    pin_questions(&mut tool_plan, "shadow-tool-question-pack");
    tools.binding.plan = tool_plan;
    tools.binding.mapping = ToolRiskChoiceMapping::try_new(
      &choices,
      vec![
        (key("a-allow"), DecisionSignalDisposition::Allow),
        (key("z-deny"), DecisionSignalDisposition::Deny),
      ],
    )
    .unwrap();
    model.profile.tool_risk = Some(FactoryCodexToolRiskPolicy::from_binding(&tools.binding).unwrap());
    assert!(model.profile.capture.is_none());
    assert!(model.profile.candidate_capture_tool.is_none());
    assert_eq!(model.profile.consumer, FactoryCredentialConsumer::ToolReasoning);
    assert_eq!(
      model.profile.project_permissions.secret_profiles().collect::<Vec<_>>(),
      [model.profile.credentials.profile(FactoryCredentialPurpose::Model)]
    );
    let state = FlowDataSchema::new(
      key("public-shadow-state"),
      key("v1"),
      FlowValueSchema::Object {
        fields: ["risk", "work_kind", "size"]
          .into_iter()
          .map(|name| {
            (
              key(name),
              FlowFieldSchema::required(FlowValueSchema::String {
                min_bytes: 1,
                max_bytes: 32,
              }),
            )
          })
          .collect::<BTreeMap<_, _>>(),
      },
    )
    .unwrap();
    let projection = FlowDataMapping::new(FlowValueMapping::Object {
      fields: ["risk", "work_kind", "size"]
        .into_iter()
        .map(|name| {
          (
            key(name),
            FlowValueMapping::Pointer {
              path: format!("/{name}"),
            },
          )
        })
        .collect(),
    })
    .unwrap();
    let routes = vec![key("a-baseline"), key("z-recommendation")];
    let mut plan = signals::plan(DecisionSignalPurpose::Routing, state.clone(), projection, &routes);
    pin_questions(&mut plan, "shadow-routing-question-pack");
    assert_eq!(plan.capability, tools.binding.plan.capability);
    assert_ne!(plan.profile.question_set(), tools.binding.plan.profile.question_set());
    let routing = signals::routing_binding(
      resources.project.parse().unwrap(),
      key("installed-shadow-routing"),
      key("observed"),
      plan,
      FlowDataMapping::new(FlowValueMapping::Constant { value: json!(true) }).unwrap(),
      routes[0].clone(),
      NetworkHost::new("localhost").unwrap(),
    );
    let receipt = FlowDataSchema::new(key("shadow-observed"), key("v1"), FlowValueSchema::Boolean).unwrap();
    let tool_result = FlowDataSchema::new(
      key("original-source-tool-result"),
      key("v1"),
      FlowValueSchema::Enum {
        values: vec![json!("baseline")],
      },
    )
    .unwrap();
    Self {
      model,
      tools,
      routing,
      state,
      receipt,
      tool_result,
    }
  }
  /// One private Server connector file serves both typed purposes. No JEV secret
  /// goes into the ordinary Agent's credential profile map.
  pub(super) fn connectors(&self, directory: &Path) -> PathBuf {
    self.tools.connectors(directory)
  }
  pub(super) fn install(&self, catalog: &mut Value) {
    let p = &self.model.profile;
    let entry = json!({"toolchain":{"plugin":p.toolchain.plugin,"executable":p.toolchain.executable,
      "model":p.toolchain.model},"build_configuration":p.build_configuration,
      "instructions":p.instructions,"credentials":p.credentials,"credential_purpose":"model",
      "usage_policy":p.usage_policy,"pipeline_node":p.pipeline_node,"parameters":p.parameters,
      "project_permissions":p.project_permissions,"local_permissions":{"grants":p.local_permissions.grants(),
      "enforceable":PermissionCategory::ALL}});
    // The existing helper's index-zero contract is local to this single-model
    // patch. It never rewrites the coding profile in the combined catalog.
    let mut patch = json!({"projects":[{"references":[]}],"codex":[entry]});
    self.tools.catalog(&mut patch, p);
    catalog["codex"].as_array_mut().unwrap().push(patch["codex"][0].clone());
    for name in ["tool_risk", "signals"] {
      if catalog[name].is_null() {
        catalog[name] = json!([]);
      }
    }
    catalog["tool_risk"]
      .as_array_mut()
      .unwrap()
      .extend(patch["tool_risk"].as_array().unwrap().iter().cloned());
    catalog["signals"]
      .as_array_mut()
      .unwrap()
      .push(signals::routing_entry(&self.routing));
    let refs = catalog["projects"][0]["references"].as_array_mut().unwrap();
    refs.extend(patch["projects"][0]["references"].as_array().unwrap().iter().cloned());
    for (alias, reference) in [
      ("shadow-tool-model", &p.toolchain.model),
      ("shadow-tool-executable", &p.toolchain.executable),
    ] {
      refs.push(json!({"kind":"node_capability","alias":alias,"reference":reference}));
    }
    for (index, capability) in p
      .project_permissions
      .executables()
      .chain(p.project_permissions.tools())
      .enumerate()
    {
      let reference = json!(capability);
      if !refs
        .iter()
        .any(|entry| entry["kind"] == "node_capability" && entry["reference"] == reference)
      {
        refs.push(
          json!({"kind":"node_capability","alias":format!("shadow-confined-capability-{index}"),"reference":reference}),
        );
      }
    }
    refs.push(
      json!({"kind":"secret_profile","alias":p.credentials.profile(FactoryCredentialPurpose::Model),
      "reference":p.toolchain.model}),
    );
  }
  pub(super) fn extend(&self, draft: &mut Value, nodes: ShadowNodes<'_>) {
    let mut routing = workflow::node(
      nodes.routing,
      "decision_signal",
      &self.receipt,
      self.routing.plan.call_budget,
      &self.routing.project_permissions,
      "observed",
    );
    routing["input_schema"] = json!(self.state.reference());
    routing["action"] = json!(self.routing.action);
    routing["input_binding"] = json!({"sources":{"contract":{"source":"predecessor","view":"payload","path":""}},
      "mapping":{"value":{"mapping":"object","fields":{
        "risk":{"mapping":"pointer","path":"/contract/risk"},
        "work_kind":{"mapping":"pointer","path":"/contract/work_kind"},
        "size":{"mapping":"pointer","path":"/contract/size"}}}}});
    let p = &self.model.profile;
    let mut tool = workflow::node(
      nodes.tool,
      "reasoning",
      &self.tool_result,
      tool_budget(p.tool_risk.unwrap()),
      &p.project_permissions,
      "checked",
    );
    tool["input_schema"] = json!(self.receipt.reference());
    tool["input_binding"] = json!({"sources":{"author":{"source":"predecessor","view":"metadata","path":""}},
      "mapping":{"value":{"mapping":"constant","value":true}}});
    tool["build"] = json!(FlowBuildBinding {
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
      result_outcome: key("checked"),
      evidence: vec![]
    });
    for row in draft["root"]["nodes"].as_array_mut().unwrap() {
      if row["key"] == nodes.acceptance {
        row["input_binding"] = json!({"sources":{
          "contract":{"source":"record","node":nodes.prelude,"outcome":"returned","view":"payload","path":""},
          "routing":{"source":"predecessor","view":"metadata","path":""}},
          "mapping":{"value":{"mapping":"pointer","path":"/contract"}}});
      }
    }
    let edges = draft["root"]["transitions"].as_array_mut().unwrap();
    for edge in edges.iter_mut() {
      if edge["predecessor"] == nodes.prelude && edge["target"]["key"] == nodes.acceptance {
        edge["target"]["key"] = json!(nodes.routing);
      }
      if edge["predecessor"] == nodes.author && edge["target"]["key"] == nodes.coder {
        edge["target"]["key"] = json!(nodes.tool);
      }
    }
    edges.extend([
      workflow::edge(nodes.routing, "observed", "node", nodes.acceptance),
      workflow::edge(nodes.tool, "checked", "node", nodes.coder),
    ]);
    draft["root"]["nodes"].as_array_mut().unwrap().extend([routing, tool]);
    draft["data_schemas"].as_array_mut().unwrap().extend([
      json!(self.state),
      json!(self.receipt),
      json!(self.tool_result),
    ]);
  }
  /// Call before and after restart; compare returned exact bytes outside this helper.
  pub(super) async fn verify(
    &self,
    postgres: &str,
    run: &str,
    tool_record: &Value,
  ) -> (Vec<(Uuid, Vec<u8>, Vec<u8>)>, Vec<(Vec<u8>, Vec<u8>)>) {
    assert_eq!(tool_record["payload"]["value"], "baseline");
    assert_eq!(tool_record["input"]["context"]["entries"].as_array().unwrap().len(), 0);
    let pool = sqlx::PgPool::connect(postgres).await.unwrap();
    let run = run.parse::<Uuid>().unwrap();
    let requests:Vec<(Uuid,Vec<u8>,Vec<u8>)>=sqlx::query_as(
      "SELECT request_id,request_digest,document FROM factory_decision_signal_requests WHERE run_id=$1 ORDER BY request_id")
      .bind(run).fetch_all(&pool).await.unwrap();
    let receipts: Vec<(Vec<u8>, Vec<u8>)> = sqlx::query_as(
      "SELECT receipt_digest,document FROM factory_decision_signal_receipts WHERE run_id=$1 ORDER BY request_id",
    )
    .bind(run)
    .fetch_all(&pool)
    .await
    .unwrap();
    pool.close().await;
    assert_eq!(requests.len(), 2, "both typed purposes belong to this same real Run");
    assert_eq!(receipts.len(), 2);
    for (id, digest, bytes) in &requests {
      let request: DecisionSignalProviderRequest = serde_json::from_slice(bytes).unwrap();
      request.validate_integrity().unwrap();
      assert_eq!(request.digest().as_bytes().as_slice(), digest.as_slice());
      assert_eq!(request.id().as_uuid(), *id);
    }
    let captured = self.tools.fixture.captured();
    assert_eq!(
      captured.len(),
      2,
      "one POST per exact canonical request, including hook/read deduplication"
    );
    let mut seen = (false, false);
    for (digest, bytes) in &receipts {
      let receipt: DecisionSignalReceipt = serde_json::from_slice(bytes).unwrap();
      receipt.validate_integrity().unwrap();
      assert_eq!(receipt.receipt_digest().as_bytes().as_slice(), digest.as_slice());
      assert_eq!(receipt.mode(), DecisionSignalMode::Shadow);
      let consumption = receipt.consumption();
      assert_eq!(consumption.kind(), DecisionSignalConsumptionKind::Shadow);
      assert_eq!(consumption.final_disposition(), consumption.baseline());
      let expected = match consumption.baseline() {
        DecisionSignalDisposition::Allow => {
          assert_eq!(receipt.question_set(), self.tools.binding.plan.profile.question_set());
          seen.1 = true;
          DecisionSignalDisposition::Deny
        }
        DecisionSignalDisposition::Route(route) => {
          assert_eq!(route.as_str(), "a-baseline");
          assert_eq!(receipt.question_set(), self.routing.plan.profile.question_set());
          seen.0 = true;
          DecisionSignalDisposition::Route(key("z-recommendation"))
        }
        _ => panic!("undeclared pilot baseline"),
      };
      assert_eq!(consumption.recommendation(), Some(&expected));
      let id = receipt.request_id().to_string();
      let request = captured
        .iter()
        .find(|request| request.idempotency_key.as_deref() == Some(id.as_str()))
        .unwrap();
      assert!(request.credential_matched);
      match consumption.baseline() {
        DecisionSignalDisposition::Allow => assert_eq!(request.body["state"], json!({})),
        _ => assert_eq!(
          request.body["state"],
          json!({"risk":"low","work_kind":"feature_request","size":"small"})
        ),
      }
      assert!(
        !serde_json::to_string(&request.body)
          .unwrap()
          .contains(workflow_tools::PUBLIC_FILE)
      );
    }
    assert_eq!(seen, (true, true));
    (requests, receipts)
  }
}
