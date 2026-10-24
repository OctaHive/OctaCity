//! First native execution slice: real published inputs, nested Flow and evaluator Job.
use super::*;
use models::InstalledModel;
use octacity_release_harness::InstalledRelease;
use octacity_server_factory::*;
use resources::PilotResources;
use std::collections::BTreeMap;

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn reference(value: &str) -> ImmutableReference {
  ImmutableReference::new(key(value), key("v1"), FactoryDigest::content_sha256(value.as_bytes()))
}

#[expect(
  clippy::too_many_arguments,
  reason = "packaged fixture explicitly supplies independently owned resources"
)]
pub(super) async fn run_model(
  directory: &Path,
  release: &InstalledRelease,
  resources: &PilotResources,
  backend: &ReleaseBackend,
  model: &InstalledModel,
  postgres: &str,
  management_addr: SocketAddr,
  agent_addr: SocketAddr,
  revision: &str,
  artifacts: &Value,
  tools: Option<&workflow_tools::InstalledTools>,
  role: Option<&workflow_roles::RolePilot>,
) {
  let profile = &model.profile;
  let credential_profile = profile.credentials.profile(profile.consumer.purpose());
  let mut references = vec![
    json!({"kind":"admission_policy","alias":"pilot-admission","reference":reference("pilot.admission")}),
    json!({"kind":"permission_ceiling","alias":"pilot-ceiling","reference":reference("pilot.ceiling")}),
    json!({"kind":"secret_profile","alias":credential_profile,"reference":reference("pilot.evaluator-profile")}),
  ];
  references.extend(
    profile
      .project_permissions
      .plugins()
      .chain(profile.project_permissions.executables())
      .chain(profile.project_permissions.tools())
      .chain(std::iter::once(&profile.toolchain.model))
      .map(|reference| json!({"kind":"node_capability","alias":reference.identity(),"reference":reference})),
  );
  let mut catalog = json!({"version":1,"projects":[{"project_id":resources.project,
    "references":references,"build_configurations":[{"alias":"pilot-observer","reference":resources.configuration}]}],
    "codex":[{"toolchain":{"plugin":profile.toolchain.plugin,"executable":profile.toolchain.executable,"model":profile.toolchain.model},
      "build_configuration":profile.build_configuration,"instructions":profile.instructions,"credentials":profile.credentials,
      "credential_purpose":"evaluator","usage_policy":profile.usage_policy,"pipeline_node":profile.pipeline_node,
      "parameters":profile.parameters,"mcp_configurations":profile.mcp_configurations,"project_permissions":profile.project_permissions,
      "local_permissions":{"grants":profile.local_permissions.grants(),"enforceable":PermissionCategory::ALL}}]});
  if let Some(tools) = tools {
    tools.catalog(&mut catalog, profile);
  }
  if let Some(role) = role {
    role.catalog(&mut catalog);
  }
  let profiles = private_file(directory, "profiles.json", &catalog.to_string());
  let connectors = tools.map_or_else(
    || private_file(directory, "connectors.json", r#"{"version":1,"connectors":[]}"#),
    |tools| tools.connectors(directory),
  );
  let objects = required_string("OCTACITY_MINIO_ENDPOINT");
  let config = write_factory_server_config(
    directory,
    &ServerConfigInput {
      postgres_url: postgres,
      object_endpoint: &objects,
      cache_endpoint: "https://cache.example",
      toolchain: &release.toolchain,
      management_addr,
      agent_addr,
      cache_addr: None,
    },
    &FactoryServerFiles {
      profiles: &profiles,
      connectors: &connectors,
    },
  );
  private_file(
    directory,
    "object-access-key",
    &required_string("OCTACITY_MINIO_ACCESS_KEY"),
  );
  private_file(
    directory,
    "object-secret-key",
    &required_string("OCTACITY_MINIO_SECRET_KEY"),
  );
  let client = Client::new();
  let origin = format!("http://{management_addr}");
  let server_log = directory.join("factory-server.stdout.log");
  let mut server = spawn_server(
    &release.server_binary,
    &config,
    &server_log,
    &directory.join("factory-server.stderr.log"),
  );
  wait_for_server_ready(&client, &origin, &mut server, &server_log).await;
  if let Some(role) = role {
    role.check_model_catalog(&client, &origin, &resources.project).await;
  }

  let installation = directory.join("observer");
  fs::create_dir(&installation).unwrap();
  set_mode(&installation, 0o700);
  let credential = private_file(
    &installation,
    "evaluator.json",
    &json!({"OCTACITY_CODEX_AUTH":if tools.is_some(){workflow_tools::MODEL_CANARY}else{"finite-evaluator-private-canary"}}).to_string(),
  );
  let credential_profiles = BTreeMap::from([(credential_profile.as_str().to_owned(), credential)]);
  let grants = octacity_server_store::factory_job_permissions(&profile.project_permissions).unwrap();
  let tool = agent::AgentToolExecutable {
    product: "codex-cli",
    path: &model.executable,
    version: "0.161.0",
    platform: &model.platform,
    sha256: &profile.toolchain.executable.digest().to_string(),
  };
  let executables = [tool];
  let origins = [objects.as_str()];
  let mut overrides = agent::AgentConfigOverrides::restricted_without_outputs(&origins);
  overrides.factory = Some(agent::AgentFactoryOverrides {
    permissions: &grants,
    credential_profiles: &credential_profiles,
    capture: None,
    protected_reader: tools.map(|tools| &tools.reader),
  });
  overrides.tool_executables = &executables;
  overrides.output_limits = Some(octacity_protocol::OutputLimits {
    artifact_count: 16,
    artifact_bytes: 4 * 1024 * 1024,
    report_count: 16,
    report_bytes: 4 * 1024 * 1024,
    single_output_bytes: 4 * 1024 * 1024,
  });
  let identity = Uuid::new_v4().simple().to_string();
  let enrollment = resources::issue_enrollment(&client, &origin, &identity, &resources.pool, backend).await;
  let source_plugins = directory.join("source-mirror/source-plugins");
  let agent_config = agent::write_agent_config(
    &installation,
    &format!("http://{agent_addr}"),
    &format!("observer-{identity}"),
    &enrollment,
    agent::AgentReleasePaths {
      octa_root: &release.octa_root,
      source_plugins: &source_plugins,
    },
    backend,
    &overrides,
  );
  let mut worker = agent::spawn_agent(
    &release.agent_binary,
    &agent_config,
    &installation.join("stdout.log"),
    &installation.join("stderr.log"),
  );

  if let Some(tools) = tools {
    tools
      .require_ready(
        &client,
        &origin,
        &resources.pool,
        &format!("observer-{identity}"),
        &mut worker,
      )
      .await;
  }
  let schema = FlowDataSchema::new(
    key(if tools.is_some() {
      "pilot.projected-text"
    } else {
      "pilot.projected-boolean"
    }),
    key("v1"),
    if tools.is_some() {
      FlowValueSchema::String {
        min_bytes: 5,
        max_bytes: 5,
      }
    } else {
      FlowValueSchema::Boolean
    },
  )
  .unwrap();
  let value = if tools.is_some() { json!("facts") } else { json!(true) };
  let budget = BudgetLimit::new(
    8,
    600_000,
    if tools.is_some() { 10000 } else { 1000 },
    if tools.is_some() { 10000 } else { 1000 },
    16 * 1024 * 1024,
  )
  .unwrap();
  let ceiling = tools.map_or(&profile.project_permissions, |tools| &tools.permissions);
  let selected = format!("observation-{identity}");
  let call = format!("nested-{identity}");
  let hold = format!("approval-{identity}");
  let permissions = &profile.project_permissions;
  let binding = FlowBuildBinding {
    tool: profile.toolchain.executable.clone(),
    plugin: profile.toolchain.plugin.clone(),
    result_tool: None,
    capture_write_paths: None,
    capture_deliverables: None,
    model_or_tool: profile.toolchain.model.clone(),
    task_digest: profile.task_digest(),
    build_configuration: profile.build_configuration.clone(),
    result_output: key("codex-run-result"),
    result_output_kind: EvidenceOutputKind::Report,
    result_outcome: key("observed"),
    evidence: vec![],
  };
  let mut observation = node(&selected, "reasoning", &schema, budget, permissions, "observed");
  observation["build"] = json!(&binding);
  if let Some(role) = role {
    role.select(&mut observation, binding, model, artifacts);
  }
  observation["input_binding"]["mapping"]["value"]["value"] = value.clone();
  let child=post_management(&client,&origin,"/api/v1/flow-definitions",&format!("child-{identity}"),json!({
    "id":Uuid::new_v4(),"project_id":resources.project,
    "definition":draft(&selected,vec![observation],vec![edge(&selected,"observed","terminal","returned")],"returned",&schema,budget,ceiling)
  })).await;
  let mut nested = node(&call, "subflow_call", &schema, budget, ceiling, "returned");
  nested["input_binding"]["mapping"]["value"]["value"] = value.clone();
  nested["subflow"] = child["resource"]["flow"]["closure"]["root"].clone();
  let mut human = node(&hold, "human_gate", &schema, budget, ceiling, "approved");
  human["input_binding"]["mapping"]["value"]["value"] = value.clone();
  let parent=post_management(&client,&origin,"/api/v1/flow-definitions",&format!("root-{identity}"),json!({
    "id":Uuid::new_v4(),"project_id":resources.project,
    "definition":draft(&call,vec![nested,human],vec![edge(&call,"returned","node",&hold),edge(&hold,"approved","terminal","finished")],"finished",&schema,budget,ceiling)
  })).await;
  let configuration = post_management(
    &client,
    &origin,
    "/api/v1/factory-configurations",
    &format!("factory-{identity}"),
    json!({
      "id":Uuid::new_v4(),"project_id":resources.project,"definition":{
        "admission_policy":"pilot-admission","permission_ceiling":"pilot-ceiling","hard_budget":budget,
        "wip_limits":{"max_active_runs":4,"max_active_nodes":4},"enabled":true,"flow":parent["resource"]["flow"]
      }
    }),
  )
  .await;
  let artifact = |name: &str| {
    let artifact = artifacts["items"]
      .as_array()
      .unwrap()
      .iter()
      .find(|item| item["name"] == name)
      .unwrap();
    json!({"artifact_id":artifact["id"],"content_digest":artifact["sha256"],"encoded_size":artifact["size_bytes"]})
  };
  let admission = post_management(
    &client,
    &origin,
    &format!("/api/v1/projects/{}/factory-work", resources.project),
    &format!("work-{identity}"),
    json!({
      "configuration_id":configuration["resource"]["configuration"]["reference"]["id"],"configuration_version":1,
      "repository_id":resources.repository,"repository_version":1,"external_identity":format!("pilot/{identity}"),
      "source":{"kind":"exact_revision","value":revision},
      "artifacts":{"task":artifact("task"),"acceptance":artifact("acceptance"),"specifications":[]},
      "classification":{"priority":40,"risk":"low","metadata":{}}
    }),
  )
  .await;
  let run = string(&admission, "factory_run_id");
  let attempt = timeout(Duration::from_secs(180), async {
    loop {
      let attempts = diagnostic(&client, &origin, &run, "node-attempts").await;
      if let Some(row) = attempts["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["record"]["node_key"] == hold)
      {
        break row["record"].clone();
      }
      if let Some(status) = worker.try_wait().unwrap() {
        panic!(
          "observer Agent exited: {status}; private evidence: {}",
          directory.display()
        );
      }
      let links = diagnostic(&client, &origin, &run, "builds").await;
      for link in links["items"].as_array().unwrap() {
        let build = get_json(
          &client,
          format!("{origin}/api/v1/builds/{}", string(&link["record"], "build_id")),
        )
        .await;
        assert!(
          !["failed", "cancelled"].contains(&build["state"].as_str().unwrap_or("")),
          "evaluator Build failed: {build}; private evidence: {}",
          directory.display()
        );
      }
      let state = get_json(&client, format!("{origin}/api/v1/factory-runs/{run}")).await;
      assert!(
        !["failed", "cancelled", "completed"].contains(&state["state"].as_str().unwrap_or("")),
        "observer stopped before operator gate: {state}; private evidence: {}",
        directory.display()
      );
      sleep(Duration::from_millis(100)).await;
    }
  })
  .await
  .expect("real evaluator did not complete before the operator gate");
  let completed = diagnostic(&client, &origin, &run, "node-completions").await;
  assert!(
    completed["items"]
      .as_array()
      .unwrap()
      .iter()
      .any(|row| row["record"]["node_key"] == selected)
  );
  let builds = diagnostic(&client, &origin, &run, "builds").await;
  assert_eq!(
    builds["items"].as_array().unwrap().len(),
    1,
    "one actual evaluator Build"
  );
  if let Some(tools) = tools {
    tools.verify(&client, &origin, &run, &builds, postgres).await;
  }
  let calls = diagnostic(&client, &origin, &run, "calls").await;
  assert_eq!(
    calls["items"].as_array().unwrap().len(),
    1,
    "one frozen model invocation"
  );
  assert_eq!(
    diagnostic(&client, &origin, &run, "flow-runs").await["items"]
      .as_array()
      .unwrap()
      .len(),
    2,
    "actual nested closure"
  );

  let retained = if let Some(role) = role {
    let snapshot = role.retained(postgres, &builds).await;
    role.replace_catalog(&client, &origin, &resources.project).await;
    Some(snapshot)
  } else {
    None
  };
  worker.kill().await.unwrap();
  worker.wait().await.unwrap();
  shutdown_server(&mut server, &server_log).await;
  let mut server = spawn_server(
    &release.server_binary,
    &config,
    &server_log,
    &directory.join("factory-server.stderr.log"),
  );
  wait_for_server_ready(&client, &origin, &mut server, &server_log).await;
  let mut worker = agent::spawn_agent(
    &release.agent_binary,
    &agent_config,
    &installation.join("restart.stdout.log"),
    &installation.join("restart.stderr.log"),
  );
  if let Some(role) = role {
    assert_eq!(
      Some(role.retained(postgres, &builds).await),
      retained,
      "retained package survives catalog replacement and process restart exactly"
    );
  }
  assert_eq!(
    diagnostic(&client, &origin, &run, "node-completions").await,
    completed,
    "verified receipts survive process restart exactly"
  );
  assert_eq!(
    diagnostic(&client, &origin, &run, "builds").await,
    builds,
    "restart preserves exact Build lineage"
  );
  assert_eq!(
    diagnostic(&client, &origin, &run, "calls").await,
    calls,
    "restart preserves frozen call identity"
  );
  let detail = client
    .get(format!("{origin}/api/v1/factory-runs/{run}"))
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap();
  let etag = detail.headers()["etag"].to_str().unwrap().to_owned();
  let response = client
    .post(format!(
      "{origin}/api/v1/factory-runs/{run}/nodes/{}/responses",
      string(&attempt, "id")
    ))
    .header("idempotency-key", format!("approve-{identity}"))
    .header("if-match", etag)
    .json(&json!({"outcome":"approved","value":value}))
    .send()
    .await
    .unwrap();
  assert!(
    response.status().is_success(),
    "operator response: {}",
    response.text().await.unwrap()
  );
  // A restarted coordinator must first let the former owner's durable claim
  // expire. Leave another claim interval for acquisition and completion.
  let installed_config: toml::Value = toml::from_str(&fs::read_to_string(&config).unwrap()).unwrap();
  let claim_lifetime = Duration::from_millis(
    installed_config["factory"]["claim_lifetime_milliseconds"]
      .as_integer()
      .unwrap()
      .try_into()
      .unwrap(),
  );
  timeout(claim_lifetime * 3, async {
    loop {
      let state = get_json(&client, format!("{origin}/api/v1/factory-runs/{run}")).await;
      if state["state"] == "completed" {
        break;
      }
      sleep(Duration::from_millis(100)).await;
    }
  })
  .await
  .expect("restarted owner completes the admitted immutable Flow");
  assert_eq!(
    diagnostic(&client, &origin, &run, "builds").await,
    builds,
    "no duplicate evaluator after restart"
  );
  assert!(
    !diagnostic(&client, &origin, &run, "audit").await["items"]
      .as_array()
      .unwrap()
      .is_empty()
  );
  worker.kill().await.unwrap();
  worker.wait().await.unwrap();
  shutdown_server(&mut server, &server_log).await;
}

pub(super) async fn diagnostic(client: &Client, origin: &str, run: &str, collection: &str) -> Value {
  get_json(
    client,
    format!("{origin}/api/v1/factory-runs/{run}/{collection}?limit=100"),
  )
  .await
}
pub(super) fn node(
  name: &str,
  kind: &str,
  schema: &FlowDataSchema,
  budget: BudgetLimit,
  permissions: &FactoryPermissionSet,
  outcome: &str,
) -> Value {
  json!({"key":name,"kind":kind,"input_schema":schema.reference(),"outcomes":[{"key":outcome,"kind":"success","schema":schema.reference()}],
    "budget":budget,"permissions":permissions,"required":true,"subflow":null,
    "input_binding":{"sources":{},"mapping":{"value":{"mapping":"constant","value":true}}}})
}
pub(super) fn edge(from: &str, outcome: &str, kind: &str, to: &str) -> Value {
  json!({"predecessor":from,"outcome":outcome,"target":{"kind":kind,"key":to},"max_repeats":0,"exhausted_terminal":null,"starts_cycle":false})
}
pub(super) fn draft(
  entry: &str,
  nodes: Vec<Value>,
  transitions: Vec<Value>,
  terminal: &str,
  schema: &FlowDataSchema,
  budget: BudgetLimit,
  permissions: &FactoryPermissionSet,
) -> Value {
  json!({"root":{"input_schema":schema.reference(),"entry":entry,"nodes":nodes,"transitions":transitions,
    "terminals":[{"key":terminal,"schema":schema.reference()}],"context_projections":[],"data_projections":[],
    "execution":{"budget":budget,"permissions":permissions,"max_active_nodes":4}},
    "limits":{"max_depth":4,"max_expanded_nodes":16,"max_repeat_count":1,"max_fan_out":4,"max_wip":4,"budget":budget,"permissions":permissions},
    "data_schemas":[schema],"pools":{}})
}
