//! Released role journey through ordinary publication, selection and retained inputs.
use super::*;
use octacity_server_factory::*;
use std::collections::BTreeMap;

pub(super) struct RolePilot {
  role: FactoryRole,
  profile: FactoryRoleProfile,
  default: FactoryRoleProfile,
  settings: FactoryRuntimeSettingsDefinition,
  model_discovery: Value,
}
impl RolePilot {
  pub(super) async fn prepare(
    client: &Client,
    origin: &str,
    resources: &resources::PilotResources,
    model: &mut models::InstalledModel,
    postgres: &str,
  ) -> Self {
    let project = resources.project.parse().unwrap();
    // Discovery runs natively; the actual model invocation still uses the released
    // Linux plugin inside Microsandbox. Each executable keeps its own digest.
    let discovery_plugin = required_path("OCTACITY_MODEL_DISCOVERY_PLUGIN", true);
    let runtime = model.executable.parent().unwrap().join("catalog-runtime");
    fs::copy(required_path("OCTACITY_MODEL_DISCOVERY_RUNTIME", true), &runtime).unwrap();
    set_mode(&runtime, 0o700);
    fs::write(
      runtime.with_extension("models"),
      json!({"data":[{
      "model":"finite-json-model", "displayName":"Finite model",
      "supportedReasoningEfforts":[{"reasoningEffort":"low"},{"reasoningEffort":"high"}]
    }],"nextCursor":null})
      .to_string(),
    )
    .unwrap();
    let model_discovery = json!({
      "executable":discovery_plugin,
      "executable_sha256":FactoryDigest::content_sha256(&fs::read(&discovery_plugin).unwrap()),
      "environment":{"OCTA_CODEX_EXECUTABLE":runtime}
    });
    // The selected pro identifier is deliberately absent from the suggestions.
    // Pricing remains exact for this installed finite fixture contract.
    let selected_model = ImmutableReference::new(
      FactoryKey::new("finite-json-model-pro").unwrap(),
      model.profile.toolchain.model.version().clone(),
      model.profile.toolchain.model.digest(),
    );
    model.profile.toolchain.model = selected_model.clone();
    model.profile.usage_policy = octacity_server_application::FactoryCodexUsagePolicy::TokenRates {
      model: selected_model,
      input_micro_units_per_million: 1_000_000,
      cached_input_micro_units_per_million: 1_000_000,
      output_micro_units_per_million: 1_000_000,
    };
    let imported = post_management(client, origin, &format!("/api/v1/projects/{project}/factory-skills"), "pilot-role-skill", json!({
      "identity":"review-contract", "files":[
        {"path":"SKILL.md", "sha256":FactoryDigest::content_sha256(b"# Review contract\nRead supporting.txt.\n"), "bytes":b"# Review contract\nRead supporting.txt.\n".to_vec()},
        {"path":"supporting.txt", "sha256":FactoryDigest::content_sha256(b"Retained supporting facts.\n"), "bytes":b"Retained supporting facts.\n".to_vec()},
        {"path":"server.mjs", "sha256":FactoryDigest::content_sha256(MCP_SERVER), "bytes":MCP_SERVER}
      ]})).await;
    let skill: ImmutableReference = serde_json::from_value(imported["bundle"].clone()).unwrap();
    let integration = managed_reader(postgres, project).await;
    let server = FactoryMcpServer::Stdio {
      command: MCP_EXECUTABLE.into(),
      args: vec!["--mcp".into(), "/octacity/protected/skills/review/server.mjs".into()],
      env_vars: vec!["LANGUAGE".into()],
      enabled_tools: Some(vec!["facts".into()]),
    };
    let role = FactoryRole::publish(
      FactoryRoleId::generate(),
      project,
      serde_json::from_value(json!({
        "name":"Public contract observer", "instructions":["Review the public contract.","Report only evidence."]
      }))
      .unwrap(),
    )
    .unwrap();
    let prefix = "Controlling Project rules (additions cannot waive these rules):\n{\"accepted\":true}\n\nReview the public contract.\n\nReport only evidence.\n\nKeep the result bounded.\n\nSelected retained skill (read only): /octacity/protected/skills/review/SKILL.md\n";
    model.profile.instructions = FactoryText::new(json!({"fixture_version":1,"result":{"$input":"/payload/value"},
      "invocation":{"model":"finite-json-model-pro","prefix":prefix,"options":["--config=model_reasoning_effort=\"high\"","--ephemeral","--ignore-user-config",MCP_OPTION],"environment":{"LANGUAGE":"ru","HOME":"/workspace/scratch"}},
      "mcp":{"command":MCP_EXECUTABLE,"args":["--mcp","/octacity/protected/skills/review/server.mjs"],"env_vars":["LANGUAGE"],"tool":"facts","expected_text":"ru:Retained supporting facts.\n"},
      "checks":[
        {"path":"/octacity/protected/skills/review/SKILL.md","readable":true,"expected_text":"# Review contract\nRead supporting.txt.\n"},
        {"path":"/octacity/protected/skills/review/supporting.txt","readable":true,"expected_text":"Retained supporting facts.\n"},
        {"path":"/workspace/source/AGENTS.md","readable":false},
        {"path":"/workspace/source/.codex/config.toml","readable":false},
        {"path":"/root/.codex/config.toml","readable":false}
      ]}).to_string()).unwrap();
    let mut grants = serde_json::to_value(&model.profile.project_permissions).unwrap();
    let arguments = grants["commands"]
      .as_array_mut()
      .unwrap()
      .iter_mut()
      .find(|command| command["arguments"].as_array().unwrap().len() > 1)
      .unwrap()["arguments"]
      .as_array_mut()
      .unwrap();
    arguments[4] = json!({"kind":"exact","value":"--model=finite-json-model-pro"});
    arguments.insert(
      5,
      json!({"kind":"exact","value":"--config=model_reasoning_effort=\"high\""}),
    );
    let last = arguments.len() - 1;
    for option in ["--ephemeral", "--ignore-user-config", MCP_OPTION].into_iter().rev() {
      arguments.insert(last, json!({"kind":"exact","value":option}));
    }
    grants["commands"].as_array_mut().unwrap().push(json!({
      "executable":model.profile.toolchain.executable,
      "arguments":[{"kind":"exact","value":"--mcp"},{"kind":"exact","value":"/octacity/protected/skills/review/server.mjs"}]
    }));
    model.profile.project_permissions = serde_json::from_value(grants).unwrap();
    model.profile.local_permissions = LocalPermissionCeiling::fully_enforced(model.profile.project_permissions.clone());
    let settings = FactoryRuntimeSettingsDefinition {
      toolchain: model.profile.toolchain.clone(),
      mcp_servers: [(
        FactoryKey::new("reader").unwrap(),
        FactoryRuntimeMcpEntry {
          server,
          integration: Some(FactoryKey::new("reader").unwrap()),
        },
      )]
      .into(),
      integrations: [(FactoryKey::new("reader").unwrap(), integration.clone())].into(),
      permissions: model.profile.project_permissions.clone(),
    };
    let retained = octacity_server_store::FactoryRuntimeSettingsIntent {
      project_id: project,
      runtime: FactoryKey::new("pilot-selected").unwrap(),
      expected_version: None,
      definition: settings.clone(),
    }
    .settings()
    .unwrap();
    let mcp = retained
      .mcp_configuration(&FactoryKey::new("reader").unwrap())
      .unwrap()
      .unwrap();
    assert!(
      model.profile.mcp_configurations.is_empty(),
      "configured transports must not be copied into installation files"
    );
    let profile = FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &role, serde_json::from_value(json!({
      "name":"pro","model_identifier":"finite-json-model-pro","instructions":["Keep the result bounded."],"toolchain":model.profile.toolchain,
      "model_parameters":{"reasoning_effort":"high"},"thinking_parameter":"reasoning_effort",
      "execution":{"environment":{"LANGUAGE":{"kind":"public","value":"ru"}},"cli_arguments":["--ephemeral"],"mcp_servers":{"reader":{"configuration":mcp.reference(),"integration":"reader"}},"integrations":{"reader":integration}},"skills":{"review":skill},"context_requirements":["task","project-rules"],
      "build_configuration":resources.configuration,"credential_purpose":"evaluator",
      "permissions":model.profile.project_permissions,"budget":BudgetLimit::new(1,120_000,1000,1000,16*1024*1024).unwrap()
    })).unwrap()).unwrap();
    let mut default_definition = profile.definition().clone();
    default_definition.name = FactoryText::new("default").unwrap();
    default_definition.model_identifier = Some("finite-json-model".into());
    default_definition
      .model_parameters
      .insert(FactoryKey::new("reasoning_effort").unwrap(), json!("low"));
    let default =
      FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &role, default_definition).unwrap();
    Self {
      role,
      profile,
      default,
      settings,
      model_discovery,
    }
  }
  pub(super) async fn configure_and_check_catalog(&self, client: &Client, origin: &str, project: &str) {
    let builds_path = format!("{origin}/api/v1/projects/{project}/builds?limit=25");
    let builds_before = get_json(client, builds_path.clone()).await;
    let path = format!("/api/v1/projects/{project}/factory-profile-capabilities/pilot-selected");
    let runtime_path = format!("/api/v1/projects/{project}/factory-runtimes/pilot-selected");
    let choices_path =
      format!("{origin}/api/v1/projects/{project}/factory-configuration-choices?kind=node_capability&limit=100");
    let configuration =
      json!(self.profile.definition().execution.mcp_servers[&FactoryKey::new("reader").unwrap()].configuration);
    let has_mcp = |page: Value| {
      assert!(
        page["next_cursor"].is_null(),
        "the fixture catalog must fit on one page"
      );
      page["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["reference"]["reference"] == configuration)
    };
    assert!(
      !has_mcp(get_json(client, choices_path.clone()).await),
      "MCP configuration must not be preinstalled before Runtime settings publication"
    );
    let request = json!({"expected_version":null,"definition":self.settings});
    let reviewed = post_management(
      client,
      origin,
      &format!("{runtime_path}/validate"),
      "pilot-runtime-review",
      request.clone(),
    )
    .await;
    assert_eq!(reviewed["settings"]["definition"], request["definition"]);
    post_management(client, origin, &runtime_path, "pilot-runtime-publish", request).await;
    assert!(
      has_mcp(get_json(client, choices_path).await),
      "Runtime settings publication must expose the exact configured MCP reference"
    );
    let role = &self.role;
    let profile = &self.profile;
    let default = &self.default;
    post_management(
      client,
      origin,
      &format!("/api/v1/projects/{project}/factory-role-publications"),
      "pilot-role-create",
      json!({
        "role":{"id":role.reference().id,"expected_current_version":null,"definition":role.definition()},"profiles":[]
      }),
    )
    .await;
    let available = get_json(client, format!("{origin}{path}")).await;
    assert_eq!(
      available["definition"]["execution"]["mcp_servers"]["reader"],
      json!(profile.definition().execution.mcp_servers[&FactoryKey::new("reader").unwrap()])
    );
    let available_execution: FactoryRoleProfileExecution =
      serde_json::from_value(available["definition"]["execution"].clone()).unwrap();
    let reader = FactoryKey::new("reader").unwrap();
    for selected in [profile, default] {
      let mut definition = selected.definition().clone();
      definition.execution.mcp_servers = [(reader.clone(), available_execution.mcp_servers[&reader].clone())].into();
      definition.execution.integrations = [(reader.clone(), available_execution.integrations[&reader].clone())].into();
      let checked = post_management(
        client,
        origin,
        &format!("/api/v1/projects/{project}/factory-role-profiles/validate"),
        "pilot-selected-profile-review",
        json!({
          "id":selected.reference().id,"role":role.reference(),"capability":"pilot-selected","definition":definition
        }),
      )
      .await;
      assert!(
        checked["incompatibility"].is_null(),
        "selected configured MCP must qualify: {checked}"
      );
      assert_eq!(checked["profile"], json!(selected));
    }
    let publication = post_management(client, origin, &format!("/api/v1/projects/{project}/factory-role-publications"), "pilot-role-publish", json!({
      "profiles":[{"id":profile.reference().id,"expected_current_version":null,"role":role.reference(),"definition":profile.definition()},
        {"id":default.reference().id,"expected_current_version":null,"role":role.reference(),"definition":default.definition()}]
    })).await;
    assert!(publication["role"].is_null());
    assert_eq!(publication["profiles"][0], json!(profile));
    assert_eq!(publication["profiles"][1], json!(default));

    assert_eq!(
      get_json(client, builds_path).await,
      builds_before,
      "Runtime and profile publication must start no Build"
    );
    let runtime = get_json(
      client,
      format!("{origin}/api/v1/projects/{project}/factory-runtimes/pilot-selected"),
    )
    .await;
    assert!(
      runtime["settings"].is_object(),
      "MCP must be published through Runtime settings before selecting the profile"
    );
    let capability = get_json(client, format!("{origin}{path}")).await;
    assert_eq!(
      capability["model_catalog"]["manual_model"], true,
      "the plugin declares manual model support"
    );
    assert!(
      capability["model_catalog"]["models"]
        .as_array()
        .unwrap()
        .iter()
        .any(|model| model["identifier"] == "finite-json-model")
    );
    let profiles = format!(
      "/api/v1/projects/{project}/factory-roles/{}/profiles",
      self.role.reference().id
    );
    let before = get_json(client, format!("{origin}{profiles}")).await;
    let mut names: Vec<_> = before["items"]
      .as_array()
      .unwrap()
      .iter()
      .map(|profile| profile["name"].as_str().unwrap())
      .collect();
    names.sort();
    assert_eq!(names, ["default", "pro"]);
    let runtime = std::path::Path::new(
      self.model_discovery["environment"]["OCTA_CODEX_EXECUTABLE"]
        .as_str()
        .unwrap(),
    );
    fs::write(
      runtime.with_extension("models"),
      json!({"data":[{
      "model":"finite-next-model", "displayName":"Refreshed model", "supportedReasoningEfforts":[]
    }],"nextCursor":null})
      .to_string(),
    )
    .unwrap();
    let refreshed = post_management(
      client,
      origin,
      &format!("{path}/refresh"),
      "pilot-model-refresh",
      json!({}),
    )
    .await;
    assert!(refreshed["model_catalog"]["available"].as_bool().unwrap());
    assert_eq!(
      refreshed["model_catalog"]["models"][0]["identifier"],
      "finite-next-model"
    );
    assert_eq!(
      get_json(client, format!("{origin}{profiles}")).await,
      before,
      "refresh cannot rewrite profile versions"
    );
  }
  pub(super) async fn replace_catalog(&self, client: &Client, origin: &str, project: &str) {
    let imported=post_management(client,origin,&format!("/api/v1/projects/{project}/factory-skills"),"pilot-replace-skill",json!({"identity":"review-contract","files":[{"path":"SKILL.md","sha256":FactoryDigest::content_sha256(b"New skill, for future tasks only."),"bytes":b"New skill, for future tasks only.".to_vec()}]})).await;
    let mut role = self.role.definition().clone();
    role.instructions = FactoryInstructions::new(vec!["New instructions, for future tasks only.".into()]).unwrap();
    let next = self
      .role
      .replace(FactoryRoleVersion::new(2).unwrap(), role.clone())
      .unwrap();
    let mut profile = self.profile.definition().clone();
    profile.skills.insert(
      FactoryKey::new("review").unwrap(),
      serde_json::from_value(imported["bundle"].clone()).unwrap(),
    );
    profile
      .model_parameters
      .insert(FactoryKey::new("reasoning_effort").unwrap(), json!("low"));
    let published=post_management(client,origin,&format!("/api/v1/projects/{project}/factory-role-publications"),"pilot-replace-role",json!({
      "role":{"id":self.role.reference().id,"expected_current_version":1,"definition":role},
      "profiles":[{"id":self.profile.reference().id,"expected_current_version":1,"role":next.reference(),"definition":profile}]
    })).await;
    assert_eq!(published["role"]["reference"]["version"], 2);
    assert_eq!(published["profiles"][0]["reference"]["version"], 2);
  }
  pub(super) async fn retained(&self, postgres: &str, builds: &Value) -> Vec<u8> {
    use octacity_server_store::FactoryRoleInvocationStore;
    let pool = sqlx::postgres::PgPoolOptions::new()
      .max_connections(2)
      .connect(postgres)
      .await
      .unwrap();
    let store = octacity_server_store_postgres::PostgresStore::new(pool.clone());
    let operation = serde_json::from_value(builds["items"][0]["record"]["operation_id"].clone()).unwrap();
    let retained = store
      .factory_role_invocation(operation)
      .await
      .unwrap()
      .expect("native compiler retained the first package");
    assert_eq!(retained.package.inputs().role, self.role);
    assert_eq!(retained.package.inputs().profile, self.profile);
    assert_eq!(retained.skills.as_ref().unwrap().inputs.len(), 3);
    pool.close().await;
    serde_json::to_vec(&retained).unwrap()
  }
  pub(super) fn catalog(&self, catalog: &mut Value) {
    let mut ceiling = self.profile.definition().clone();
    ceiling.execution.mcp_servers.clear();
    ceiling.execution.integrations.clear();
    catalog["projects"][0]["role_profiles"] = json!({"pilot-selected":{
      "settings_support":{"mcp_transports":["stdio"],"mcp_executables":{MCP_EXECUTABLE:self.profile.definition().toolchain.executable}},
      "model_discovery":self.model_discovery,
      "ceiling":ceiling,"parameters":FlowDataSchema::new(FactoryKey::new("pilot.parameters").unwrap(),FactoryKey::new("v1").unwrap(),FlowValueSchema::Object{fields:BTreeMap::from([(FactoryKey::new("reasoning_effort").unwrap(),FlowFieldSchema::required(FlowValueSchema::Enum{values:vec!["high".into()]}))])}).unwrap(),
      "public_environment":{"LANGUAGE":FlowDataSchema::new(FactoryKey::new("pilot.language").unwrap(),FactoryKey::new("v1").unwrap(),FlowValueSchema::Enum{values:vec!["ru".into()]}).unwrap()},"enforcement":PermissionCategory::ALL
    }});
    catalog["projects"][0]["references"].as_array_mut().unwrap().push(json!({"kind":"node_capability","alias":"pilot-selected","reference":self.profile.definition().toolchain.executable}));
    for reference in self.profile.definition().skills.values() {
      catalog["projects"][0]["references"]
        .as_array_mut()
        .unwrap()
        .push(json!({"kind":"node_capability","alias":reference.identity(),"reference":reference}));
    }
  }
  pub(super) fn select(
    &self,
    observation: &mut Value,
    mut binding: FlowBuildBinding,
    installed: &models::InstalledModel,
    artifacts: &Value,
  ) {
    binding.task_digest =
      octacity_server_application::factory_role_task_digest(installed.profile.task_digest(), &self.profile);
    observation.as_object_mut().unwrap().remove("build");
    observation["profile_selection"] = json!(FlowProfileSelectionBinding {
      role: self.role.reference().clone(),
      candidates: BTreeMap::from([(
        FactoryKey::new("explicit").unwrap(),
        FlowProfileCandidate {
          profile: self.profile.clone(),
          build: binding
        }
      )]),
      policy: FlowProfileSelectionPolicy::Manual {
        candidate: Some(FactoryKey::new("explicit").unwrap())
      },
      failure: FlowProfileSelectionFailure::ManualSelection,
      selection_budget: BudgetUsage::default()
    });
    let source = |name: &str| {
      let file = artifacts["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["name"] == name)
        .unwrap();
      json!(FlowDataMapping::new(FlowValueMapping::Constant { value:json!({"kind":"artifact","value":{"artifact_id":file["id"],"content_digest":file["sha256"],"encoded_size":file["size_bytes"]}}) }).unwrap())
    };
    observation["context_binding"] = json!({"entries":{
      "task":{"source_kind":"task","source":source("task")},
      "project-rules":{"source_kind":"policy","source":source("acceptance")}
    }});
  }
}

const MCP_EXECUTABLE: &str = "/opt/octacity/tools/tool-0";
const MCP_OPTION: &str = "--config=mcp_servers={ reader = { args = [\"--mcp\", \"/octacity/protected/skills/review/server.mjs\"], command = \"/opt/octacity/tools/tool-0\", enabled_tools = [\"facts\"], env_vars = [\"LANGUAGE\"], required = true } }";

async fn managed_reader(postgres: &str, project: octacity_server_domain::ProjectId) -> FactoryRoleIntegrationBinding {
  use octacity_server_domain::{IntegrationId, IntegrationVersion, Timestamp};
  use octacity_server_store::*;
  let pin = |name: &str| {
    ImmutableReference::new(
      FactoryKey::new(name).unwrap(),
      FactoryKey::new("v1").unwrap(),
      FactoryDigest::content_sha256(name.as_bytes()),
    )
  };
  let installed:InstalledIntegration=serde_json::from_value(json!({"installation":pin("pilot.connection"),"provider":"qualified-local-reader","adapter":pin("pilot.reader"),"endpoints":["https://reader.invalid/"],"authentication":["local"],"resources":[{"reference":pin("pilot.facts"),"native_id":"facts","locator":"facts","capabilities":[{"purpose":"task_read","capability":pin("pilot.read"),"configuration":pin("pilot.reader-config"),"credential":{"provider":"installed","reference":"pilot/local-reader"}}]}]})).unwrap();
  let draft=serde_json::from_value(json!({"name":"Local retained reader","installation":installed.installation,"endpoint":"https://reader.invalid/","authentication":"local","resources":[{"reference":pin("pilot.facts"),"purposes":["task_read"]}]})).unwrap();
  let connection = IntegrationId::generate();
  let capability = installed.resources[0].capabilities[0].clone();
  let pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(2)
    .connect(postgres)
    .await
    .unwrap();
  let store = octacity_server_store_postgres::PostgresStore::new(pool.clone());
  let at = Timestamp::from_unix_millis(1).unwrap();
  store
    .publish_integration_connection(mutation(PublishIntegrationConnection {
      intent: IntegrationConnectionIntent {
        id: connection,
        expected_current_version: None,
        draft,
      },
      installed,
      idempotency_key: IdempotencyKey::new("role-pilot-connection").unwrap(),
      published_at: at,
    }))
    .await
    .unwrap();
  store
    .publish_integration_project_access(mutation(PublishIntegrationProjectAccess {
      intent: IntegrationProjectAccessIntent {
        connection_id: connection,
        project_id: project,
        connection_version: IntegrationVersion::INITIAL,
        expected_current_version: None,
        resources: vec![IntegrationResourceSelection {
          reference: pin("pilot.facts"),
          purposes: vec![IntegrationPurpose::TaskRead],
        }],
      },
      idempotency_key: IdempotencyKey::new("role-pilot-access").unwrap(),
      published_at: at,
    }))
    .await
    .unwrap();
  // Prepare the credential-free local fixture through the same current administration journal.
  let preparation = store
    .begin_integration_administration(mutation(BeginIntegrationAdministration {
      intent: IntegrationAdministrationIntent {
        id: connection,
        expected_metadata_version: IntegrationVersion::INITIAL,
        expected_revision: 0,
        action: IntegrationAdministrationAction::Credential {
          reference: capability.credential,
          kind: octacity_server_application::CredentialPublicationKind::Adopt,
          activate_connection: true,
        },
      },
      idempotency_key: IdempotencyKey::new("role-pilot-ready").unwrap(),
      requested_at: at,
    }))
    .await
    .unwrap();
  store
    .complete_integration_administration(mutation(preparation))
    .await
    .unwrap();
  pool.close().await;
  FactoryRoleIntegrationBinding {
    project_id: project,
    connection_id: connection,
    connection_version: IntegrationVersion::INITIAL,
    access_version: IntegrationVersion::INITIAL,
    resource: pin("pilot.facts"),
    capability: pin("pilot.read"),
  }
}
fn mutation<T>(request: T) -> octacity_server_store::ManagementMutation<T> {
  use octacity_server_store::*;
  ManagementMutation::new(
    request,
    MutationAuditContext::try_new(
      AuditActor {
        kind: AuditActorKind::AuthenticatedManagement,
        identity: Some("released-pilot".into()),
      },
      ManagementSecurityScope::new("released-pilot").unwrap(),
      "released-pilot",
    )
    .unwrap(),
  )
}

const MCP_SERVER: &[u8] = include_bytes!("../../fixtures/factory/role-mcp.mjs");
