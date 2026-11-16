//! CI composition extends the packaged UI journey; only technical capabilities and finite peers are installed.
use super::*;
use octacity_server_application::{
  FactoryCodexBuildProfile, FactoryCodexUsagePolicy, FactoryCommandBuildProfile, FactoryCommandOctafile,
  FactoryCommandProgram,
};

const CREDENTIAL: &str = "sk-glm5-finite-ci-signal-credential";
fn key(value: &str) -> FactoryKey {
  value.parse().unwrap()
}
fn reference(value: &str) -> ImmutableReference {
  ImmutableReference::new(key(value), key("v1"), FactoryDigest::content_sha256(value.as_bytes()))
}

pub(super) async fn fields(
  client: &Client,
  origin: &str,
  project: &str,
  directory: &Path,
  code: &code::CodeInstallation,
) -> Value {
  console::author(client, origin, project, directory, "en", code, Some("fields")).await;
  get_json(
    client,
    format!("{origin}/api/v1/projects/{project}/issue-property-definitions"),
  )
  .await
}

pub(super) struct Installation {
  pub peer: jev::JevHttpsFixture,
  pub connectors: PathBuf,
  pub permissions: FactoryPermissionSet,
}

/// Finite model instructions refer to the UI-published field identities. No Issue values or business definitions are seeded.
pub(super) fn model(directory: &Path, profile: &mut FactoryCodexBuildProfile, catalog: &Value) {
  let ids: BTreeMap<_, _> = catalog["definition"]["properties"]
    .as_array()
    .unwrap()
    .iter()
    .map(|field| (field["name"].as_str().unwrap(), field["id"].as_str().unwrap()))
    .collect();
  let mut values = serde_json::Map::new();
  for (name, path) in [
    ("CI note", "summary"),
    ("CI disposition", "status"),
    ("CI passed", "passed"),
    ("CI failures", "failures"),
    ("CI date", "observed_date"),
  ] {
    values.insert(
      format!("property.{}", ids[name]),
      json!({"$input":format!("/payload/value/{path}")}),
    );
  }
  profile.instructions = FactoryText::new(
    json!({"fixture_version":1,"result":values,"invocation":{"model":"finite-model","prefix":"Controlling Project rules (additions cannot waive these rules):\nAnalyze only the selected retained report; missing reports are not passing tests.\n\n","options":["--ignore-user-config","--config=mcp_servers={}"],"environment":{}},"data_checks":[{"text":"CI_PRIVATE_LOG_CANARY","present":false}]})
      .to_string(),
  )
  .unwrap();
  let model = ImmutableReference::new(
    key("finite-model"),
    key("v1"),
    FactoryDigest::content_sha256(profile.instructions.as_str().as_bytes()),
  );
  profile.toolchain.model = model.clone();
  let FactoryCodexUsagePolicy::TokenRates { model: usage_model, .. } = &mut profile.usage_policy else {
    panic!("finite model policy")
  };
  *usage_model = model;
  let executable = directory.join("finite-model");
  fs::copy(required_path("OCTACITY_FACTORY_PILOT_CLI", true), &executable).unwrap();
  set_mode(&executable, 0o700);
  let old = profile.toolchain.executable.clone();
  let executable_pin = ImmutableReference::new(
    old.identity().clone(),
    old.version().clone(),
    FactoryDigest::content_sha256(&fs::read(executable).unwrap()),
  );
  profile.toolchain.executable = executable_pin.clone();
  let mut permissions = serde_json::to_value(&profile.project_permissions).unwrap();
  for value in permissions["executables"].as_array_mut().unwrap() {
    if *value == serde_json::to_value(&old).unwrap() {
      *value = serde_json::to_value(&executable_pin).unwrap();
    }
  }
  for command in permissions["commands"].as_array_mut().unwrap() {
    if command["executable"] == serde_json::to_value(&old).unwrap() {
      command["executable"] = serde_json::to_value(&executable_pin).unwrap();
    }
  }
  profile.project_permissions = serde_json::from_value(permissions).unwrap();
  profile.local_permissions = LocalPermissionCeiling::fully_enforced(profile.project_permissions.clone());
}

pub(super) async fn install(
  directory: &Path,
  resources: &installation::Resources,
  profile: &FactoryCodexBuildProfile,
  profiles: &Path,
) -> Installation {
  let mut body: Value = serde_json::from_slice(&fs::read(profiles).unwrap()).unwrap();
  let report = FlowDataSchema::new(
    key("ci-retained-report"),
    key("v1"),
    FlowValueSchema::Object {
      fields: [
        (key("passed"), FlowFieldSchema::required(FlowValueSchema::Boolean)),
        (
          key("status"),
          FlowFieldSchema::required(FlowValueSchema::Enum {
            values: vec![json!("passed"), json!("failed")],
          }),
        ),
        (
          key("private_log"),
          FlowFieldSchema::required(FlowValueSchema::String {
            min_bytes: 1,
            max_bytes: 128,
          }),
        ),
        (
          key("summary"),
          FlowFieldSchema::required(FlowValueSchema::String {
            min_bytes: 1,
            max_bytes: 128,
          }),
        ),
        (
          key("failures"),
          FlowFieldSchema::required(FlowValueSchema::Integer {
            minimum: 0,
            maximum: 1000,
          }),
        ),
        (
          key("observed_date"),
          FlowFieldSchema::required(FlowValueSchema::String {
            min_bytes: 10,
            max_bytes: 10,
          }),
        ),
      ]
      .into(),
    },
  )
  .unwrap();
  let command_plugin = profile.toolchain.plugin.clone();
  let command_executor = reference("ci-report-executor");
  let output = EvidenceRequirement::new(
    key("ci.report"),
    EvidenceOutputKind::Report,
    report.reference().clone(),
    command_executor.clone(),
    command_plugin.clone(),
  );
  let mut command_permissions = serde_json::to_value(&profile.project_permissions).unwrap();
  command_permissions["executables"] = json!([]);
  command_permissions["commands"] = json!([]);
  command_permissions["secret_profiles"] = json!([]);
  command_permissions["outputs"]["kinds"] = json!(["ci.report"]);
  let command_permissions: FactoryPermissionSet = serde_json::from_value(command_permissions).unwrap();
  let script = r#"import {readFileSync,writeFileSync} from 'node:fs';
const intent=JSON.parse(readFileSync('/octacity/protected/command-input.json','utf8'));
const request=JSON.parse(intent.input.payload.value.description);
if(!['passed','failed','missing'].includes(request.mode)) throw new Error('Missing CI report selection');
if(request.mode!=='missing') writeFileSync('/workspace/output/ci.report.json',JSON.stringify({passed:request.mode==='passed',status:request.mode,summary:request.mode+' report',failures:request.mode==='passed'?0:2,observed_date:'2026-11-16',private_log:'CI_PRIVATE_LOG_CANARY'}));"#;
  let octafile=FactoryCommandOctafile::new(json!({"version":1,"tasks":{"collect":{"dir":"/workspace","shell":format!("node --input-type=module <<'CI_REPORT'\n{script}\nCI_REPORT"),"reports":[{"name":"ci.report","path":"output/ci.report.json","format":report.reference().identity()}]}}}).to_string()).unwrap();
  let command = FactoryCommandBuildProfile {
    identity: reference("ci-report-collector"),
    plugin: command_plugin.clone(),
    executor: command_executor.clone(),
    build_configuration: resources.configuration.clone(),
    pipeline_node: octacity_server_domain::PipelineNodeId::new("execute").unwrap(),
    program: FactoryCommandProgram {
      octafile,
      tasks: vec!["collect".into()],
      workspace_root: Some("/workspace".into()),
      credential_profile: None,
    },
    input_destination: "/octacity/protected/command-input.json".into(),
    parameters: BTreeMap::new(),
    project_permissions: command_permissions.clone(),
    local_permissions: LocalPermissionCeiling::fully_enforced(command_permissions),
    candidate_capture_tool: None,
    outputs: vec![output],
    observation_contract: None,
  };
  let state = FlowDataSchema::new(key("ci-route-state"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let result = FlowDataSchema::new(key("ci-route-result"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let plan = signals::plan(
    DecisionSignalPurpose::Routing,
    state.clone(),
    FlowDataMapping::new(FlowValueMapping::Pointer { path: "".into() }).unwrap(),
    &[key("analyze"), key("review")],
  );
  let peer = jev::JevHttpsFixture::start(plan.capability.model().version().as_str(), CREDENTIAL).await;
  let mut signal = signals::routing_binding(
    resources.project.parse().unwrap(),
    key("choose-ci-route"),
    (key("analyze"), result.clone()),
    plan,
    FlowDataMapping::new(FlowValueMapping::Constant { value: json!(true) }).unwrap(),
    key("analyze"),
    NetworkHost::new("localhost").unwrap(),
  );
  signal
    .outputs
    .push(octacity_server_application::FactoryDecisionSignalNodeOutput {
      disposition: DecisionSignalDisposition::Route(key("review")),
      outcome: key("review"),
      schema: result,
      projection: FlowDataMapping::new(FlowValueMapping::Constant { value: json!(false) }).unwrap(),
    });
  let signal = signal.pin_policy().unwrap();
  let writer = reference("manual.issue-fields");
  let refs = body["projects"][0]["references"].as_array_mut().unwrap();
  refs.extend([
    json!({"kind":"node_capability","alias":"ci-report-executor","reference":command_executor,"metadata":{"node_kinds":["build_command"],"signal_purposes":[],"enforcement":PermissionCategory::ALL,"compatible_choices":[],"command_output":report}}),
    json!({"kind":"node_capability","alias":"ci-report-collector","reference":command.identity}),
    json!({"kind":"node_capability","alias":"ci-signal-adapter","reference":signal.action.plugin()}),
    json!({"kind":"secret_profile","alias":signal.credential_profile,"reference":signal.plan.profile.policy()}),
    json!({"kind":"decision_signal_policy","alias":"ci-disposition","reference":signal.plan.profile.policy()}),
    json!({"kind":"node_capability","alias":"manual-issue-fields","reference":writer,"metadata":{"node_kinds":["trusted_action"],"signal_purposes":[],"enforcement":PermissionCategory::ALL,"compatible_choices":[],"issue_classification":{"policy":"ci-fields","size_labels":{}}}}),
  ]);
  body["projects"][0]["build_configurations"][0]["alias"] = json!("ci-report-collector");
  let mut ceiling = profile.project_permissions.clone();
  // Reuse the permission union from the ordinary command contracts, without widening the Agent profile.
  let mut permissions = serde_json::to_value(&ceiling).unwrap();
  for (field, extra) in [
    (
      "plugins",
      json!([writer, signal.action.plugin(), command_executor, command.identity]),
    ),
    ("secret_profiles", json!([signal.credential_profile])),
    ("network_hosts", json!(["localhost"])),
  ] {
    permissions[field]
      .as_array_mut()
      .unwrap()
      .extend(extra.as_array().unwrap().iter().cloned());
  }
  permissions["outputs"]["kinds"]
    .as_array_mut()
    .unwrap()
    .push(json!("ci.report"));
  ceiling = serde_json::from_value(permissions).unwrap();
  body["projects"][0]["references"][1]["metadata"]["permission_declarations"] = json!(ceiling);
  body["commands"] = json!([{"identity":command.identity,"plugin":command.plugin,"executor":command.executor,"build_configuration":command.build_configuration,"pipeline_node":command.pipeline_node,"program":command.program,"input_destination":command.input_destination,"parameters":command.parameters,"project_permissions":command.project_permissions,"local_permissions":{"grants":command.local_permissions.grants(),"enforceable":PermissionCategory::ALL},"candidate_capture_tool":null,"outputs":command.outputs}]);
  body["signals"] = json!([signals::routing_entry(&signal)]);
  body["issue_classification_actions"] = json!([{"identity":writer,"principal":"ci-field-controller"}]);
  private_file(directory, "profiles.json", &body.to_string());
  let provider = private_file(
    directory,
    "ci-jev.json",
    &peer.connector_configuration(&signal.plan.capability).to_string(),
  );
  let credential = private_file(directory, "ci-jev-key", CREDENTIAL);
  let connectors=private_file(directory,"connectors.json",&json!({"version":1,"connectors":[{"kind":"jev","credential_profile":signal.credential_profile,"configuration_file":provider,"credential_file":credential}]}).to_string());
  Installation {
    peer,
    connectors,
    permissions: ceiling,
  }
}

pub(super) async fn passed_run(client: &Client, origin: &str, project: &str) -> String {
  let issues = get_json(client, format!("{origin}/api/v1/projects/{project}/issues?limit=100")).await;
  let issue = issues["items"]
    .as_array()
    .unwrap()
    .iter()
    .find(|issue| issue["title"] == "CI passed")
    .unwrap();
  let executions = get_json(
    client,
    format!(
      "{origin}/api/v1/projects/{project}/issues/{}/executions",
      issue["id"].as_str().unwrap()
    ),
  )
  .await;
  string(&executions["items"][0]["association"], "run_id")
}

pub(super) async fn retained(client: &Client, origin: &str, project: &str) -> Value {
  let issues = get_json(client, format!("{origin}/api/v1/projects/{project}/issues?limit=100")).await;
  let mut retained = Vec::new();
  for item in issues["items"].as_array().unwrap() {
    let issue = string(item, "id");
    let current = get_json(client, format!("{origin}/api/v1/projects/{project}/issues/{issue}")).await;
    let executions = get_json(
      client,
      format!("{origin}/api/v1/projects/{project}/issues/{issue}/executions"),
    )
    .await;
    let mut snapshots = Vec::new();
    for execution in executions["items"].as_array().unwrap() {
      snapshots.push(
        get_json(
          client,
          format!(
            "{origin}/api/v1/projects/{project}/issues/{issue}/executions/{}/snapshot",
            execution["id"].as_str().unwrap()
          ),
        )
        .await,
      );
    }
    retained.push(json!({"issue":issue,"classification":current["classification"],"planning":current["planning"],"snapshots":snapshots}));
  }
  json!(retained)
}
