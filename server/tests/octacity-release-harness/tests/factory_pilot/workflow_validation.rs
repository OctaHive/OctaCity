//! Finite independent validator over the original retained private inventory and candidate.
use super::*;
use octacity_server_application::*;
use octacity_server_factory::*;
use std::collections::BTreeMap;

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
pub(super) struct Validator {
  pub(super) profile: FactoryCommandBuildProfile,
  schemas: BTreeMap<FactoryKey, FlowDataSchema>,
  report: EvidenceRequirement,
  facts: EvidenceRequirement,
}
impl Validator {
  pub(super) fn new(author: &FactoryProtectedTestAuthoringProfile, coder: &models::InstalledModel) -> Self {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
      .ancestors()
      .find(|p| p.join("config/factory/flows").is_dir())
      .unwrap()
      .join("config/factory/flows");
    let template = FlowDefinitionTemplate::from_manifest(&fs::read(root.join("base-flow.json")).unwrap(), |path| {
      fs::read(root.join(path.as_str())).map_err(|_| FactoryError::InvalidConfiguration {
        field: "pilot template",
      })
    })
    .unwrap();
    let mut schemas = template.data_schemas().unwrap();
    // Preserve the previously qualified validator program bytes. New installed
    // programs use their dedicated source bound rather than FactoryText.
    let source = include_str!("../../fixtures/factory/protected-validator.mjs")
      .lines()
      .map(str::trim)
      .filter(|line| !line.starts_with("//"))
      .collect::<Vec<_>>()
      .join("\n");
    let shell = format!(
      "node --input-type=module - /octacity/protected/validation-input.json /octacity/protected/command-input.json /octacity/protected/test-inventory.json /octacity/protected/tests /workspace/source /workspace/output/validation.report.json /workspace/output/validation-facts.report.json <<'PILOT_VALIDATOR'\n{source}\nPILOT_VALIDATOR"
    );
    let mut profile = author.command().clone();
    profile.program.octafile = FactoryCommandOctafile::new(json!({"version":1,"tasks":{"validate":{
      "dir":"/workspace","shell":shell,"reports":[
        {"name":"validation.report","path":"output/validation.report.json","format":schemas[&key("protected_validation_report")].reference().identity()},
        {"name":"validation.facts","path":"output/validation-facts.report.json","format":schemas[&key("protected_validation_facts")].reference().identity()}
      ]}}}).to_string()).unwrap();
    profile.program.tasks = vec!["validate".into()];
    profile.identity = ImmutableReference::new(
      key("installed-validator-policy"),
      key("v1"),
      FactoryDigest::content_sha256(profile.program.octafile.as_str().as_bytes()),
    );
    profile.executor = ImmutableReference::new(key("installed-finite-validator"), key("v1"), profile.identity.digest());
    let report = EvidenceRequirement::new(
      key("validation.report"),
      EvidenceOutputKind::Report,
      schemas[&key("protected_validation_report")].reference().clone(),
      profile.executor.clone(),
      profile.plugin.clone(),
    );
    let facts = EvidenceRequirement::new(
      key("validation.facts"),
      EvidenceOutputKind::Report,
      schemas[&key("protected_validation_facts")].reference().clone(),
      profile.executor.clone(),
      profile.plugin.clone(),
    );
    let mut grants = serde_json::to_value(&profile.project_permissions).unwrap();
    grants["mounts"]
      .as_array_mut()
      .unwrap()
      .push(json!({"root":octacity_protocol::FACTORY_SOURCE_ROOT,"mode":"read_only"}));
    grants["outputs"] = json!({"kinds":["validation.report","validation.facts"],"max_artifact_count":0,"max_artifact_bytes":0,"max_report_count":2,"max_report_bytes":65536});
    profile.project_permissions = serde_json::from_value(grants).unwrap();
    profile.local_permissions = LocalPermissionCeiling::fully_enforced(profile.project_permissions.clone());
    profile.candidate_capture_tool = Some(coder.profile.capture.as_ref().unwrap().tool().clone());
    profile.outputs = vec![report.clone(), facts.clone()];
    // The finite coder's summary is a string; all provenance-bearing fields keep
    // the supplied protected-validation schema without inventing a second format.
    let mut input = serde_json::to_value(&schemas[&key("protected_validation_input")]).unwrap();
    input["shape"]["fields"]["summary"]["value"] = json!(FlowValueSchema::String {
      min_bytes: 1,
      max_bytes: 64
    });
    let original = &schemas[&key("protected_validation_input")];
    let input = FlowDataSchema::new(
      original.reference().identity().clone(),
      original.reference().version().clone(),
      serde_json::from_value(input["shape"].clone()).unwrap(),
    )
    .unwrap();
    schemas.insert(key("protected_validation_input"), input);
    Self {
      profile,
      schemas,
      report,
      facts,
    }
  }
  pub(super) fn entry(&self, author: &FactoryProtectedTestAuthoringProfile) -> Value {
    json!({"profiles":[workflow_author::command_entry(&self.profile)],"inputs":{
      "author_profile":author.command().identity,"policy":author.policy(),"inventory":author.inventory(),"capture_tool":self.profile.candidate_capture_tool
    },"report":self.report,"facts":self.facts})
  }
  pub(super) fn extend(&self, draft: &mut Value, author: &str, coder: &str, validator: &str, budget: BudgetLimit) {
    let mut node = workflow::node(
      validator,
      "build_command",
      &self.schemas[&key("protected_validation_report")],
      budget,
      &self.profile.project_permissions,
      "validated",
    );
    let source: Value = serde_json::from_str(include_str!(
      "../../../../../config/factory/flows/base-flow/nodes/protected_validation.json"
    ))
    .unwrap();
    let mut binding = source["nodes"][0]["input_binding"].clone();
    for value in binding["sources"].as_object_mut().unwrap().values_mut() {
      if value["node"] == "implementation" {
        value["node"] = json!(coder);
        value["outcome"] = json!("implemented");
      }
      if value["node"] == "protected_tests" {
        value["node"] = json!(author);
        value["outcome"] = json!("authored");
      }
    }
    binding["sources"]["candidate"]["path"] =
      json!(format!("/build/outputs/{}/data/payload/value", models::CANDIDATE_FACTS));
    binding["sources"]["tests"]["path"] = json!("/build/outputs/private.inventory/artifact");
    node["input_binding"] = binding;
    node["input_schema"] = json!(self.schemas[&key("protected_validation_input")].reference());
    node["context_binding"] = source["nodes"][0]["context_binding"].clone();
    node["build"] = json!(FlowBuildBinding {
      tool: self.profile.executor.clone(),
      plugin: self.profile.plugin.clone(),
      result_tool: None,
      capture_write_paths: None,
      capture_deliverables: None,
      model_or_tool: self.profile.identity.clone(),
      task_digest: self.profile.task_digest(),
      build_configuration: self.profile.build_configuration.clone(),
      result_output: self.report.kind().clone(),
      result_output_kind: EvidenceOutputKind::Report,
      result_outcome: key("validated"),
      evidence: vec![self.facts.clone()]
    });
    draft["root"]["nodes"].as_array_mut().unwrap().push(node);
    let transitions = draft["root"]["transitions"].as_array_mut().unwrap();
    let edge = transitions.iter_mut().find(|v| v["predecessor"] == coder).unwrap();
    *edge = workflow::edge(coder, "implemented", "node", validator);
    transitions.push(workflow::edge(validator, "validated", "terminal", "finished"));
    draft["root"]["terminals"][0]["schema"] = json!(self.schemas[&key("protected_validation_report")].reference());
    // Only schemas used by this Node are appended, each already fully expanded.
    for name in [
      "protected_validation_input",
      "protected_validation_report",
      "protected_validation_facts",
    ] {
      draft["data_schemas"]
        .as_array_mut()
        .unwrap()
        .push(json!(self.schemas[&key(name)]));
    }
  }
  pub(super) async fn verify(
    &self,
    client: &Client,
    origin: &str,
    rows: &[(Vec<u8>, Vec<u8>)],
    author: &str,
    coder: &str,
    validator: &str,
  ) {
    let records = rows
      .iter()
      .map(|(_, bytes)| serde_json::from_slice::<Value>(bytes).unwrap())
      .collect::<Vec<_>>();
    let selected = |node: &str| records.iter().find(|r| r["input"]["node"] == node).unwrap();
    let output = |node: &str, name: &str| {
      selected(node)["build"]["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == name)
        .unwrap()
    };
    let record = selected(validator);
    let report = &record["payload"]["value"];
    assert_eq!(
      report["classes"],
      json!([{"class":"fixture.contract","outcome":"passed"}])
    );
    assert_eq!(report["tests"], output(author, "private.inventory")["artifact"]);
    let candidate = &output(coder, models::CANDIDATE_FACTS)["data"]["payload"]["value"];
    assert_eq!(
      report["candidate"]["candidate_revision"],
      candidate["candidate_revision"]
    );
    assert_eq!(report["candidate"]["changeset_digest"], candidate["candidate_digest"]);
    let report_output = output(validator, "validation.report");
    let bytes = workflow_author::download_artifact(
      client,
      origin,
      report_output["artifact"]["artifact_id"].as_str().unwrap(),
    )
    .await;
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), *report);
    let facts = &output(validator, "validation.facts")["data"]["payload"]["value"];
    assert_eq!(
      facts["report_digest"],
      FactoryDigest::content_sha256(&bytes).to_string()
    );
    assert_eq!(facts["protected_inventory"], report["tests"]);
    assert_eq!(facts["candidate_digest"], candidate["candidate_digest"]);
    assert_eq!(facts["covered_classes"], json!(["fixture.contract"]));
    let serialized = record.to_string();
    assert!(!serialized.contains("\"expected\":\"facts\"") && !serialized.contains("contract.rs"));
  }
}
