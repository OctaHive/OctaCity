//! Authored local classification and independent Issue input for the packaged journey.
//! Reuses Project discovery; the fixture never invents a producer record or an Issue origin.
use super::*;
use octacity_server_factory::*;
use std::os::unix::fs::MetadataExt;

#[cfg(test)]
#[path = "workflow_issue_snapshot_tests.rs"]
mod tests;

const PRINCIPAL: &str = "pilot.issue-classification";

fn writer() -> ImmutableReference {
  ImmutableReference::new(
    "pilot.issue-classification".parse().unwrap(),
    "v1".parse().unwrap(),
    FactoryDigest::content_sha256(PRINCIPAL.as_bytes()),
  )
}

fn permissions() -> FactoryPermissionSet {
  FactoryPermissionSet::try_new(FactoryPermissionDraft {
    plugins: vec![writer()],
    ..Default::default()
  })
  .unwrap()
}

pub(super) fn install(catalog: &mut Value, ceiling: &mut FactoryPermissionSet) {
  catalog["issue_classification_actions"] = json!([{"identity":writer(),"principal":PRINCIPAL}]);
  catalog["projects"][0]["references"]
    .as_array_mut()
    .unwrap()
    .push(json!({
      "kind":"node_capability","alias":"issue-classification","reference":writer(),
      "metadata":{"node_kinds":["trusted_action"],"signal_purposes":[],"enforcement":[],"compatible_choices":[],
        "issue_classification":{"policy":"triage","size_labels":{}}}
    }));
  workflow_author::include_permissions(ceiling, &permissions());
}

pub(super) fn configure(config: &Path) {
  let mut document: toml::Value = toml::from_str(&fs::read_to_string(config).unwrap()).unwrap();
  let management = toml::Value::try_from(json!({
    "socket_path":config.parent().unwrap().join("issue.sock"),
    "peer_uid":fs::metadata(config).unwrap().uid(),"principal":PRINCIPAL
  }))
  .unwrap();
  document
    .as_table_mut()
    .unwrap()
    .insert("integration_management".into(), management);
  fs::write(config, toml::to_string(&document).unwrap()).unwrap();
}

pub(super) async fn contract(client: &Client, origin: &str, project: &str) -> Value {
  post_management(
    client,
    origin,
    &format!("/api/v1/projects/{project}/issue-property-definitions"),
    "snapshot-project-components",
    json!({"expected_version":0,"definition":{
      "labels":[],"properties":[],"components":[{"id":"api","name":"API","retired":false}]
    }}),
  )
  .await;
  read_contract(client, origin, project).await
}

pub(super) async fn read_contract(client: &Client, origin: &str, project: &str) -> Value {
  let capabilities = get_json(
    client,
    format!("{origin}/api/v1/projects/{project}/factory-authoring-capabilities"),
  )
  .await;
  let choice = capabilities["issue_classification"].clone();
  assert!(
    choice["field_contract"].is_object(),
    "the Project publishes one reusable field contract: {capabilities}"
  );
  choice
}

pub(super) fn classify(draft: &mut Value, choice: &Value, budget: BudgetLimit, model: &FactoryPermissionSet) {
  let schema: FlowDataSchema = serde_json::from_value(choice["field_contract"]["schema"].clone()).unwrap();
  let receipt: FlowDataSchema = serde_json::from_value(choice["receipt_schema"].clone()).unwrap();
  let action_permissions = permissions();
  let receipt_bytes = choice["receipt_output_bytes"].as_u64().unwrap();
  let action_budget = BudgetLimit::new(1, workflow_prelude::LOCAL_GATE_WINDOW_MILLIS, 0, 0, receipt_bytes).unwrap();
  let total = BudgetLimit::new(
    2,
    budget.max_elapsed_millis() + action_budget.max_elapsed_millis(),
    budget.max_tokens(),
    budget.max_cost_micro_units(),
    budget.max_output_bytes() + receipt_bytes,
  )
  .unwrap();
  let mut ceiling = model.clone();
  workflow_author::include_permissions(&mut ceiling, &action_permissions);
  let mut action = workflow::node(
    "apply-classification",
    "trusted_action",
    &receipt,
    action_budget,
    &action_permissions,
    "applied",
  );
  action["input_schema"] = json!(schema.reference());
  action["input_binding"] = json!({"sources":{"assessment":{"source":"record","node":"assess","outcome":"eligible","view":"payload","path":""}},
    "mapping":{"value":{"mapping":"pointer","path":"/assessment"}}});
  action["action"] = json!({"plugin":choice["plugin"],"action":"apply_issue_classification","parameters":{
    "source_node":"assess","source_outcome":"eligible","outcome":"applied","policy":choice["policy"],
    "size_labels":{},"fields":choice["field_contract"]["selection"]
  }});
  draft["root"]["nodes"].as_array_mut().unwrap().push(action);
  draft["root"]["transitions"] = json!([
    workflow::edge("assess", "eligible", "node", "apply-classification"),
    workflow::edge("apply-classification", "applied", "terminal", "triaged")
  ]);
  draft["root"]["terminals"] = json!([{"key":"triaged","schema":receipt.reference()}]);
  draft["root"]["execution"]["permissions"] = json!(ceiling);
  draft["limits"]["permissions"] = json!(ceiling);
  draft["root"]["execution"]["budget"] = json!(total);
  draft["limits"]["budget"] = json!(total);
  draft["data_schemas"].as_array_mut().unwrap().push(json!(receipt));
}

pub(super) fn consume(draft: &mut Value, schema: &FlowDataSchema) {
  draft.as_object_mut().unwrap().remove("accepted_predecessor");
  draft["issue_snapshot"] = json!({"version":1,"schema":schema.reference(),"definition_version":1,
    "fields":{
      "priority":{"source":{"kind":"priority"},"required":true},
      "component":{"source":{"kind":"component"},"required":true},
      "kind":{"source":{"kind":"type"},"required":true},
      "size":{"source":{"kind":"size"},"required":true}},
    "admission":{"priority":{"field":"priority","values":{"high":80}},
      "risk":{"field":"size","values":{"small":"low"}},"allowed":{"kind":["feature_request"]}}
  });
  let policy = draft["root"]["nodes"]
    .as_array_mut()
    .unwrap()
    .iter_mut()
    .find(|node| node["key"] == workflow_ticket::REQUIREMENTS_POLICY)
    .unwrap();
  // Adapt canonical flat fields to the existing requirements gate's published grammar.
  // Admission restricts this scenario to feature requests; reproduction is not applicable.
  policy["input_binding"] = json!({"sources":{"issue":{"source":"incoming","schema":schema.reference(),"path":""}},
  "mapping":{"value":{"mapping":"object","fields":{
    "summary":{"mapping":"object","fields":{
      "eligible":{"mapping":"constant","value":true},
      "work_kind":{"mapping":"pointer","path":"/issue/kind"},
      "reproducibility":{"mapping":"constant","value":"not_applicable"},
      "component":{"mapping":"pointer","path":"/issue/component"},
      "size":{"mapping":"pointer","path":"/issue/size"}}},
    "priority_assessment":{"mapping":"constant","value":80}
  }}}});
}

pub(super) struct Retained {
  pub(super) execution: String,
  pub(super) reviewed: Value,
}

impl Retained {
  pub(super) async fn verify(&self, client: &Client, origin: &str, path: &str, triage: Option<&str>, run: &Value) {
    assert_eq!(run["issue_association"]["request_id"], self.execution);
    let request = get_json(client, format!("{origin}{path}/{}", self.execution)).await;
    assert_eq!(request["association"]["run_id"], run["id"]);
    assert_eq!(request["snapshot_digest"], self.reviewed["snapshot_digest"]);
    let detail = get_json(client, format!("{origin}{path}/{}/snapshot", self.execution)).await;
    assert_eq!(detail["availability"], "available");
    assert_eq!(
      detail["snapshot"], self.reviewed["snapshot"],
      "publication/restart must retain the reviewed input"
    );
    let snapshot = &detail["snapshot"];
    assert_eq!(snapshot["definition_head"], 1);
    assert_eq!(
      snapshot["values"],
      json!({"priority":"high","component":"api","kind":"feature_request","size":"small"})
    );
    assert!(snapshot["target"].get("predecessor_run").is_none_or(Value::is_null));
    assert_eq!(snapshot["attribution"].as_object().unwrap().len(), 4);
    let assessment = if let Some(triage) = triage {
      let producer = get_json(client, format!("{origin}/api/v1/factory-runs/{triage}")).await;
      assert_eq!(producer["id"], triage);
      let completions = workflow::diagnostic(client, origin, triage, "node-completions").await;
      let accepted: Vec<_> = completions["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["record"]["node_key"] == "assess" && row["record"]["outcome"] == "eligible")
        .collect();
      assert_eq!(accepted.len(), 1, "the fixture has one exact accepted assessment");
      assert!(producer["work_id"].is_string() && accepted[0]["record"]["output_digest"].is_string());
      Some(json!({"run_id":triage,"work_id":producer["work_id"],"record":accepted[0]["record"]["output_digest"]}))
    } else {
      None
    };
    for origin in snapshot["attribution"].as_object().unwrap().values() {
      if let Some(assessment) = &assessment {
        assert_eq!(origin["origin"]["kind"], "pipeline");
        for key in ["run_id", "work_id", "record"] {
          assert_eq!(
            origin["origin"][key], assessment[key],
            "classification must retain its exact {key}"
          );
        }
      } else {
        assert_eq!(origin["origin"]["kind"], "manual");
        assert!(origin["origin"].get("run_id").is_none());
      }
    }
  }
}
