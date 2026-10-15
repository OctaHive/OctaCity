//! Actual installed command, lease-bound server finalizer and opaque private bundle.
use super::*;
use octacity_release_harness::InstalledRelease;
use octacity_runner::RunnerInstallation;
use octacity_server_application::*;
use octacity_server_factory::*;
use resources::PilotResources;
use std::collections::BTreeMap;

const PRIVATE_BODY: &[u8] = b"assert_eq!(actual, PRIVATE_EXPECTATION);";
const EXECUTABLE_PRIVATE_BODY: &[u8] = br#"{"path":"public/facts.txt","expected":"facts"}"#;
const PRIVATE_OUTPUT: &str = "hidden.contract";
const INVENTORY_OUTPUT: &str = "private.inventory";
const PUBLIC_OUTPUT: &str = "public.handoff";
fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn reference(value: &str) -> ImmutableReference {
  ImmutableReference::new(key(value), key("v1"), FactoryDigest::content_sha256(value.as_bytes()))
}

/// Joins only declared fixture grants. Agent and Server ceilings are composed
/// separately so a Server connector credential never enters an Agent profile.
pub(super) fn include_permissions(ceiling: &mut FactoryPermissionSet, extra: &FactoryPermissionSet) {
  let mut joined = serde_json::to_value(&*ceiling).unwrap();
  let mounts = joined["mounts"].as_array_mut().unwrap();
  for mount in extra.mounts() {
    let grant = json!({"root":mount.root().as_str(),"mode":mount.mode().as_str()});
    if let Some(previous) = mounts
      .iter_mut()
      .find(|previous| previous["root"] == mount.root().as_str())
    {
      if mount.mode() == MountMode::ReadWrite {
        *previous = grant;
      }
    } else {
      mounts.push(grant);
    }
  }
  let extra = serde_json::to_value(extra).unwrap();
  for category in [
    "plugins",
    "executables",
    "commands",
    "tools",
    "secret_profiles",
    "network_hosts",
  ] {
    let values = joined[category].as_array_mut().unwrap();
    for value in extra[category].as_array().unwrap() {
      if !values.contains(value) {
        values.push(value.clone());
      }
    }
  }
  for field in [
    "cpu_millis",
    "memory_bytes",
    "disk_bytes",
    "process_count",
    "elapsed_millis",
  ] {
    joined["resources"][field] = json!(
      joined["resources"][field]
        .as_u64()
        .unwrap()
        .max(extra["resources"][field].as_u64().unwrap())
    );
  }
  joined["max_descendants"] = json!(
    joined["max_descendants"]
      .as_u64()
      .unwrap()
      .max(extra["max_descendants"].as_u64().unwrap())
  );
  *ceiling = serde_json::from_value(joined).unwrap();
}

#[test]
fn fixture_ceiling_preserves_distinct_job_scopes_with_one_mount_per_root() {
  let permissions = |mode| {
    FactoryPermissionSet::try_new(FactoryPermissionDraft {
      mounts: vec![MountPermission::new(
        FactoryPath::new("/workspace/source").unwrap(),
        mode,
      )],
      ..Default::default()
    })
    .unwrap()
  };
  let read = permissions(MountMode::ReadOnly);
  let write = permissions(MountMode::ReadWrite);
  for (mut ceiling, extra) in [(read.clone(), write.clone()), (write.clone(), read.clone())] {
    include_permissions(&mut ceiling, &extra);
    assert!(read.is_no_broader_than(&ceiling));
    assert!(write.is_no_broader_than(&ceiling));
    assert_eq!(ceiling.mounts().count(), 1);
    assert_eq!(read.mounts().next().unwrap().mode(), MountMode::ReadOnly);
  }
}

#[test]
fn fixture_candidate_freshness_uses_the_declared_continuation_budgets() {
  let draft = json!({"root":{
    "nodes":[
      {"key":"captured","budget":{"max_elapsed_millis":120000}},
      {"key":"original-check","budget":{"max_elapsed_millis":300000}},
      {"key":"readonly-review","budget":{"max_elapsed_millis":90000}},
      {"key":"publish","budget":{"max_elapsed_millis":120000}},
      {"key":"earlier-role","budget":{"max_elapsed_millis":900000}}
    ],
    "transitions":[
      workflow::edge("earlier-role","done","node","captured"),
      workflow::edge("captured","done","node","original-check"),
      workflow::edge("original-check","passed","node","readonly-review"),
      workflow::edge("original-check","failed","terminal","stopped"),
      workflow::edge("readonly-review","accepted","node","publish"),
      workflow::edge("publish","done","terminal","finished")
    ]
  }});
  assert_eq!(candidate_continuation_millis(&draft, "captured"), 510000);
}

/// Conservative reservation for this acyclic pilot: count each reachable
/// downstream node once, including a subflow call's declared child reservation.
fn candidate_continuation_millis(draft: &Value, producer: &str) -> u64 {
  let transitions = draft["root"]["transitions"].as_array().unwrap();
  assert!(
    transitions
      .iter()
      .all(|edge| edge["max_repeats"] == 0 && edge["starts_cycle"] == false)
  );
  let mut pending = vec![producer];
  let mut visited = std::collections::BTreeSet::new();
  let mut remaining = 0_u64;
  while let Some(key) = pending.pop() {
    if !visited.insert(key) {
      continue;
    }
    if key != producer {
      let node = draft["root"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["key"] == key)
        .unwrap();
      remaining = remaining
        .checked_add(node["budget"]["max_elapsed_millis"].as_u64().unwrap())
        .unwrap();
    }
    for edge in transitions
      .iter()
      .filter(|edge| edge["predecessor"] == key && edge["target"]["kind"] == "node")
    {
      let next = edge["target"]["key"].as_str().unwrap();
      assert_ne!(
        next, producer,
        "this fixture does not declare a candidate-producing cycle"
      );
      pending.push(next);
    }
  }
  remaining
}

pub(super) struct AuthorScenario<'a> {
  pub(super) directory: &'a Path,
  pub(super) release: &'a InstalledRelease,
  pub(super) resources: &'a PilotResources,
  pub(super) backend: &'a ReleaseBackend,
  pub(super) postgres: &'a str,
  pub(super) management_addr: SocketAddr,
  pub(super) agent_addr: SocketAddr,
  pub(super) revision: &'a str,
  pub(super) artifacts: &'a Value,
  pub(super) coder: Option<&'a models::InstalledModel>,
  pub(super) validate: bool,
  pub(super) review: bool,
  pub(super) deliver: bool,
  pub(super) prelude: bool,
  pub(super) deployment_configuration: Option<&'a BuildConfigurationRef>,
  pub(super) repository_locator: &'a str,
  pub(super) isolated: Option<&'a workflow_verification::VerifierResources>,
}

fn installed_profile(s: &AuthorScenario<'_>, schema: &FlowDataSchema) -> FactoryProtectedTestAuthoringProfile {
  let runner = RunnerInstallation::load(&s.release.octa_root).unwrap();
  let shell = &runner.plugins["shell"];
  let plugin = ImmutableReference::new(
    key("shell"),
    key(&shell.version),
    FactoryDigest::from_lower_hex(&shell.sha256).unwrap(),
  );
  let body = if s.coder.is_some() {
    EXECUTABLE_PRIVATE_BODY
  } else {
    PRIVATE_BODY
  };
  let command = if s.prelude {
    format!(
      "node --input-type=module - /octacity/protected/command-input.json /workspace/output private-contract.rs <<'PILOT_REQUIREMENTS_AUTHOR'\n{}\nPILOT_REQUIREMENTS_AUTHOR",
      include_str!("../../fixtures/factory/requirements-author.mjs")
    )
  } else {
    format!(
      "printf '%s' '{}' > /workspace/output/private-contract.rs",
      std::str::from_utf8(body).unwrap().replace('\'', "'\"'\"'")
    )
  };
  let octafile = FactoryCommandOctafile::new(json!({"version":1,"tasks":{"author":{
    "dir":"/workspace",
    "shell":command,
    "artifacts":[{"name":PRIVATE_OUTPUT,"path":"output/private-contract.rs","content_type":"application/octet-stream"}]
  }}}).to_string()).unwrap();
  let executor = ImmutableReference::new(
    key("installed-finite-test-author"),
    key("v1"),
    FactoryDigest::content_sha256(octafile.as_str().as_bytes()),
  );
  let inventory = EvidenceRequirement::new(
    key(INVENTORY_OUTPUT),
    EvidenceOutputKind::Artifact,
    reference("private-inventory-format"),
    executor.clone(),
    plugin.clone(),
  );
  let permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    plugins: runner
      .plugins
      .iter()
      .map(|(name, pin)| {
        ImmutableReference::new(
          key(name),
          key(&pin.version),
          FactoryDigest::from_lower_hex(&pin.sha256).unwrap(),
        )
      })
      .collect(),
    max_descendants: 63,
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
    resources: FactoryResourceLimits::new(1000, 536_870_912, s.backend.workspace_bytes(), 64, 120_000).unwrap(),
    outputs: FactoryOutputPermissions::try_new(
      vec![key(PRIVATE_OUTPUT), key(INVENTORY_OUTPUT), key(PUBLIC_OUTPUT)],
      2,
      65536,
      1,
      4096,
    )
    .unwrap(),
    ..Default::default()
  })
  .unwrap();
  let policy = FactoryProtectedTestAuthoringPolicy::new(
    FactoryProtectedTestPolicy::new(
      ProtectedTestCoverage::new(vec![key("fixture.contract")], 1, 1024, if s.validate { 6 } else { 4 }).unwrap(),
      // Freeze retention for the declared journey; later observers cannot
      // renew this policy when reconstructing the original validation proof.
      if s.prelude { 3_600_000 } else { 600_000 },
    )
    .unwrap(),
    inventory.clone(),
    vec![FactoryProtectedTestFileOutput {
      output: key(PRIVATE_OUTPUT),
      class: key("fixture.contract"),
      path: FactoryRepositoryPath::new("contract.rs").unwrap(),
    }],
  )
  .unwrap();
  let command = FactoryCommandBuildProfile {
    identity: ImmutableReference::new(key("installed-author-policy"), key("v1"), policy.digest().unwrap()),
    plugin: plugin.clone(),
    executor: executor.clone(),
    build_configuration: s.resources.configuration.clone(),
    pipeline_node: s.resources.pipeline_node.clone(),
    program: FactoryCommandProgram {
      octafile,
      tasks: vec!["author".into()],
      workspace_root: Some(octacity_protocol::FACTORY_WORKSPACE_ROOT.into()),
      credential_profile: None,
    },
    input_destination: "/octacity/protected/command-input.json".into(),
    parameters: BTreeMap::new(),
    project_permissions: permissions.clone(),
    local_permissions: LocalPermissionCeiling::fully_enforced(permissions),
    candidate_capture_tool: None,
    outputs: vec![
      EvidenceRequirement::new(
        key(PUBLIC_OUTPUT),
        EvidenceOutputKind::Report,
        schema.reference().clone(),
        executor,
        plugin,
      ),
      inventory,
    ],
  };
  FactoryProtectedTestAuthoringProfile::new(command, policy).unwrap()
}

pub(super) fn command_entry(command: &FactoryCommandBuildProfile) -> Value {
  json!({"identity":command.identity,"plugin":command.plugin,"executor":command.executor,
    "build_configuration":command.build_configuration,"pipeline_node":command.pipeline_node,"program":command.program,
    "input_destination":command.input_destination,"parameters":command.parameters,"project_permissions":command.project_permissions,
    "local_permissions":{"grants":command.local_permissions.grants(),"enforceable":PermissionCategory::ALL},
    "candidate_capture_tool":command.candidate_capture_tool,"outputs":command.outputs})
}

fn handoff_schema() -> FlowDataSchema {
  FlowDataSchema::new(
    key("closed-author-handoff"),
    key("v1"),
    FlowValueSchema::Object {
      fields: BTreeMap::from([
        (
          key("accepted_contract"),
          FlowFieldSchema::required(FlowValueSchema::String {
            min_bytes: 64,
            max_bytes: 64,
          }),
        ),
        (
          key("classes"),
          FlowFieldSchema::required(FlowValueSchema::Array {
            item: Box::new(FlowValueSchema::String {
              min_bytes: 1,
              max_bytes: 64,
            }),
            min_items: 1,
            max_items: 1,
          }),
        ),
      ]),
    },
  )
  .unwrap()
}

fn flow_definition(
  profile: &FactoryProtectedTestAuthoringProfile,
  public_schema: &FlowDataSchema,
  budget: BudgetLimit,
  acceptance: &str,
  selected: &str,
) -> Value {
  let command = profile.command();
  let accepted_schema = FlowDataSchema::new(key("accepted-contract"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let mut gate = workflow::node(
    acceptance,
    "deterministic_gate",
    &accepted_schema,
    budget,
    &FactoryPermissionSet::deny_all(),
    "accepted",
  );
  gate["gate"] = json!(
    FlowGateProgram::new(vec![], key("accepted"))
      .unwrap()
      .with_default_output_mapping(FlowDataMapping::new(FlowValueMapping::Pointer { path: String::new() }).unwrap())
      .unwrap()
      .binding()
      .unwrap()
  );
  gate["accepted_work_outcomes"] = json!(["accepted"]);
  let mut author = workflow::node(
    selected,
    "reasoning",
    public_schema,
    budget,
    &command.project_permissions,
    "authored",
  );
  author["input_schema"] = json!(accepted_schema.reference());
  author["input_binding"] = json!(
    FlowInputBinding::new(
      BTreeMap::from([(
        key("accepted"),
        FlowInputSource::Record {
          node: key(acceptance),
          outcome: key("accepted"),
          view: FlowRecordView::Payload,
          path: String::new(),
        }
      )]),
      FlowDataMapping::new(FlowValueMapping::Pointer {
        path: "/accepted".into()
      })
      .unwrap()
    )
    .unwrap()
  );
  author["build"] = json!(FlowBuildBinding {
    tool: command.executor.clone(),
    plugin: command.plugin.clone(),
    result_tool: None,
    capture_write_paths: None,
    capture_deliverables: None,
    model_or_tool: command.identity.clone(),
    task_digest: command.task_digest(),
    build_configuration: command.build_configuration.clone(),
    result_output: key(PUBLIC_OUTPUT),
    result_output_kind: EvidenceOutputKind::Report,
    result_outcome: key("authored"),
    evidence: vec![profile.inventory().clone()],
  });
  let mut draft = workflow::draft(
    acceptance,
    vec![gate, author],
    vec![
      workflow::edge(acceptance, "accepted", "node", selected),
      workflow::edge(selected, "authored", "terminal", "finished"),
    ],
    "finished",
    public_schema,
    budget,
    &command.project_permissions,
  );
  draft["root"]["input_schema"] = json!(accepted_schema.reference());
  draft["data_schemas"]
    .as_array_mut()
    .unwrap()
    .push(json!(accepted_schema));
  draft
}

pub(super) async fn run(s: AuthorScenario<'_>) {
  let public_schema = handoff_schema();
  let profile = installed_profile(&s, &public_schema);
  let command = profile.command();
  let validator = s
    .validate
    .then(|| workflow_validation::Validator::new(&profile, s.coder.unwrap()));
  let verifier = s.isolated.filter(|_| !s.review || s.prelude).map(|isolated| {
    workflow_verification::Verifier::new(
      s.directory,
      s.release,
      s.resources,
      isolated,
      s.backend,
      s.coder.unwrap(),
    )
  });
  let reviewer = s
    .review
    .then(|| workflow_review::Reviewer::new(&s, &profile, validator.as_ref().unwrap()));
  let prelude = s
    .prelude
    .then(|| workflow_prelude::Prelude::new(s.directory, s.release, s.resources, s.backend));
  let delivery = if s.deliver {
    Some(workflow_delivery::InstalledDelivery::new(s.directory, s.resources, s.repository_locator).await)
  } else {
    None
  };
  let deployment = if let Some(configuration) = s.deployment_configuration {
    Some(
      workflow_deployment::InstalledDeployment::new(
        s.directory,
        command,
        s.coder.unwrap().profile.capture.as_ref().unwrap().tool(),
        s.backend,
        configuration,
      )
      .await,
    )
  } else {
    None
  };
  let shadow = if s.prelude && deployment.is_some() {
    Some(workflow_shadow::Shadow::new(s.directory, s.release, s.resources, s.backend).await)
  } else {
    None
  };
  let policy: JobSpecToolchainPolicy =
    serde_json::from_slice(&fs::read(s.directory.join("job-spec-policy.json")).unwrap()).unwrap();
  command.validate_installation(&policy).unwrap();
  assert_eq!(command.project_permissions.executables().count(), 0);
  assert_eq!(command.project_permissions.secret_profiles().count(), 0);
  assert!(
    command
      .project_permissions
      .mounts()
      .all(|mount| mount.root().as_str() != octacity_protocol::FACTORY_SOURCE_ROOT)
  );
  let canonical_policy = FactoryProtectedTestAuthoringPolicy::new(
    profile.policy().clone(),
    profile.inventory().clone(),
    profile.files().to_vec(),
  )
  .unwrap();
  let mut catalog = json!({"version":1,"projects":[{"project_id":s.resources.project,
    "references":[
      {"kind":"admission_policy","alias":"pilot-admission","reference":reference("pilot.admission")},
      {"kind":"permission_ceiling","alias":"pilot-ceiling","reference":reference("pilot.ceiling")},
      {"kind":"node_capability","alias":"author","reference":command.executor},
      {"kind":"node_capability","alias":"author-policy","reference":command.identity}
    ],"build_configurations":[{"alias":"author-build","reference":command.build_configuration}]}],
    "codex":[],"protected_test_authors":[{"command":command_entry(command),"policy":canonical_policy}]});
  let mut ceiling = command.project_permissions.clone();
  if let Some(coder) = s.coder {
    catalog["codex"] = json!([coder.coder_entry()]);
    let mut grants = serde_json::to_value(&coder.profile.project_permissions).unwrap();
    grants["outputs"]["kinds"]
      .as_array_mut()
      .unwrap()
      .extend(command.project_permissions.outputs().kinds().map(|kind| json!(kind)));
    ceiling = serde_json::from_value(grants).unwrap();
    let references = catalog["projects"][0]["references"].as_array_mut().unwrap();
    references.extend([
      json!({"kind":"secret_profile","alias":coder.profile.credentials.profile(FactoryCredentialPurpose::Model),"reference":reference("installed.coder-credential")}),
      json!({"kind":"node_capability","alias":"coder-executable","reference":coder.profile.toolchain.executable}),
      json!({"kind":"node_capability","alias":"coder-model","reference":coder.profile.toolchain.model}),
      json!({"kind":"node_capability","alias":"capture-tool","reference":coder.profile.capture.as_ref().unwrap().tool()}),
    ]);
  }
  if let Some(validator) = &validator {
    validator.profile.validate_installation(&policy).unwrap();
    catalog["protected_validation"] = json!([validator.entry(&profile)]);
    let references = catalog["projects"][0]["references"].as_array_mut().unwrap();
    references.extend([
      json!({"kind":"node_capability","alias":"validator-executor","reference":validator.profile.executor}),
      json!({"kind":"node_capability","alias":"validator-policy","reference":validator.profile.identity}),
    ]);
    let mut grants = serde_json::to_value(&ceiling).unwrap();
    grants["outputs"]["kinds"].as_array_mut().unwrap().extend(
      validator
        .profile
        .project_permissions
        .outputs()
        .kinds()
        .map(|kind| json!(kind)),
    );
    ceiling = serde_json::from_value(grants).unwrap();
  }
  if let Some(deployment) = &deployment {
    deployment.profile.validate_installation(&policy).unwrap();
    deployment.install(&mut catalog, &mut ceiling);
  }
  if let Some(prelude) = &prelude {
    prelude.model.profile.validate_installation(&policy).unwrap();
    prelude.install(&mut catalog, &mut ceiling);
  }
  if let Some(shadow) = &shadow {
    shadow.model.profile.validate_installation(&policy).unwrap();
    shadow.install(&mut catalog);
    include_permissions(&mut ceiling, &shadow.model.profile.project_permissions);
  }
  let agent_permissions = ceiling.clone();
  if let Some(shadow) = &shadow {
    include_permissions(&mut ceiling, &shadow.tools.permissions);
    assert!(
      !agent_permissions
        .secret_profiles()
        .any(|key| key == &shadow.tools.binding.credential_profile)
    );
  }
  if let Some(verifier) = &verifier {
    let p = &verifier.model.profile;
    p.validate_installation(&policy).unwrap();
    catalog["codex"].as_array_mut().unwrap().push(verifier.entry());
    catalog["projects"][0]["build_configurations"]
      .as_array_mut()
      .unwrap()
      .push(json!({"alias":"isolated-verifier-build","reference":p.build_configuration}));
    catalog["projects"][0]["references"].as_array_mut().unwrap().extend([
      json!({"kind":"node_capability","alias":"isolated-verifier-executable","reference":p.toolchain.executable}),
      json!({"kind":"node_capability","alias":"isolated-verifier-model","reference":p.toolchain.model}),
      json!({"kind":"secret_profile","alias":p.credentials.profile(FactoryCredentialPurpose::Evaluator),"reference":reference("installed.isolated-credential")}),
    ]);
    let mut grants = serde_json::to_value(&ceiling).unwrap();
    let extra = serde_json::to_value(&p.project_permissions).unwrap();
    for category in ["executables", "commands", "secret_profiles"] {
      grants[category]
        .as_array_mut()
        .unwrap()
        .extend(extra[category].as_array().unwrap().iter().cloned());
    }
    ceiling = serde_json::from_value(grants).unwrap();
  }
  if let Some(reviewer) = &reviewer {
    reviewer.install(&mut catalog, &mut ceiling, &profile);
  }
  if let Some(delivery) = &delivery {
    delivery.install(&mut catalog, &mut ceiling);
  }
  let references = catalog["projects"][0]["references"].as_array_mut().unwrap();
  for (index, plugin) in command.project_permissions.plugins().enumerate() {
    references.push(json!({"kind":"node_capability","alias":format!("installed-plugin-{index}"),"reference":plugin}));
  }
  let profiles = private_file(s.directory, "profiles.json", &catalog.to_string());
  let connectors = shadow.as_ref().map_or_else(
    || private_file(s.directory, "connectors.json", r#"{"version":1,"connectors":[]}"#),
    |shadow| shadow.connectors(s.directory),
  );
  let objects = required_string("OCTACITY_MINIO_ENDPOINT");
  let config = write_factory_server_config(
    s.directory,
    &ServerConfigInput {
      postgres_url: s.postgres,
      object_endpoint: &objects,
      cache_endpoint: "https://cache.example",
      toolchain: &s.release.toolchain,
      management_addr: s.management_addr,
      agent_addr: s.agent_addr,
      cache_addr: None,
    },
    &FactoryServerFiles {
      profiles: &profiles,
      connectors: &connectors,
    },
  );
  private_file(
    s.directory,
    "object-access-key",
    &required_string("OCTACITY_MINIO_ACCESS_KEY"),
  );
  private_file(
    s.directory,
    "object-secret-key",
    &required_string("OCTACITY_MINIO_SECRET_KEY"),
  );
  let client = Client::new();
  let origin = format!("http://{}", s.management_addr);
  let server_log = s.directory.join("author-server.stdout.log");
  let server_error = s.directory.join("author-server.stderr.log");
  let mut server = spawn_server(&s.release.server_binary, &config, &server_log, &server_error);
  wait_for_server_ready(&client, &origin, &mut server, &server_log).await;
  let installation = s.directory.join("protected-author");
  fs::create_dir(&installation).unwrap();
  set_mode(&installation, 0o700);
  let origins = [objects.as_str()];
  let mut credentials = BTreeMap::new();
  if let Some(coder) = s.coder {
    let credential = private_file(
      &installation,
      "model.json",
      &json!({"OCTACITY_CODEX_AUTH":"finite-coder-private-canary"}).to_string(),
    );
    credentials.insert(
      coder
        .profile
        .credentials
        .profile(FactoryCredentialPurpose::Model)
        .to_string(),
      credential,
    );
  }
  if let Some(prelude) = &prelude {
    assert_eq!(
      prelude.model.profile.toolchain.executable,
      s.coder.unwrap().profile.toolchain.executable,
      "the pinned finite CLI serves both separately installed profiles"
    );
    credentials.insert(
      prelude
        .model
        .profile
        .credentials
        .profile(FactoryCredentialPurpose::Evaluator)
        .to_string(),
      private_file(
        &installation,
        "prelude.json",
        &json!({"OCTACITY_CODEX_AUTH":"readonly-prelude-private-canary"}).to_string(),
      ),
    );
  }
  if let Some(shadow) = &shadow {
    assert_eq!(
      shadow.model.profile.toolchain.executable,
      s.coder.unwrap().profile.toolchain.executable
    );
    credentials.insert(
      shadow
        .model
        .profile
        .credentials
        .profile(FactoryCredentialPurpose::Model)
        .to_string(),
      private_file(
        &installation,
        "shadow-model.json",
        &json!({"OCTACITY_CODEX_AUTH":"shadow-model-private-canary"}).to_string(),
      ),
    );
  }
  let capture_platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
  let capture_digest = s
    .coder
    .map(|coder| coder.profile.capture.as_ref().unwrap().tool().digest().to_string());
  let capture = s.coder.map(|coder| agent::AgentToolExecutable {
    product: "git",
    path: coder.capture_executable.as_ref().unwrap(),
    version: coder.profile.capture.as_ref().unwrap().tool().version().as_str(),
    platform: &capture_platform,
    sha256: capture_digest.as_deref().unwrap(),
  });
  let model_digest = s
    .coder
    .map(|coder| coder.profile.toolchain.executable.digest().to_string());
  let executables = s
    .coder
    .into_iter()
    .map(|coder| agent::AgentToolExecutable {
      product: "codex-cli",
      path: &coder.executable,
      version: "0.161.0",
      platform: &coder.platform,
      sha256: model_digest.as_deref().unwrap(),
    })
    .collect::<Vec<_>>();
  let grants = octacity_server_store::factory_job_permissions(&agent_permissions).unwrap();
  let mut overrides = agent::AgentConfigOverrides::restricted_without_outputs(&origins);
  overrides.factory = Some(agent::AgentFactoryOverrides {
    permissions: &grants,
    credential_profiles: &credentials,
    capture: capture.as_ref(),
    protected_reader: shadow.as_ref().map(|shadow| &shadow.tools.reader),
  });
  overrides.tool_executables = &executables;
  overrides.output_limits = Some(octacity_protocol::OutputLimits {
    // The ordinary Build Configuration has these publication ceilings. The
    // independent signed Factory permissions above remain restricted to 2/1
    // outputs and 64/4 KiB; the Agent checks both ceilings independently.
    artifact_count: 16,
    artifact_bytes: 4 * 1024 * 1024,
    report_count: 16,
    report_bytes: 4 * 1024 * 1024,
    single_output_bytes: 4 * 1024 * 1024,
  });
  let identity = Uuid::new_v4().simple().to_string();
  let enrollment = resources::issue_enrollment(&client, &origin, &identity, &s.resources.pool, s.backend).await;
  let source_plugins = s.directory.join("source-mirror/source-plugins");
  let agent_config = agent::write_agent_config(
    &installation,
    &format!("http://{}", s.agent_addr),
    &format!("author-{identity}"),
    &enrollment,
    agent::AgentReleasePaths {
      octa_root: &s.release.octa_root,
      source_plugins: &source_plugins,
    },
    s.backend,
    &overrides,
  );
  if let Some(deployment) = &deployment {
    let mut local: toml::Value = toml::from_str(&fs::read_to_string(&agent_config).unwrap()).unwrap();
    local["allowed_network_hosts"] = toml::Value::try_from(
      deployment
        .permissions()
        .network_hosts()
        .map(|host| host.as_str().to_owned())
        .collect::<Vec<_>>(),
    )
    .unwrap();
    fs::write(&agent_config, toml::to_string(&local).unwrap()).unwrap();
  }
  let validation = tokio::process::Command::new(&s.release.agent_binary)
    .arg("validate")
    .arg(&agent_config)
    .output()
    .await
    .unwrap();
  fs::write(installation.join("validate.stdout.log"), validation.stdout).unwrap();
  fs::write(installation.join("validate.stderr.log"), validation.stderr).unwrap();
  assert!(
    validation.status.success(),
    "author Agent installation failed validation; private evidence: {}",
    s.directory.display()
  );
  let mut worker = agent::spawn_agent(
    &s.release.agent_binary,
    &agent_config,
    &installation.join("stdout.log"),
    &installation.join("stderr.log"),
  );
  let mut verifier_worker = if let Some(verifier) = &verifier {
    assert!(
      !credentials.contains_key(
        verifier
          .model
          .profile
          .credentials
          .profile(FactoryCredentialPurpose::Evaluator)
          .as_str()
      )
    );
    Some(verifier.start(&s, s.isolated.unwrap(), &client, &origin).await)
  } else if let Some(reviewer) = &reviewer {
    Some(reviewer.verifier.start(&s, s.isolated.unwrap(), &client, &origin).await)
  } else {
    None
  };
  let budget = BudgetLimit::new(
    if s.prelude {
      32
    } else if s.review || deployment.is_some() {
      12
    } else {
      8
    },
    if s.prelude {
      3_600_000
    } else if deployment.is_some() {
      2_400_000
    } else if s.review || s.deliver {
      1_200_000
    } else {
      600_000
    },
    if shadow.is_some() {
      12000
    } else if s.review {
      2000
    } else {
      1000
    },
    if shadow.is_some() {
      12000
    } else if s.review {
      2000
    } else {
      1000
    },
    if s.prelude {
      96
    } else if deployment.is_some() {
      40
    } else if s.isolated.is_some() {
      32
    } else if s.validate {
      24
    } else {
      16
    } * 1024
      * 1024,
  )
  .unwrap();
  let ordinary = get_json(
    &client,
    format!(
      "{origin}/api/v1/build-configurations/{}/versions/1",
      command.build_configuration.id()
    ),
  )
  .await;
  let outputs = &ordinary["definition"]["artifacts"];
  assert!(
    outputs["artifact_bytes"].as_u64().unwrap() + outputs["report_bytes"].as_u64().unwrap()
      <= budget.max_output_bytes(),
    "fixture Node budget must reserve the exact ordinary Job's signed output ceilings"
  );
  let acceptance = format!("confirm-{identity}");
  let selected = format!("author-{identity}");
  let shadow_nodes = [format!("routing-shadow-{identity}"), format!("tool-shadow-{identity}")];
  let node_budget = if s.coder.is_some() {
    let count = if s.isolated.is_some() {
      4
    } else if s.validate {
      3
    } else {
      2
    };
    let bounded = BudgetLimit::new(
      1,
      if s.review {
        120_000
      } else if s.deliver {
        180_000
      } else if s.isolated.is_some() {
        150_000
      } else if s.validate {
        180_000
      } else {
        120_000
      },
      100,
      100,
      8 * 1024 * 1024,
    )
    .unwrap();
    assert!(count * bounded.max_attempts() <= budget.max_attempts());
    assert!(u64::from(count) * bounded.max_elapsed_millis() <= budget.max_elapsed_millis());
    assert!(u64::from(count) * bounded.max_tokens() <= budget.max_tokens());
    assert!(u64::from(count) * bounded.max_cost_micro_units() <= budget.max_cost_micro_units());
    assert!(u64::from(count) * bounded.max_output_bytes() <= budget.max_output_bytes());
    bounded
  } else {
    budget
  };
  let mut draft = flow_definition(&profile, &public_schema, node_budget, &acceptance, &selected);
  draft["root"]["execution"]["budget"] = json!(budget);
  draft["limits"]["budget"] = json!(budget);
  let coding = format!("coding-{identity}");
  if let Some(coder) = s.coder {
    extend_coder_flow(
      &mut draft,
      coder,
      &public_schema,
      if s.review && !s.deliver {
        BudgetLimit::new(1, 600_000, 100, 100, 8 * 1024 * 1024).unwrap()
      } else {
        node_budget
      },
      &selected,
      &coding,
      &ceiling,
    );
  }
  let validating = format!("validating-{identity}");
  if let Some(validator) = &validator {
    let validation_budget = if s.review {
      BudgetLimit::new(1, 300_000, 100, 100, 8 * 1024 * 1024).unwrap()
    } else {
      node_budget
    };
    validator.extend(&mut draft, &selected, &coding, &validating, validation_budget);
  }
  let verifying = format!("verifying-{identity}");
  let nested_verifier = format!("nested-verifier-{identity}");
  if let Some(verifier) = &verifier {
    verifier
      .extend(
        &client,
        &origin,
        &s.resources.project,
        &mut draft,
        workflow_verification::VerificationNodes {
          coder: &coding,
          previous: &validating,
          call: &nested_verifier,
          leaf: &verifying,
        },
        node_budget,
      )
      .await;
  }
  let review_nodes = [
    format!("evidence-{identity}"),
    format!("plan-{identity}"),
    format!("assessment-{identity}"),
    format!("decision-{identity}"),
  ];
  if let Some(reviewer) = &reviewer {
    reviewer.extend(
      &mut draft,
      workflow_review::ReviewStages {
        coder: &coding,
        validator: &validating,
        previous: if verifier.is_some() {
          &nested_verifier
        } else {
          &validating
        },
        previous_outcome: if verifier.is_some() { "verified" } else { "validated" },
        evidence: &review_nodes[0],
        planner: &review_nodes[1],
        reviewer: &review_nodes[2],
        decision: &review_nodes[3],
      },
    );
  }
  let delivering = format!("delivering-{identity}");
  if let Some(delivery) = &delivery {
    delivery.extend(
      &mut draft,
      &coding,
      &nested_verifier,
      if reviewer.is_some() {
        (&review_nodes[3], "accept")
      } else {
        (&nested_verifier, "verified")
      },
      &delivering,
      node_budget,
    );
  }
  if let Some(deployment) = &deployment {
    deployment.extend(&mut draft, &delivering, "published", &coding, node_budget);
  }
  let prelude_publication = if let Some(prelude) = &prelude {
    let required = prelude.budget();
    assert!(required.max_elapsed_millis() < budget.max_elapsed_millis());
    draft["limits"]["max_expanded_nodes"] = json!(64);
    Some(if shadow.is_some() {
      prelude
        .extend_variant(
          &client,
          &origin,
          &mut draft,
          &acceptance,
          Some(&selected),
          workflow_prelude::PreludeVariant::new(vec![
            workflow_prelude::PreludePhase::Research,
            workflow_prelude::PreludePhase::Requirements,
          ]),
        )
        .await
    } else {
      prelude
        .extend(&client, &origin, &mut draft, &acceptance, &selected)
        .await
    })
  } else {
    None
  };
  if let Some(shadow) = &shadow {
    shadow.extend(
      &mut draft,
      workflow_shadow::ShadowNodes {
        prelude: &prelude_publication.as_ref().unwrap().call,
        acceptance: &acceptance,
        author: &selected,
        coder: &coding,
        routing: &shadow_nodes[0],
        tool: &shadow_nodes[1],
      },
    );
  }
  if let Some(coder) = s.coder.filter(|_| s.deliver) {
    let continuation = candidate_continuation_millis(&draft, &coding);
    let producer_window = coder.profile.project_permissions.resources().elapsed_millis() + continuation;
    assert!(producer_window + continuation <= budget.max_elapsed_millis());
    let node = draft["root"]["nodes"]
      .as_array_mut()
      .unwrap()
      .iter_mut()
      .find(|node| node["key"] == coding)
      .unwrap();
    node["budget"]["max_elapsed_millis"] = json!(producer_window);
  }
  let _: octacity_server_application::FactoryFlowDefinitionDraft =
    serde_json::from_value(draft.clone()).expect("typed author parent draft");
  let flow = post_management(
    &client,
    &origin,
    "/api/v1/flow-definitions",
    &format!("author-flow-{identity}"),
    json!({"id":Uuid::new_v4(),"project_id":s.resources.project,"definition":draft}),
  )
  .await;
  let configuration = post_management(&client,&origin,"/api/v1/factory-configurations",&format!("author-config-{identity}"),json!({"id":Uuid::new_v4(),"project_id":s.resources.project,"definition":{
    "admission_policy":"pilot-admission","permission_ceiling":"pilot-ceiling","hard_budget":budget,"wip_limits":{"max_active_runs":4,"max_active_nodes":4},"enabled":true,"flow":flow["resource"]["flow"]
  }})).await;
  let artifact = |name: &str| {
    let value = s.artifacts["items"]
      .as_array()
      .unwrap()
      .iter()
      .find(|value| value["name"] == name)
      .unwrap();
    json!({"artifact_id":value["id"],"content_digest":value["sha256"],"encoded_size":value["size_bytes"]})
  };
  let admitted = post_management(&client,&origin,&format!("/api/v1/projects/{}/factory-work",s.resources.project),&format!("author-work-{identity}"),json!({
    "configuration_id":configuration["resource"]["configuration"]["reference"]["id"],"configuration_version":1,
    "repository_id":s.resources.repository,"repository_version":1,"external_identity":format!("protected-author/{identity}"),
    "source":{"kind":"exact_revision","value":s.revision},"artifacts":{"task":artifact("task"),"acceptance":artifact("acceptance"),"specifications":[]},
    "classification":{"priority":40,"risk":"low","metadata":{}}
  })).await;
  let run = string(&admitted, "factory_run_id");
  let mut delivery_restarted = false;
  let completed = timeout(
    Duration::from_secs(if s.review || deployment.is_some() {
      budget.max_elapsed_millis().div_ceil(1000)
    } else if s.deliver {
      240
    } else {
      180
    }),
    async {
      loop {
        if let Some(delivery) = &delivery
          && !delivery_restarted
          && delivery.accepted()
        {
          // The forge has synced the accepted effect but holds every reply.
          // Inject an actual process crash; ordinary graceful shutdown is
          // qualified separately and must not release this acceptance barrier.
          server.start_kill().expect("stop only the owned Server process");
          let status = timeout(Duration::from_secs(10), server.wait())
            .await
            .expect("owned Server crash cleanup must be bounded")
            .unwrap();
          assert!(!status.success(), "this boundary injects a lost Server process");
          delivery.release_response();
          server = spawn_server(&s.release.server_binary, &config, &server_log, &server_error);
          wait_for_server_ready(&client, &origin, &mut server, &server_log).await;
          delivery_restarted = true;
        }
        let state = get_json(&client, format!("{origin}/api/v1/factory-runs/{run}")).await;
        if state["state"] == "completed" {
          break state;
        }
        assert!(
          !["failed", "cancelled"].contains(&state["state"].as_str().unwrap_or("")),
          "author Flow stopped; private evidence: {}",
          s.directory.display()
        );
        assert!(
          worker.try_wait().unwrap().is_none(),
          "author Agent exited; private evidence: {}",
          s.directory.display()
        );
        if let Some(verifier) = verifier_worker.as_mut() {
          assert!(
            verifier.worker.try_wait().unwrap().is_none(),
            "isolated verifier Agent exited; private evidence: {}",
            s.directory.display()
          );
        }
        let links = workflow::diagnostic(&client, &origin, &run, "builds").await;
        for link in links["items"].as_array().unwrap() {
          let build = get_json(
            &client,
            format!("{origin}/api/v1/builds/{}", string(&link["record"], "build_id")),
          )
          .await;
          assert!(
            !["failed", "cancelled"].contains(&build["state"].as_str().unwrap_or("")),
            "author ordinary Build failed; private evidence: {}",
            s.directory.display()
          );
        }
        sleep(Duration::from_millis(100)).await;
      }
    },
  )
  .await
  .expect("the real author Job did not complete its configured Flow");
  let completions = workflow::diagnostic(&client, &origin, &run, "node-completions").await;
  assert_eq!(
    completions["items"].as_array().unwrap().len(),
    (if s.prelude {
      if s.deliver { 20 } else { 17 }
    } else if deployment.is_some() {
      9
    } else if s.review {
      8
    } else if s.deliver {
      7
    } else if s.isolated.is_some() {
      6
    } else if s.validate {
      4
    } else if s.coder.is_some() {
      3
    } else {
      2
    }) + shadow.as_ref().map_or(0, |_| 3)
  );
  let builds = workflow::diagnostic(&client, &origin, &run, "builds").await;
  assert_eq!(
    builds["items"].as_array().unwrap().len(),
    (if s.prelude {
      if s.deliver { 9 } else { 8 }
    } else if deployment.is_some() {
      5
    } else if s.isolated.is_some() {
      4
    } else if s.validate {
      3
    } else if s.coder.is_some() {
      2
    } else {
      1
    }) + 2 * usize::from(shadow.is_some()),
    "each installed role executes through its actual ordinary Build"
  );
  let pool = sqlx::PgPool::connect(s.postgres).await.unwrap();
  let rows: Vec<(Vec<u8>, Vec<u8>)> =
    sqlx::query_as("SELECT id,document FROM factory_flow_records WHERE run_id=$1 ORDER BY node_attempt_id")
      .bind(run.parse::<Uuid>().unwrap())
      .fetch_all(&pool)
      .await
      .unwrap();
  let (digest, record) = rows
    .iter()
    .map(|(digest, bytes)| {
      let record: Value = serde_json::from_slice(bytes).unwrap();
      (digest, record)
    })
    .find(|(_, record)| record["input"]["node"] == selected)
    .unwrap();
  let build = string(&record["build"]["outputs"][0]["producer"], "build_id");
  let outputs = get_json(&client, format!("{origin}/api/v1/builds/{build}/artifacts?limit=100")).await;
  assert_eq!(
    outputs["items"].as_array().unwrap().len(),
    3,
    "only real private file and server-derived inventory/handoff"
  );
  let completion = completions["items"]
    .as_array()
    .unwrap()
    .iter()
    .find(|row| row["record"]["node_key"] == selected)
    .unwrap();
  assert_eq!(completion["record"]["output_digest"], hex::encode(digest));
  assert_eq!(record["payload"]["value"]["classes"], json!(["fixture.contract"]));
  assert_eq!(record["payload"]["value"].as_object().unwrap().len(), 2);
  let inventory = record["build"]["outputs"]
    .as_array()
    .unwrap()
    .iter()
    .find(|output| output["name"] == INVENTORY_OUTPUT)
    .unwrap();
  assert_eq!(inventory["output_kind"], "artifact");
  assert!(inventory["data"].is_null());
  let public = serde_json::to_string(&record).unwrap();
  assert!(!public.contains("PRIVATE_EXPECTATION") && !public.contains("contract.rs"));
  let private = outputs["items"]
    .as_array()
    .unwrap()
    .iter()
    .find(|value| value["name"] == PRIVATE_OUTPUT)
    .unwrap();
  let download = client
    .post(format!("{origin}/api/v1/artifacts/{}/download", string(private, "id")))
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap()
    .json::<Value>()
    .await
    .unwrap();
  let response = client
    .get(string(&download, "get_url"))
    .send()
    .await
    .unwrap_or_else(|_| panic!("private Artifact download request failed"));
  assert!(
    response.status().is_success(),
    "private Artifact download failed with {}",
    response.status()
  );
  let body = response
    .bytes()
    .await
    .unwrap_or_else(|_| panic!("private Artifact download body was unavailable"));
  assert_eq!(
    body.as_ref(),
    if s.coder.is_some() {
      EXECUTABLE_PRIVATE_BODY
    } else {
      PRIVATE_BODY
    },
    "the retained private output came from the actual released command"
  );
  if let Some(prelude) = &prelude {
    let accepted = rows
      .iter()
      .map(|(_, bytes)| serde_json::from_slice::<Value>(bytes).unwrap())
      .find(|record| record["input"]["node"] == acceptance)
      .unwrap();
    assert_eq!(
      accepted["payload"]["schema"],
      json!(prelude.accepted_schema().reference())
    );
    if let Some(publication) = &prelude_publication {
      prelude
        .verify_variant(
          &client,
          &origin,
          &run,
          &accepted["payload"]["value"],
          &json!({"task":artifact("task"),"acceptance":artifact("acceptance")}),
          &publication.variant,
        )
        .await;
    }
  }
  if s.coder.is_some() {
    verify_coder(&client, &origin, &run, &coding, s.revision, &rows).await;
  }
  if let Some(validator) = &validator {
    validator
      .verify(&client, &origin, &rows, &selected, &coding, &validating)
      .await;
  }
  if let Some(verifier) = &verifier {
    verifier
      .verify(
        &client,
        &origin,
        s.isolated.unwrap(),
        &s.resources.pool,
        &rows,
        workflow_verification::VerificationNodes {
          coder: &coding,
          previous: &validating,
          call: &nested_verifier,
          leaf: &verifying,
        },
      )
      .await;
  }
  if let Some(reviewer) = &reviewer {
    reviewer
      .verify(
        &client,
        &origin,
        &rows,
        review_nodes.each_ref().map(String::as_str),
        &coding,
        s.isolated.unwrap(),
        &s.resources.pool,
      )
      .await;
  }
  if let Some(delivery) = &delivery {
    assert!(
      delivery_restarted,
      "unknown accepted review crossed actual Server restart"
    );
    delivery.verify(&rows, &delivering, &coding, s.revision);
  }
  if let Some(deployment) = &deployment {
    deployment.verify(&client, &origin, &rows).await;
  }
  let shadow_receipts = if let Some(shadow) = &shadow {
    let record = rows
      .iter()
      .map(|(_, bytes)| serde_json::from_slice::<Value>(bytes).unwrap())
      .find(|record| record["input"]["node"] == shadow_nodes[1])
      .unwrap();
    Some(shadow.verify(s.postgres, &run, &record).await)
  } else {
    None
  };
  let audit = if s.prelude {
    Some(workflow_audit::capture(&client, &origin, &run, &flow["resource"]["flow"], &rows).await)
  } else {
    None
  };
  let variant_context = shadow.as_ref().map(|_| workflow_variants::VariantProbeContext {
    prelude: prelude.as_ref().unwrap(),
    client: &client,
    origin: &origin,
    resources: s.resources,
    artifacts: s.artifacts,
    revision: s.revision,
    ceiling: &prelude.as_ref().unwrap().model.profile.project_permissions,
    postgres: s.postgres,
    run_a: &run,
    root_reference_a: &flow["resource"]["flow"]["closure"]["root"],
    published_a: prelude_publication.as_ref().unwrap(),
  });
  let variants = if let Some(context) = &variant_context {
    Some(workflow_variants::run(context).await)
  } else {
    None
  };
  if let Some(verifier) = verifier_worker.as_mut() {
    verifier.worker.kill().await.unwrap();
    verifier.worker.wait().await.unwrap();
  }
  worker.kill().await.unwrap();
  worker.wait().await.unwrap();
  shutdown_server(&mut server, &server_log).await;
  let mut server = spawn_server(&s.release.server_binary, &config, &server_log, &server_error);
  wait_for_server_ready(&client, &origin, &mut server, &server_log).await;
  let mut worker = agent::spawn_agent(
    &s.release.agent_binary,
    &agent_config,
    &installation.join("restart.stdout.log"),
    &installation.join("restart.stderr.log"),
  );
  if let Some(verifier) = verifier_worker.as_mut() {
    verifier.worker = agent::spawn_agent(
      &s.release.agent_binary,
      &verifier.config,
      &s.directory.join("isolated-verifier-agent/restart.stdout.log"),
      &s.directory.join("isolated-verifier-agent/restart.stderr.log"),
    );
  }
  assert_eq!(
    get_json(&client, format!("{origin}/api/v1/factory-runs/{run}")).await,
    completed
  );
  assert_eq!(
    workflow::diagnostic(&client, &origin, &run, "node-completions").await,
    completions
  );
  assert_eq!(workflow::diagnostic(&client, &origin, &run, "builds").await, builds);
  let after: Vec<(Vec<u8>, Vec<u8>)> =
    sqlx::query_as("SELECT id,document FROM factory_flow_records WHERE run_id=$1 ORDER BY node_attempt_id")
      .bind(run.parse::<Uuid>().unwrap())
      .fetch_all(&pool)
      .await
      .unwrap();
  assert_eq!(
    after, rows,
    "private inventory metadata and public frozen receipts survive restart exactly"
  );
  if let Some(audit) = &audit {
    audit.assert_retained_by(&workflow_audit::capture(&client, &origin, &run, &flow["resource"]["flow"], &after).await);
  }
  if let Some(context) = &variant_context {
    workflow_variants::verify_after_restart(context, variants.as_ref().unwrap()).await;
  }
  if let Some(shadow) = &shadow {
    let record = after
      .iter()
      .map(|(_, bytes)| serde_json::from_slice::<Value>(bytes).unwrap())
      .find(|record| record["input"]["node"] == shadow_nodes[1])
      .unwrap();
    assert_eq!(Some(shadow.verify(s.postgres, &run, &record).await), shadow_receipts);
  }
  assert_eq!(
    get_json(&client, format!("{origin}/api/v1/builds/{build}/artifacts?limit=100")).await,
    outputs
  );
  pool.close().await;
  if let Some(delivery) = &delivery {
    delivery.verify(&after, &delivering, &coding, s.revision);
  }
  if let Some(deployment) = &deployment {
    deployment.verify(&client, &origin, &after).await;
  }
  if let Some(reviewer) = &reviewer {
    reviewer
      .verify(
        &client,
        &origin,
        &rows,
        review_nodes.each_ref().map(String::as_str),
        &coding,
        s.isolated.unwrap(),
        &s.resources.pool,
      )
      .await;
  }
  if let Some(verifier) = verifier_worker.as_mut() {
    verifier.worker.kill().await.unwrap();
    verifier.worker.wait().await.unwrap();
  }
  worker.kill().await.unwrap();
  worker.wait().await.unwrap();
  shutdown_server(&mut server, &server_log).await;
  if let Some(delivery) = delivery {
    delivery.stop().await;
  }
  if let Some(deployment) = deployment {
    deployment.stop().await;
  }
  if let Some(shadow) = shadow {
    shadow.tools.fixture.shutdown().await;
  }
}

fn extend_coder_flow(
  draft: &mut Value,
  model: &models::InstalledModel,
  public_schema: &FlowDataSchema,
  budget: BudgetLimit,
  author: &str,
  coder: &str,
  ceiling: &FactoryPermissionSet,
) {
  use octacity_protocol::{CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_OUTPUT};
  let summary = FlowDataSchema::new(
    key("finite-coder-summary"),
    key("v1"),
    FlowValueSchema::String {
      min_bytes: 1,
      max_bytes: 64,
    },
  )
  .unwrap();
  let facts = models::candidate_schema();
  let profile = &model.profile;
  let capture = profile.capture.as_ref().unwrap();
  let mut node = workflow::node(
    coder,
    "reasoning",
    &summary,
    budget,
    &profile.project_permissions,
    "implemented",
  );
  node["input_schema"] = json!(public_schema.reference());
  node["input_binding"] = json!(
    FlowInputBinding::new(
      BTreeMap::from([(
        key("handoff"),
        FlowInputSource::Record {
          node: key(author),
          outcome: key("authored"),
          view: FlowRecordView::Payload,
          path: String::new()
        }
      )]),
      FlowDataMapping::new(FlowValueMapping::Pointer {
        path: "/handoff".into()
      })
      .unwrap()
    )
    .unwrap()
  );
  let mut evidence = [CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_OUTPUT]
    .into_iter()
    .map(|name| {
      EvidenceRequirement::new(
        key(name),
        EvidenceOutputKind::Artifact,
        reference("immutable-capture-artifact"),
        capture.tool().clone(),
        profile.toolchain.plugin.clone(),
      )
    })
    .collect::<Vec<_>>();
  evidence.push(EvidenceRequirement::new(
    key(models::CANDIDATE_FACTS),
    EvidenceOutputKind::Report,
    facts.reference().clone(),
    capture.tool().clone(),
    profile.toolchain.plugin.clone(),
  ));
  node["build"] = json!(FlowBuildBinding {
    tool: profile.toolchain.executable.clone(),
    plugin: profile.toolchain.plugin.clone(),
    result_tool: None,
    capture_write_paths: Some(vec![FactoryRepositoryPath::new("public").unwrap()]),
    capture_deliverables: None,
    model_or_tool: profile.toolchain.model.clone(),
    task_digest: profile.task_digest(),
    build_configuration: profile.build_configuration.clone(),
    result_output: key("codex-run-result"),
    result_output_kind: EvidenceOutputKind::Report,
    result_outcome: key("implemented"),
    evidence
  });
  draft["root"]["nodes"].as_array_mut().unwrap().push(node);
  draft["root"]["transitions"][1] = workflow::edge(author, "authored", "node", coder);
  draft["root"]["transitions"]
    .as_array_mut()
    .unwrap()
    .push(workflow::edge(coder, "implemented", "terminal", "finished"));
  draft["root"]["terminals"][0]["schema"] = json!(summary.reference());
  draft["root"]["execution"]["permissions"] = json!(ceiling);
  draft["limits"]["permissions"] = json!(ceiling);
  draft["data_schemas"]
    .as_array_mut()
    .unwrap()
    .extend([json!(summary), json!(facts)]);
}

pub(super) async fn download_artifact(client: &Client, origin: &str, id: &str) -> Vec<u8> {
  let download = client
    .post(format!("{origin}/api/v1/artifacts/{id}/download"))
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap()
    .json::<Value>()
    .await
    .unwrap();
  let response = client
    .get(string(&download, "get_url"))
    .send()
    .await
    .unwrap_or_else(|_| panic!("artifact download request failed"));
  assert!(
    response.status().is_success(),
    "artifact download status {}",
    response.status()
  );
  response
    .bytes()
    .await
    .unwrap_or_else(|_| panic!("artifact download body unavailable"))
    .to_vec()
}
async fn verify_coder(client: &Client, origin: &str, run: &str, coder: &str, base: &str, rows: &[(Vec<u8>, Vec<u8>)]) {
  use octacity_protocol::{CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_OUTPUT, CapturedChangeSetManifestV1};
  let record = rows
    .iter()
    .map(|(_, bytes)| serde_json::from_slice::<Value>(bytes).unwrap())
    .find(|record| record["input"]["node"] == coder)
    .unwrap();
  assert_eq!(record["payload"]["value"], "implemented");
  let outputs = record["build"]["outputs"].as_array().unwrap();
  let manifest = outputs
    .iter()
    .find(|output| output["name"] == CHANGE_SET_MANIFEST_OUTPUT)
    .unwrap();
  let bundle = outputs
    .iter()
    .find(|output| output["name"] == CHANGE_SET_BUNDLE_OUTPUT)
    .unwrap();
  assert!(
    manifest["data"].is_null() && bundle["data"].is_null(),
    "binary capture artifacts never become Flow payloads"
  );
  let bytes = download_artifact(client, origin, manifest["artifact"]["artifact_id"].as_str().unwrap()).await;
  let digest = FactoryDigest::content_sha256(&bytes).to_string();
  let capture: CapturedChangeSetManifestV1 = serde_json::from_slice(&bytes).unwrap();
  capture.validate().unwrap();
  assert_eq!(capture.base_revision, base);
  assert_ne!(capture.candidate_revision, base);
  assert_eq!(capture.changed_paths.len(), 1);
  assert_eq!(capture.changed_paths[0].path, "public/facts.txt");
  assert_eq!(
    capture.changed_paths[0].content_sha256.as_deref(),
    Some(FactoryDigest::content_sha256(b"facts").to_string().as_str())
  );
  let facts = outputs
    .iter()
    .find(|output| output["name"] == models::CANDIDATE_FACTS)
    .unwrap();
  let value = &facts["data"]["payload"]["value"];
  assert_eq!(value.as_object().unwrap().len(), 4);
  assert_eq!(value["candidate_revision"], capture.candidate_revision);
  assert_eq!(value["candidate_digest"], capture.candidate_digest(&digest).unwrap());
  let attempts = workflow::diagnostic(client, origin, run, "node-attempts").await;
  let attempt = attempts["items"]
    .as_array()
    .unwrap()
    .iter()
    .find(|row| row["record"]["node_key"] == coder)
    .unwrap();
  assert_eq!(value["input_digest"], attempt["record"]["input_digest"]);
  assert_eq!(value["fresh_until"], attempt["record"]["deadline"]);
  let encoded = serde_json::to_string(&record).unwrap();
  assert!(
    !encoded.contains("PRIVATE_EXPECTATION")
      && !encoded.contains("private-contract.rs")
      && !encoded.contains("finite-coder-private-canary")
  );
}
