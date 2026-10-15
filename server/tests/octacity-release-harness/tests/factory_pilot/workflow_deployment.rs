//! Stage-only deployment over the ordinary command, capture and promotion boundaries.
use super::*;
use octacity_server_application::*;
use octacity_server_factory::*;
use std::collections::BTreeMap;

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn reference(value: &str) -> ImmutableReference {
  ImmutableReference::new(key(value), key("v1"), FactoryDigest::content_sha256(value.as_bytes()))
}
/// Publish a distinct immutable configuration while the bootstrap Server is live.
/// The author/coder configuration keeps its disabled runtime network policy.
pub(super) async fn create_configuration(
  client: &Client,
  origin: &str,
  run: &str,
  resources: &resources::PilotResources,
) -> BuildConfigurationRef {
  let original = get_json(
    client,
    format!(
      "{origin}/api/v1/build-configurations/{}/versions/1",
      resources.configuration.id()
    ),
  )
  .await;
  let mut definition = original["definition"].clone();
  definition["runtime"]["network"] = json!({
    "mode":"restricted", "allowed_hosts":["host.microsandbox.internal"]
  });
  let created = post_management(
    client,
    origin,
    "/api/v1/build-configurations",
    &format!("{run}-stage-configuration"),
    json!({
      "project_id":resources.project,"name":format!("stage-{run}"),"definition":definition
    }),
  )
  .await;
  let id = resource_id(&created);
  let published = get_json(client, format!("{origin}/api/v1/build-configurations/{id}/versions/1")).await;
  assert_eq!(published, created["resource"]);
  assert_eq!(published["definition"], definition);
  let digest = FactoryDigest::content_sha256(&serde_json::to_vec(&published["definition"]).unwrap());
  BuildConfigurationRef::new(
    id.parse().unwrap(),
    octacity_server_domain::BuildConfigurationVersion::INITIAL,
    resources.project.parse().unwrap(),
    digest,
  )
}
pub(super) struct InstalledDeployment {
  pub(super) profile: FactoryDeploymentCommandProfile,
  pub(super) report: FlowDataSchema,
  input: FlowDataSchema,
  qualified: FlowDataSchema,
  target: deployment_target::DeploymentTarget,
}
impl InstalledDeployment {
  pub(super) async fn new(
    directory: &Path,
    base: &FactoryCommandBuildProfile,
    capture: &ImmutableReference,
    backend: &ReleaseBackend,
    configuration: &BuildConfigurationRef,
  ) -> Self {
    let directory = directory.join("deployment-target");
    fs::create_dir(&directory).unwrap();
    set_mode(&directory, 0o700);
    let target = deployment_target::DeploymentTarget::start(&directory).await;
    // This pilot declares the selected backend's host bridge explicitly. No
    // provider gateway address or host grant is inferred by Factory core.
    assert!(matches!(backend, ReleaseBackend::Microsandbox { .. }));
    let mut endpoint = url::Url::parse(target.endpoint()).unwrap();
    endpoint.set_host(Some("host.microsandbox.internal")).unwrap();
    let authority = FactoryDeploymentCommandAuthority::new(
      key("installed-stage-target"),
      key("v1"),
      reference("pilot-stage-environment"),
      endpoint.into(),
      None,
    )
    .unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
      .ancestors()
      .find(|p| p.join("config/factory/flows").is_dir())
      .unwrap()
      .join("config/factory/flows");
    let template =
      FlowDefinitionTemplate::from_manifest(&fs::read(root.join("promotion-flow.json")).unwrap(), |path| {
        fs::read(root.join(path.as_str())).map_err(|_| FactoryError::InvalidConfiguration {
          field: "pilot template",
        })
      })
      .unwrap();
    let schemas = template.data_schemas().unwrap();
    let mut report_shape = schemas[&key("deployment_receipt")].shape().clone();
    let FlowValueSchema::Object { fields } = &mut report_shape else {
      unreachable!()
    };
    fields.insert(
      key("measurements"),
      FlowFieldSchema::required(FlowValueSchema::Object {
        fields: BTreeMap::from([(
          key("matching_reads"),
          FlowFieldSchema::required(FlowValueSchema::Integer { minimum: 2, maximum: 2 }),
        )]),
      }),
    );
    let report = FlowDataSchema::new(key("pilot.deployment-observation"), key("v1"), report_shape).unwrap();
    let qualified = FlowDataSchema::new(
      key("pilot.qualified-deployment"),
      key("v1"),
      FlowValueSchema::Object {
        fields: BTreeMap::from([
          (key("observation"), FlowFieldSchema::required(report.shape().clone())),
          (
            key("fresh_until"),
            FlowFieldSchema::required(FlowValueSchema::Integer {
              minimum: 0,
              maximum: i64::MAX,
            }),
          ),
        ]),
      },
    )
    .unwrap();
    let mut input_shape = schemas[&key("release_input")].shape().clone();
    let FlowValueSchema::Object { fields } = &mut input_shape else {
      unreachable!()
    };
    fields.remove(&key("subject"));
    fields.remove(&key("public_contract"));
    let input = FlowDataSchema::new(key("pilot.stage-candidate"), key("v1"), input_shape).unwrap();
    let mut command = base.clone();
    command.identity = authority.reference().clone();
    command.build_configuration = configuration.clone();
    // Promotion executes as the independently installed stock shell capability,
    // already present in the exact approved plugin inventory.
    command.executor = command.plugin.clone();
    let source = include_str!("../../fixtures/factory/deployer.mjs");
    let candidate_file = Path::new(models::CANDIDATE_FILE)
      .strip_prefix(octacity_protocol::FACTORY_SOURCE_ROOT)
      .unwrap()
      .to_str()
      .unwrap();
    let shell = format!(
      "node --input-type=module - /octacity/protected/deployment-target.json /octacity/protected/promotion-input.json /octacity/protected/command-input.json /workspace/source {candidate_file} /workspace/output/deployment.report.json configured-pilot-strategy <<'PILOT_DEPLOYER'\n{source}\nPILOT_DEPLOYER"
    );
    command.program.octafile = FactoryCommandOctafile::new(json!({"version":1,"tasks":{"deploy":{
      "dir":"/workspace","shell":shell,"reports":[{"name":"deployment_receipt","path":"output/deployment.report.json","format":report.reference().identity()}]
    }}}).to_string()).unwrap();
    command.program.tasks = vec!["deploy".into()];
    command.program.credential_profile = None;
    command.candidate_capture_tool = Some(capture.clone());
    command.outputs = vec![EvidenceRequirement::new(
      key("deployment_receipt"),
      EvidenceOutputKind::Report,
      report.reference().clone(),
      command.executor.clone(),
      command.plugin.clone(),
    )];
    let mut grants = serde_json::to_value(&base.project_permissions).unwrap();
    grants["network_hosts"] = json!(["host.microsandbox.internal"]);
    grants["mounts"]
      .as_array_mut()
      .unwrap()
      .push(json!({"root":octacity_protocol::FACTORY_SOURCE_ROOT,"mode":"read_only"}));
    grants["outputs"] = json!({"kinds":["deployment_receipt"],"max_artifact_count":0,"max_artifact_bytes":0,"max_report_count":1,"max_report_bytes":65536});
    command.project_permissions = serde_json::from_value(grants).unwrap();
    command.local_permissions = LocalPermissionCeiling::fully_enforced(command.project_permissions.clone());
    let profile = FactoryDeploymentCommandProfile::new(command, authority).unwrap();
    Self {
      profile,
      report,
      input,
      qualified,
      target,
    }
  }
  pub(super) fn entry(&self) -> Value {
    let authority = self.profile.authority();
    json!({"identity":self.profile.command().executor,"profiles":[workflow_author::command_entry(self.profile.command())],
      "deployment_authority":{"identity":authority.reference().identity(),"version":authority.reference().version(),
        "environment":authority.environment(),"endpoint":authority.endpoint(),"credential_profile":null}})
  }
  pub(super) fn permissions(&self) -> &FactoryPermissionSet {
    &self.profile.command().project_permissions
  }
  pub(super) fn install(&self, catalog: &mut Value, ceiling: &mut FactoryPermissionSet) {
    catalog["promotions"] = json!([self.entry()]);
    let command = self.profile.command();
    catalog["projects"][0]["build_configurations"]
      .as_array_mut()
      .unwrap()
      .push(json!({"alias":"installed-stage-build","reference":command.build_configuration}));
    catalog["projects"][0]["references"].as_array_mut().unwrap().extend([
      json!({"kind":"node_capability","alias":"stage-executor","reference":command.executor}),
      json!({"kind":"node_capability","alias":"stage-policy","reference":command.identity}),
    ]);
    let mut grants = serde_json::to_value(&*ceiling).unwrap();
    grants["network_hosts"]
      .as_array_mut()
      .unwrap()
      .push(json!("host.microsandbox.internal"));
    grants["outputs"]["kinds"]
      .as_array_mut()
      .unwrap()
      .push(json!("deployment_receipt"));
    grants["outputs"]["max_report_bytes"] = json!(grants["outputs"]["max_report_bytes"].as_u64().unwrap().max(65536));
    *ceiling = serde_json::from_value(grants).unwrap();
  }
  pub(super) fn extend(&self, draft: &mut Value, previous: &str, outcome: &str, candidate: &str, budget: BudgetLimit) {
    let command = self.profile.command();
    let deploy_key = format!("{previous}-stage");
    let check_key = format!("{previous}-stage-check");
    let mut deploy = workflow::node(
      &deploy_key,
      "trusted_action",
      &self.report,
      budget,
      &command.project_permissions,
      "observed",
    );
    deploy["input_schema"] = json!(self.input.reference());
    deploy["input_binding"] = json!({"sources":{
      "candidate":{"source":"record","node":candidate,"outcome":"implemented","view":"metadata","path":format!("/build/outputs/{}/data/payload/value",models::CANDIDATE_FACTS)},
      "bundle":{"source":"record","node":candidate,"outcome":"implemented","view":"metadata","path":format!("/build/outputs/{}/artifact",octacity_protocol::CHANGE_SET_BUNDLE_OUTPUT)},
      "manifest":{"source":"record","node":candidate,"outcome":"implemented","view":"metadata","path":format!("/build/outputs/{}/artifact",octacity_protocol::CHANGE_SET_MANIFEST_OUTPUT)}
    },"mapping":{"value":{"mapping":"object","fields":{
      "candidate_revision":{"mapping":"pointer","path":"/candidate/candidate_revision"},
      "candidate_digest":{"mapping":"pointer","path":"/candidate/candidate_digest"},
      "bundle":{"mapping":"pointer","path":"/bundle"},"manifest":{"mapping":"pointer","path":"/manifest"}
    }}}});
    deploy["context_binding"] = json!({"candidate_revision":{"value":{"mapping":"pointer","path":"/candidate_revision"}},"entries":{
      "candidate_bundle":{"source_kind":"evidence","source":{"value":{"mapping":"object","fields":{"kind":{"mapping":"constant","value":"artifact"},"value":{"mapping":"pointer","path":"/bundle"}}}}},
      "candidate_manifest":{"source_kind":"evidence","source":{"value":{"mapping":"object","fields":{"kind":{"mapping":"constant","value":"artifact"},"value":{"mapping":"pointer","path":"/manifest"}}}}}
    }});
    deploy["build"] = json!(FlowBuildBinding {
      tool: command.executor.clone(),
      plugin: command.plugin.clone(),
      result_tool: None,
      capture_write_paths: None,
      capture_deliverables: None,
      model_or_tool: command.identity.clone(),
      task_digest: command.task_digest(),
      build_configuration: command.build_configuration.clone(),
      result_output: key("deployment_receipt"),
      result_output_kind: EvidenceOutputKind::Report,
      result_outcome: key("observed"),
      evidence: vec![],
    });
    let plan = PromotionPolicy {
      strategy: key("configured-pilot-strategy"),
      environments: BTreeMap::from([(key("stage"), self.profile.authority().environment().clone())]),
      allocation: json!({}),
      observation_window_millis: 100,
      evidence_requirements: vec![],
      production_environments: vec![],
      allow_immediate_low_risk: false,
      required_gates: vec![],
      criteria: key(&check_key),
    };
    deploy["action"] =
      json!({"plugin":command.executor,"action":"deploy-to-installed-stage","parameters":{"plan":plan}});
    let mut check = workflow::node(
      &check_key,
      "deterministic_gate",
      &self.report,
      budget,
      &FactoryPermissionSet::deny_all(),
      "continue",
    );
    check["input_schema"] = json!(self.qualified.reference());
    check["outcomes"]
      .as_array_mut()
      .unwrap()
      .push(json!({"key":"failed","kind":"failure","schema":self.report.reference()}));
    check["input_binding"] = json!({"sources":{
      "observation":{"source":"record","node":deploy_key,"outcome":"observed","view":"payload","path":""},
      "freshness":{"source":"record","node":deploy_key,"outcome":"observed","view":"metadata","path":"/build/outputs/deployment_receipt/fresh_until"}
    },"mapping":{"value":{"mapping":"object","fields":{
      "observation":{"mapping":"pointer","path":"/observation"},"fresh_until":{"mapping":"pointer","path":"/freshness"}
    }}}});
    let program: FlowGateProgram = serde_json::from_value(json!({"rules":[{"when":{"operation":"all","predicates":[
      {"operation":"compare","left":{"source":"path","path":"/observation/outcome"},"comparison":"equal","right":{"source":"literal","value":"passed"}},
      {"operation":"compare","left":{"source":"path","path":"/observation/measurements/matching_reads"},"comparison":"equal","right":{"source":"literal","value":2}}
    ]},"outcome":"continue"}],"fallback":"failed","default_output":{"value":{"mapping":"pointer","path":"/observation"}},
      "precondition":{"operation":"compare","left":{"source":"path","path":"/fresh_until"},"comparison":"greater","right":{"source":"observation_time"}}})).unwrap();
    check["gate"] = json!(program.binding().unwrap());
    draft["root"]["nodes"].as_array_mut().unwrap().extend([deploy, check]);
    let transitions = draft["root"]["transitions"].as_array_mut().unwrap();
    let edge = transitions
      .iter_mut()
      .find(|edge| edge["predecessor"] == previous && edge["outcome"] == outcome)
      .unwrap();
    *edge = workflow::edge(previous, outcome, "node", &deploy_key);
    transitions.extend([
      workflow::edge(&deploy_key, "observed", "node", &check_key),
      workflow::edge(&check_key, "continue", "terminal", "finished"),
      workflow::edge(&check_key, "failed", "terminal", "deployment-failed"),
    ]);
    draft["root"]["terminals"][0]["schema"] = json!(self.report.reference());
    draft["root"]["terminals"]
      .as_array_mut()
      .unwrap()
      .push(json!({"key":"deployment-failed","schema":self.report.reference()}));
    draft["data_schemas"].as_array_mut().unwrap().extend([
      json!(self.input),
      json!(self.report),
      json!(self.qualified),
    ]);
  }
  pub(super) async fn verify(&self, client: &Client, origin: &str, rows: &[(Vec<u8>, Vec<u8>)]) {
    let records = rows
      .iter()
      .map(|(_, raw)| serde_json::from_slice::<Value>(raw).unwrap())
      .collect::<Vec<_>>();
    let promotion = records
      .iter()
      .find(|record| record["discovery"]["kind"] == "promotion")
      .unwrap();
    let candidate = records
      .iter()
      .find(|record| record["discovery"]["kind"] == "candidate")
      .unwrap();
    let observation = &promotion["payload"]["value"];
    assert_eq!(observation["subject"], candidate["discovery"]["subject"]);
    let attempts = workflow::diagnostic(
      client,
      origin,
      promotion["input"]["factory_run_id"].as_str().unwrap(),
      "node-attempts",
    )
    .await;
    let attempt = attempts["items"]
      .as_array()
      .unwrap()
      .iter()
      .find(|row| row["record"]["id"] == promotion["node_attempt_id"])
      .expect("promotion retains its exact public node attempt");
    assert!(attempt["record"]["input_digest"].is_string());
    assert_eq!(observation["input_digest"], attempt["record"]["input_digest"]);
    assert_eq!(
      observation["environment_contract"],
      json!(self.profile.authority().environment())
    );
    assert_eq!(observation["outcome"], "passed");
    assert_eq!(observation["measurements"], json!({"matching_reads":2}));
    assert_eq!(
      observation["finished_at"].as_i64().unwrap() - observation["started_at"].as_i64().unwrap(),
      100
    );
    let output = promotion["build"]["outputs"]
      .as_array()
      .unwrap()
      .iter()
      .find(|output| output["name"] == "deployment_receipt")
      .unwrap();
    let bytes =
      workflow_author::download_artifact(client, origin, output["artifact"]["artifact_id"].as_str().unwrap()).await;
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), *observation);
    let live = client
      .get(self.target.live_endpoint())
      .send()
      .await
      .unwrap()
      .bytes()
      .await
      .unwrap();
    assert_eq!(live.as_ref(), b"facts");
    let deployed: Value = client
      .get(format!(
        "{}/{}",
        self.target.endpoint(),
        observation["deployment"]["identity"].as_str().unwrap()
      ))
      .send()
      .await
      .unwrap()
      .json()
      .await
      .unwrap();
    assert_eq!(deployed["deployment"], observation["deployment"]);
    assert_eq!(deployed["candidate"], observation["subject"]);
    assert_eq!(
      deployed["content_sha256"],
      FactoryDigest::content_sha256(b"facts").to_string()
    );
  }
  pub(super) async fn stop(self) {
    self.target.stop().await;
  }
}
