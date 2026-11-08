//! An installed command observes the symptom on the exact admitted base.
use super::*;
use octacity_server_application::{
  FactoryCommandBuildProfile, FactoryCommandObservationContract, FactoryCommandOctafile,
};
use octacity_server_factory::*;
use std::collections::BTreeMap;

const CHECK: &str = "reproduction";
const RESULT: &str = "reproduction-result";
const REPORT: &str = "reproduction.report";

fn key(value: &str) -> FactoryKey {
  value.parse().unwrap()
}

fn pin(name: &str, bytes: &[u8]) -> ImmutableReference {
  ImmutableReference::new(key(name), key("v1"), FactoryDigest::content_sha256(bytes))
}

pub(super) struct Reproduction {
  command: FactoryCommandBuildProfile,
  report: FlowDataSchema,
}

impl Reproduction {
  pub(super) fn new(base: &FactoryCommandBuildProfile) -> Self {
    let observations = FlowDataSchema::new(
      key("pilot.reproduction-observation"),
      key("v1"),
      FlowValueSchema::Object {
        fields: BTreeMap::from([
          (
            key("checks"),
            FlowFieldSchema::required(FlowValueSchema::Integer { minimum: 0, maximum: 1 }),
          ),
          (
            key("symptom_present"),
            FlowFieldSchema::required(FlowValueSchema::Boolean),
          ),
        ]),
      },
    )
    .unwrap();
    let predicate = |path: &str, expected: Value| {
      serde_json::from_value(json!({
      "operation":"compare","left":{"source":"path","path":path},"comparison":"equal","right":{"source":"literal","value":expected}
    })).unwrap()
    };
    let contract = FactoryCommandObservationContract {
      environment: pin("released-pilot-repository", models::CANDIDATE_FILE.as_bytes()),
      criterion: pin("corrupted-facts-marker", b"corrupted"),
      observations,
      completion: predicate("/checks", json!(1)),
      symptom: predicate("/symptom_present", json!(true)),
      requires_protected_tests: false,
      stop_on_indeterminate: false,
    };
    let report = contract.report_schema().unwrap();
    let mut command = base.clone();
    let fixture = json!({"environment":contract.environment,"criterion":contract.criterion,
      "file":models::CANDIDATE_FILE,"marker":"corrupted","input":command.input_destination,
      "report":format!("/workspace/output/{REPORT}.json")});
    let program = include_str!("../../fixtures/factory/reproduction.mjs");
    let shell = format!("node --input-type=module - '{fixture}' <<'PILOT_REPRODUCTION'\n{program}\nPILOT_REPRODUCTION");
    command.program.octafile = FactoryCommandOctafile::new(
      json!({"version":1,"tasks":{"reproduce":{
        "dir":"/workspace","shell":shell,
        "reports":[{"name":REPORT,"path":format!("output/{REPORT}.json"),"format":report.reference().identity()}]
      }}})
      .to_string(),
    )
    .unwrap();
    command.program.tasks = vec!["reproduce".into()];
    command.identity = pin(
      "installed-reproduction-policy",
      command.program.octafile.as_str().as_bytes(),
    );
    command.executor = ImmutableReference::new(key("installed-reproduction"), key("v1"), command.identity.digest());
    command.outputs = vec![EvidenceRequirement::new(
      key(REPORT),
      EvidenceOutputKind::Report,
      report.reference().clone(),
      command.executor.clone(),
      command.plugin.clone(),
    )];
    command.observation_contract = Some(contract);
    let mut permissions = json!(command.project_permissions);
    permissions["mounts"]
      .as_array_mut()
      .unwrap()
      .push(json!({"root":octacity_protocol::FACTORY_SOURCE_ROOT,"mode":"read_only"}));
    permissions["outputs"] = json!({"kinds":[REPORT],"max_artifact_count":0,
      "max_artifact_bytes":0,"max_report_count":1,"max_report_bytes":65536});
    command.project_permissions = serde_json::from_value(permissions).unwrap();
    command.local_permissions = LocalPermissionCeiling::fully_enforced(command.project_permissions.clone());
    Self { command, report }
  }

  pub(super) fn install(&self, catalog: &mut Value, ceiling: &mut FactoryPermissionSet) {
    let command = &self.command;
    let mut entry = workflow_author::command_entry(command);
    entry["observation_contract"] = json!(command.observation_contract);
    catalog["commands"] = json!([entry]);
    catalog["projects"][0]["references"].as_array_mut().unwrap().extend([
      json!({"kind":"node_capability","alias":"reproduction-command","reference":command.executor}),
      json!({"kind":"node_capability","alias":"reproduction-policy","reference":command.identity}),
    ]);
    workflow_author::include_permissions(ceiling, &command.project_permissions);
    let mut grants = json!(ceiling);
    grants["outputs"]["kinds"].as_array_mut().unwrap().push(json!(REPORT));
    *ceiling = serde_json::from_value(grants).unwrap();
  }

  pub(super) fn extend(&self, draft: &mut Value) {
    let part: Value = serde_json::from_str(include_str!(
      "../../../../../config/factory/presets/ticket-delivery/reproduction-branch-v1.json"
    ))
    .unwrap();
    let command = &self.command;
    let budget = BudgetLimit::new(1, 120_000, 0, 0, 8 * 1024 * 1024).unwrap();
    let mut check = workflow::node(
      CHECK,
      "build_command",
      &self.report,
      budget,
      &command.project_permissions,
      "observed",
    );
    let triage_schema = draft["root"]["input_schema"].clone();
    check["input_schema"] = triage_schema.clone();
    check["input_binding"] = json!({"sources":{"triage":{"source":"incoming","schema":triage_schema,"path":""}},
      "mapping":{"value":{"mapping":"pointer","path":"/triage"}}});
    check["context_binding"] = json!({"work_task":"task","entries":{}});
    check["build"] = json!(FlowBuildBinding {
      execution_permissions: None,
      tool: command.executor.clone(),
      plugin: command.plugin.clone(),
      result_tool: None,
      capture_write_paths: None,
      capture_deliverables: None,
      model_or_tool: command.identity.clone(),
      task_digest: command.task_digest(),
      build_configuration: command.build_configuration.clone(),
      result_output: key(REPORT),
      result_output_kind: EvidenceOutputKind::Report,
      result_outcome: key("observed"),
      evidence: vec![],
    });
    let template: FlowTemplateNode = serde_json::from_value(
      part["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["key"] == RESULT)
        .unwrap()
        .clone(),
    )
    .unwrap();
    let mut result = workflow::node(
      RESULT,
      "deterministic_gate",
      &self.report,
      template.budget,
      &FactoryPermissionSet::deny_all(),
      "cannot_reproduce",
    );
    result["outcomes"] = json!(
      template
        .outcomes
        .iter()
        .map(|outcome| json!({
          "key":outcome.key,"kind":outcome.kind,"schema":self.report.reference(),
        }))
        .collect::<Vec<_>>()
    );
    result["input_binding"] = json!(template.input_binding);
    result["gate"] = json!(template.gate.unwrap().binding().unwrap());
    let root = &mut draft["root"];
    root["entry"] = json!(CHECK);
    root["nodes"].as_array_mut().unwrap().extend([check, result]);
    root["transitions"].as_array_mut().unwrap().extend([
      workflow::edge(CHECK, "observed", "node", RESULT),
      workflow::edge(RESULT, "reproduced", "node", workflow_ticket::REQUIREMENTS_POLICY),
      workflow::edge(RESULT, "cannot_reproduce", "terminal", "cannot_reproduce"),
      workflow::edge(RESULT, "unresolved", "terminal", "reproduction_unresolved"),
    ]);
    root["terminals"].as_array_mut().unwrap().extend(
      ["cannot_reproduce", "reproduction_unresolved"].map(|key| json!({"key":key,"schema":self.report.reference()})),
    );
    draft["data_schemas"].as_array_mut().unwrap().push(json!(self.report));
  }
}
