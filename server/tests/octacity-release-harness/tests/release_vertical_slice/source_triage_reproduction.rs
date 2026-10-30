//! A reviewed installed command observes the exact source independently of AI.
use super::*;
use octacity_server_application::{
  FactoryCommandBuildProfile, FactoryCommandObservationContract, FactoryCommandOctafile, FactoryCommandProgram,
  FactoryFlowDefinitionDraft,
};
use octacity_server_factory::*;
use std::collections::BTreeMap;

fn key(value: &str) -> FactoryKey {
  value.parse().unwrap()
}
fn reference(name: &str, bytes: &[u8]) -> ImmutableReference {
  ImmutableReference::new(key(name), key("v1"), FactoryDigest::content_sha256(bytes))
}

pub(super) async fn install(
  execution: &ExecutionEnvironment,
  model: &models::InstalledModel,
) -> FactoryCommandBuildProfile {
  let original = get_json(
    &execution.environment.client,
    format!(
      "{}/api/v1/build-configurations/{}/versions/1",
      execution.environment.origin,
      execution.resources.configuration.id()
    ),
  )
  .await;
  let mut definition = original["definition"].clone();
  definition["runtime"]["network"] = json!({"mode":"disabled"});
  let published = post_management(
    &execution.environment.client,
    &execution.environment.origin,
    "/api/v1/build-configurations",
    &format!("{}-reproduction-build", execution.run),
    json!({"project_id":execution.resources.project,"name":"Bounded offline reproduction","definition":definition}),
  )
  .await;
  let configuration = BuildConfigurationRef::new(
    resource_id(&published).parse().unwrap(),
    octacity_server_domain::BuildConfigurationVersion::INITIAL,
    execution.resources.project.parse().unwrap(),
    FactoryDigest::content_sha256(&serde_json::to_vec(&published["resource"]["definition"]).unwrap()),
  );
  let runner = octacity_runner::RunnerInstallation::load(&execution.release.octa_root).unwrap();
  let installed = &runner.plugins["shell"];
  let plugin = ImmutableReference::new(key("shell"), key(&installed.version), installed.sha256.parse().unwrap());
  let observations = FlowDataSchema::new(
    key("packaged.health-observations"),
    key("v1"),
    FlowValueSchema::Object {
      fields: BTreeMap::from([
        (
          key("requests"),
          FlowFieldSchema::required(FlowValueSchema::Integer { minimum: 0, maximum: 1 }),
        ),
        (
          key("status"),
          FlowFieldSchema::required(FlowValueSchema::Integer {
            minimum: 100,
            maximum: 599,
          }),
        ),
      ]),
    },
  )
  .unwrap();
  let contract = FactoryCommandObservationContract {
    environment: reference("node-health-environment", &serde_json::to_vec(&execution.backend.immutable_image()).unwrap()),
    criterion: reference("health-status-500", b"one HTTP GET /health returns status 500"),
    observations,
    completion: serde_json::from_value(json!({"operation":"compare","left":{"source":"path","path":"/requests"},"comparison":"equal","right":{"source":"literal","value":1}})).unwrap(),
    symptom: serde_json::from_value(json!({"operation":"compare","left":{"source":"path","path":"/status"},"comparison":"equal","right":{"source":"literal","value":500}})).unwrap(),
  };
  let policy = json!({"environment":contract.environment,"criterion":contract.criterion});
  let source = include_str!("../../fixtures/factory/triage-reproduction.mjs");
  let shell = format!(
    "node --input-type=module - /octacity/protected/command-input.json /workspace/source /workspace/output/reproduction.report.json '{}' <<'TRIAGE_PROBE'\n{source}\nTRIAGE_PROBE",
    policy
  );
  let octafile = FactoryCommandOctafile::new(json!({"version":1,"tasks":{"reproduce":{"dir":"/workspace","shell":shell,
    "reports":[{"name":"reproduction-report","path":"output/reproduction.report.json","format":contract.report_schema().unwrap().reference().identity()}]
  }}}).to_string()).unwrap();
  let identity = reference("packaged-health-command", octafile.as_str().as_bytes());
  let executor = reference("packaged-health-probe", source.as_bytes());
  let mut aggregate = serde_json::to_value(&model.profile.project_permissions).unwrap();
  aggregate["mounts"]
    .as_array_mut()
    .unwrap()
    .push(json!({"root":"/workspace/source","mode":"read_only"}));
  for field in [
    "executables",
    "tools",
    "commands",
    "network_hosts",
    "secret_profiles",
    "workload_identity_profiles",
  ] {
    aggregate[field] = json!([]);
  }
  aggregate["outputs"] = json!({"kinds":["reproduction-report"],"max_artifact_count":0,"max_artifact_bytes":0,"max_report_count":1,"max_report_bytes":1048576});
  let permissions: FactoryPermissionSet = serde_json::from_value(aggregate).unwrap();
  let schema = contract.report_schema().unwrap();
  FactoryCommandBuildProfile {
    identity,
    plugin: plugin.clone(),
    executor: executor.clone(),
    build_configuration: configuration,
    pipeline_node: execution.resources.pipeline_node.clone(),
    program: FactoryCommandProgram {
      octafile,
      tasks: vec!["reproduce".into()],
      workspace_root: Some("/workspace".into()),
      credential_profile: None,
    },
    input_destination: "/octacity/protected/command-input.json".into(),
    parameters: Default::default(),
    project_permissions: permissions.clone(),
    local_permissions: LocalPermissionCeiling::fully_enforced(permissions),
    candidate_capture_tool: None,
    observation_contract: Some(contract),
    outputs: vec![EvidenceRequirement::new(
      key("reproduction-report"),
      EvidenceOutputKind::Report,
      schema.reference().clone(),
      executor,
      plugin,
    )],
  }
}

pub(super) fn entry(command: &FactoryCommandBuildProfile) -> Value {
  json!({"identity":command.identity,"plugin":command.plugin,"executor":command.executor,
    "build_configuration":command.build_configuration,"pipeline_node":command.pipeline_node,"program":command.program,
    "input_destination":command.input_destination,"parameters":command.parameters,"project_permissions":command.project_permissions,
    "local_permissions":{"grants":command.local_permissions.grants(),"enforceable":PermissionCategory::ALL},
    "outputs":command.outputs,"observation_contract":command.observation_contract})
}

pub(super) fn bind(draft: Value, command: &FactoryCommandBuildProfile, flow: FlowDefinitionId) -> Value {
  let original: FactoryFlowDefinitionDraft = serde_json::from_value(draft).unwrap();
  let root = Path::new(env!("CARGO_MANIFEST_DIR"))
    .ancestors()
    .nth(3)
    .unwrap()
    .join("config/factory");
  let mut template = FlowDefinitionTemplate::from_manifest(
    &fs::read(root.join("presets/triage/with-reproduction-v1.json")).unwrap(),
    |path| {
      fs::read(root.join(path.as_str())).map_err(|_| FactoryError::InvalidConfiguration {
        field: "qualified template",
      })
    },
  )
  .unwrap();
  let schema = command.observation_contract.as_ref().unwrap().report_schema().unwrap();
  template.schemas.insert(
    key("reproduction_report"),
    FlowTemplateSchema {
      identity: schema.reference().identity().clone(),
      version: schema.reference().version().clone(),
      shape: schema.shape().clone(),
    },
  );
  template.execution = original.root.execution.clone();
  for node in &mut template.nodes {
    if let Some(selected) = original.root.nodes.iter().find(|selected| selected.key() == &node.key)
      && selected.profile_selection().is_some()
    {
      node.profile_selection = selected.profile_selection().cloned();
      node.build_profile = None;
      node.context_binding = selected.context_binding().cloned();
      node.budget = selected.budget();
      node.permissions = selected.permissions().clone();
    }
    if node.key.as_str() == "reproduction" {
      node.permissions = command.project_permissions.clone();
      node.budget = BudgetLimit::new(1, 120_000, 0, 0, 8 * 1024 * 1024).unwrap();
    }
  }
  let choice = command.observation_bindings().unwrap().remove(0);
  let mut wire = serde_json::to_value(&template).unwrap();
  let analyst = wire["nodes"]
    .as_array_mut()
    .unwrap()
    .iter_mut()
    .find(|node| node["key"] == "classification")
    .unwrap();
  analyst["input_binding"]["mapping"]["value"]["fields"]["command_policy"]["value"] = json!([choice]);
  template = serde_json::from_value(wire).unwrap();
  let binding = FlowBuildBinding {
    tool: command.executor.clone(),
    result_tool: None,
    capture_write_paths: None,
    capture_deliverables: None,
    plugin: command.plugin.clone(),
    model_or_tool: command.identity.clone(),
    task_digest: command.task_digest(),
    build_configuration: command.build_configuration.clone(),
    result_output: key("reproduction-report"),
    result_output_kind: EvidenceOutputKind::Report,
    result_outcome: key("observed"),
    evidence: vec![],
  };
  let published = template
    .instantiate(
      flow,
      FlowDefinitionVersion::INITIAL,
      &BTreeMap::from([(key("reproduction"), binding)]),
    )
    .unwrap();
  // Extend the ordinary reviewed draft; preserve already composed reporting
  // calls and dependency identities returned by the preset resolver.
  let mut wire = serde_json::to_value(original).unwrap();
  wire["root"]["nodes"]
    .as_array_mut()
    .unwrap()
    .push(serde_json::to_value(published.definition.node(&key("reproduction")).unwrap()).unwrap());
  let analyst = wire["root"]["nodes"]
    .as_array_mut()
    .unwrap()
    .iter_mut()
    .find(|node| node["key"] == "classification")
    .unwrap();
  analyst["input_binding"]["mapping"]["value"]["fields"]["command_policy"]["value"] = json!([choice]);
  let edges = wire["root"]["transitions"].as_array_mut().unwrap();
  let edge = edges
    .iter_mut()
    .find(|edge| edge["predecessor"] == "eligibility_gate" && edge["outcome"] == "classify")
    .unwrap();
  edge["target"] = json!({"kind":"node","key":"reproduction"});
  edges.push(
    serde_json::to_value(
      published
        .definition
        .transitions()
        .iter()
        .find(|edge| edge.predecessor().as_str() == "reproduction")
        .unwrap(),
    )
    .unwrap(),
  );
  wire["data_schemas"]
    .as_array_mut()
    .unwrap()
    .push(serde_json::to_value(schema).unwrap());
  wire
}
