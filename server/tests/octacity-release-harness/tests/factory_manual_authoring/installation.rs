//! Technical installation only: no business Flow, task, gate or demonstration Build.
use super::*;
use octacity_server_application::{FactoryCodexBuildProfile, FactoryCodexUsagePolicy};
use octacity_server_domain::{BuildConfigurationVersion, PipelineNodeId};

pub(super) struct Resources {
  pub project: String,
  pub pool: String,
  pub configuration: BuildConfigurationRef,
}
pub(super) async fn create(client: &Client, origin: &str, backend: &ReleaseBackend) -> Resources {
  let pool = resource_id(&post_management(client,origin,"/api/v1/agent-pools","manual-pool",json!({
    "name":"manual-workers", "definition":{"enabled":true,"drain_state":"accepting","admission_policy":backend.pool_admission_policy(),"concurrency_limit":2,"fairness_policy":"priority_fifo","static_capacity_limit":2}
  })).await);
  let project = resource_id(
    &post_management(
      client,
      origin,
      "/api/v1/projects",
      "manual-project",
      json!({"parent_id":null,"name":"Blank authoring Project"}),
    )
    .await,
  );
  let pipeline = resource_id(&post_management(client,origin,"/api/v1/pipelines","manual-worker",json!({
    "project_id":project,"name":"Installed isolated worker","dag":{"nodes":[{"id":"execute","name":"Managed task","dependency_policy":"all_succeeded","required_capabilities":backend.required_capabilities(),"execution":{"octafile":"Octafile.yml","commands":["execute"],"parallel":false,"failfast":true}}],"edges":[]}
  })).await);
  let outputs = json!({"artifact_count":16,"artifact_bytes":4*1024*1024,"report_count":16,"report_bytes":4*1024*1024,"single_output_bytes":4*1024*1024});
  let published = post_management(client,origin,"/api/v1/build-configurations","manual-build-capability",json!({
    "project_id":project,"name":"Source-free managed worker","definition":{
      "enabled":true,"source":{"kind":"source_free_v1"},"job_concurrency_limit":2,"pipeline_id":pipeline,"pipeline_version":1,
      "parameters":{"parameters":{},"deny_unknown":true},"triggers":["manual"],
      "agent_requirements":{"capabilities":backend.required_capabilities(),"labels":{},"minimum_cpu_millis":1000,"minimum_memory_bytes":536870912,"minimum_disk_bytes":backend.workspace_bytes()},
      "allowed_pools":[pool],"runtime":{"class":backend.runtime_class(),"operating_system":"linux","architecture":backend.guest_architecture(),"host_platform":backend.host_platform(),"required_guarantees":backend.required_guarantees(),"immutable_image":backend.immutable_image(),"cpu_millis":1000,"memory_bytes":536870912,"writable_disk_bytes":backend.workspace_bytes(),"timeout_seconds":120,"network":{"mode":"disabled"},"workload_identity_profile":null},
      "cache":{"namespace":null,"read":false,"write":false},"artifacts":outputs,"retry":{"max_attempts":1,"retry_on":[]}
    }
  })).await;
  let definition = &published["resource"]["definition"];
  let configuration = BuildConfigurationRef::new(
    resource_id(&published).parse().unwrap(),
    BuildConfigurationVersion::INITIAL,
    project.parse().unwrap(),
    FactoryDigest::content_sha256(&serde_json::to_vec(definition).unwrap()),
  );
  let mut policy = support::project_policy_body(&pool, "", backend.runtime_class(), backend.execution_target());
  policy["policy"]["repositories"]["value"] = json!([]);
  policy["policy"]["artifacts"]["value"] = outputs;
  policy["policy"]["concurrency"]["value"] = json!({"active_builds":2,"active_jobs":2});
  post_management(
    client,
    origin,
    &format!("/api/v1/projects/{project}/policy-versions"),
    "manual-policy",
    policy,
  )
  .await;
  Resources {
    project,
    pool,
    configuration,
  }
}

pub(super) fn profile(
  directory: &Path,
  resources: &Resources,
  backend: &ReleaseBackend,
  octa: &Path,
) -> FactoryCodexBuildProfile {
  let executable = directory.join("finite-model");
  fs::write(
    &executable,
    include_bytes!("../../fixtures/factory/manual-model-cli.sh"),
  )
  .unwrap();
  set_mode(&executable, 0o700);
  let runner = octacity_runner::RunnerInstallation::load(octa).unwrap();
  let reference = |name: &str, version: &str, digest: FactoryDigest| {
    ImmutableReference::new(name.parse().unwrap(), version.parse().unwrap(), digest)
  };
  let pin = reference(
    "codex-cli",
    "0.161.0",
    FactoryDigest::content_sha256(&fs::read(executable).unwrap()),
  );
  let plugin = &runner.plugins["codex"];
  let plugin = reference(
    "codex",
    &plugin.version,
    FactoryDigest::from_lower_hex(&plugin.sha256).unwrap(),
  );
  let model = reference(
    "finite-model",
    "v1",
    FactoryDigest::content_sha256(b"finite protocol peer"),
  );
  let run = format!("run-{}", blake3::hash(b"codex").to_hex());
  let exact = |value: &str| CommandArgumentPattern::Exact(FactoryText::new(value).unwrap());
  let arguments = vec![
    exact("--ask-for-approval=never"),
    exact("exec"),
    exact("--json"),
    exact("--sandbox=workspace-write"),
    exact("--model=finite-model"),
    exact(&format!(
      "--output-schema=/workspace/output/codex-runs/{run}/.records.tmp/.result-schema.json"
    )),
    exact("--ignore-user-config"),
    exact("--config=mcp_servers={}"),
    exact("-"),
  ];
  let permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    plugins: runner
      .plugins
      .iter()
      .map(|(name, pin)| reference(name, &pin.version, FactoryDigest::from_lower_hex(&pin.sha256).unwrap()))
      .collect(),
    executables: vec![pin.clone()],
    commands: vec![
      CommandPermission::try_new(pin.clone(), vec![exact("--version")]).unwrap(),
      CommandPermission::try_new(pin.clone(), arguments).unwrap(),
    ],
    mounts: vec![
      MountPermission::new(
        FactoryPath::new(octacity_protocol::PROTECTED_INPUT_ROOT).unwrap(),
        MountMode::ReadOnly,
      ),
      MountPermission::new(
        FactoryPath::new(octacity_protocol::FACTORY_SCRATCH_ROOT).unwrap(),
        MountMode::ReadWrite,
      ),
      MountPermission::new(
        FactoryPath::new(octacity_protocol::FACTORY_OUTPUT_ROOT).unwrap(),
        MountMode::ReadWrite,
      ),
    ],
    max_descendants: 63,
    secret_profiles: vec!["manual-model".parse().unwrap()],
    resources: FactoryResourceLimits::new(1000, 536870912, backend.workspace_bytes(), 64, 120000).unwrap(),
    outputs: FactoryOutputPermissions::try_new(
      ["codex-run-result", "codex-run-provenance", "codex-run-trace"]
        .into_iter()
        .map(|name| name.parse().unwrap())
        .collect(),
      2,
      4 * 1024 * 1024,
      1,
      1024 * 1024,
    )
    .unwrap(),
    ..Default::default()
  })
  .unwrap();
  FactoryCodexBuildProfile {
    authentication: Default::default(),
    mcp_configurations: vec![],
    toolchain: FactoryTaskToolchain {
      plugin,
      executable: pin,
      model: model.clone(),
    },
    build_configuration: resources.configuration.clone(),
    instructions: FactoryText::new("Follow the reviewed step assignment using only selected inputs.").unwrap(),
    credentials: FactoryCredentialProfiles::new(
      "manual-author".parse().unwrap(),
      "manual-model".parse().unwrap(),
      "manual-source".parse().unwrap(),
      "manual-delivery".parse().unwrap(),
    )
    .unwrap(),
    consumer: FactoryCredentialConsumer::ReadOnlyReview,
    usage_policy: FactoryCodexUsagePolicy::TokenRates {
      model,
      input_micro_units_per_million: 1000000,
      cached_input_micro_units_per_million: 1000000,
      output_micro_units_per_million: 1000000,
    },
    tool_risk: None,
    tool_response_limits: None,
    reviewer: None,
    candidate_capture_tool: None,
    capture: None,
    pipeline_node: PipelineNodeId::new("execute").unwrap(),
    parameters: BTreeMap::new(),
    project_permissions: permissions.clone(),
    local_permissions: LocalPermissionCeiling::fully_enforced(permissions),
  }
}

pub(super) fn definition(profile: &FactoryCodexBuildProfile) -> Value {
  json!({"name":"General reasoning","model_identifier":null,"instructions":["Respect the current step and its exact input contract."],"toolchain":profile.toolchain,"model_parameters":{},"thinking_parameter":null,"execution":{},"skills":{},"context_requirements":["task"],"build_configuration":profile.build_configuration,"credential_purpose":"evaluator","permissions":profile.project_permissions,"budget":BudgetLimit::new(1,120000,1000,1000,16*1024*1024).unwrap()})
}
pub(super) fn catalog(
  directory: &Path,
  resources: &Resources,
  profile: &FactoryCodexBuildProfile,
  codes: &[&FactoryCodexBuildProfile],
) -> PathBuf {
  let reference = |name: &str| {
    ImmutableReference::new(
      name.parse().unwrap(),
      "v1".parse().unwrap(),
      FactoryDigest::content_sha256(name.as_bytes()),
    )
  };
  let mut references = vec![
    json!({"kind":"admission_policy","alias":"manual-admission","reference":reference("manual.admission")}),
    json!({"kind":"permission_ceiling","alias":"manual-ceiling","reference":reference("manual.ceiling")}),
    json!({"kind":"secret_profile","alias":"manual-model","reference":reference("manual.model-credential")}),
    json!({"kind":"node_capability","alias":"manual-runtime","reference":profile.toolchain.executable}),
  ];
  references[1]["metadata"] = json!({
    "node_kinds":[], "signal_purposes":[], "enforcement":PermissionCategory::ALL,
    "compatible_choices":[], "permission_declarations":profile.project_permissions
  });
  references.extend(
    profile
      .project_permissions
      .plugins()
      .chain(profile.project_permissions.executables())
      .chain(std::iter::once(&profile.toolchain.model))
      .map(|reference| json!({"kind":"node_capability","alias":reference.identity(),"reference":reference})),
  );
  let mut body = json!({"version":1,"projects":[{"project_id":resources.project,"references":references,"build_configurations":[{"alias":"isolated-worker","reference":resources.configuration}],"role_profiles":{"manual-runtime":{"ceiling":definition(profile),"parameters":FlowDataSchema::new("parameters".parse().unwrap(),"v1".parse().unwrap(),FlowValueSchema::Object{fields:BTreeMap::new()}).unwrap(),"public_environment":{},"enforcement":PermissionCategory::ALL}}}],"codex":[{"toolchain":profile.toolchain,"build_configuration":profile.build_configuration,"instructions":profile.instructions,"credentials":profile.credentials,"credential_purpose":"evaluator","usage_policy":profile.usage_policy,"pipeline_node":profile.pipeline_node,"parameters":{},"mcp_configurations":[],"project_permissions":profile.project_permissions,"local_permissions":{"grants":profile.local_permissions.grants(),"enforceable":PermissionCategory::ALL}}]});
  for (index, code) in codes.iter().enumerate() {
    let capability_alias = if index == 0 {
      "manual-code-runtime".to_owned()
    } else {
      format!("manual-code-runtime-{index}")
    };
    let build_alias = if index == 0 {
      "code-worker".to_owned()
    } else {
      format!("code-worker-{index}")
    };
    let project = &mut body["projects"][0];
    project["references"][1]["metadata"]["permission_declarations"] =
      serde_json::to_value(&code.project_permissions).unwrap();
    project["references"]
      .as_array_mut()
      .unwrap()
      .push(json!({"kind":"node_capability","alias":capability_alias,"reference":code.toolchain.executable}));
    project["build_configurations"]
      .as_array_mut()
      .unwrap()
      .push(json!({"alias":build_alias,"reference":code.build_configuration}));
    let mut capability = project["role_profiles"]["manual-runtime"].clone();
    capability["ceiling"] = definition(code);
    project["role_profiles"][&capability_alias] = capability;
    let mut compiler = body["codex"][0].clone();
    compiler["build_configuration"] = serde_json::to_value(&code.build_configuration).unwrap();
    compiler["project_permissions"] = serde_json::to_value(&code.project_permissions).unwrap();
    compiler["local_permissions"]["grants"] = serde_json::to_value(code.local_permissions.grants()).unwrap();
    body["codex"].as_array_mut().unwrap().push(compiler);
  }
  private_file(directory, "profiles.json", &body.to_string())
}
pub(super) async fn publish_role(
  client: &Client,
  origin: &str,
  resources: &Resources,
  profile: &FactoryCodexBuildProfile,
  name: &str,
) -> Value {
  let path = format!("/api/v1/projects/{}/factory-role-publications", resources.project);
  let mut publication=post_management(client,origin,&path,&format!("{name}-role"),json!({
    "role":{"id":Uuid::new_v4(),"expected_current_version":null,"definition":{"name":name,"instructions":["Work only on the assigned task."]}},"profiles":[]
  })).await;
  let profiles=post_management(client,origin,&path,&format!("{name}-profile"),json!({
    "profiles":[{"id":Uuid::new_v4(),"expected_current_version":null,"role":publication["role"]["reference"],"definition":definition(profile)}]
  })).await;
  publication["profiles"] = profiles["profiles"].clone();
  publication
}
