//! Released role journey through ordinary publication, selection and retained inputs.
use super::*;
use octacity_server_factory::*;
use std::collections::BTreeMap;

pub(super) struct RolePilot {
  role: FactoryRole,
  profile: FactoryRoleProfile,
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
    let imported = post_management(client, origin, &format!("/api/v1/projects/{project}/factory-skills"), "pilot-role-skill", json!({
      "identity":"review-contract", "files":[
        {"path":"SKILL.md", "sha256":FactoryDigest::content_sha256(b"# Review contract\nRead supporting.txt.\n"), "bytes":b"# Review contract\nRead supporting.txt.\n".to_vec()},
        {"path":"supporting.txt", "sha256":FactoryDigest::content_sha256(b"Retained supporting facts.\n"), "bytes":b"Retained supporting facts.\n".to_vec()},
        {"path":"server.mjs", "sha256":FactoryDigest::content_sha256(MCP_SERVER), "bytes":MCP_SERVER}
      ]})).await;
    let skill: ImmutableReference = serde_json::from_value(imported["bundle"].clone()).unwrap();
    let integration = managed_reader(postgres, project).await;
    let mcp = octacity_server_application::FactoryCodexMcpConfiguration::new(
      FactoryKey::new("pilot-mcp").unwrap(),
      FactoryKey::new("v1").unwrap(),
      octacity_server_application::FactoryCodexMcpServer::Stdio {
        command: "/usr/local/bin/node".into(),
        args: vec!["/octacity/protected/skills/review/server.mjs".into()],
        env_vars: vec!["LANGUAGE".into()],
        enabled_tools: Some(vec!["facts".into()]),
      },
      Some(integration.clone()),
    )
    .unwrap();
    model.profile.mcp_configurations = vec![mcp.clone()];
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
      "invocation":{"prefix":prefix,"options":["--config=model_reasoning_effort=\"high\"","--ephemeral","--ignore-user-config",MCP_OPTION],"environment":{"LANGUAGE":"ru","HOME":"/workspace/scratch"}},
      "mcp":{"command":"/usr/local/bin/node","args":["/octacity/protected/skills/review/server.mjs"],"env_vars":["LANGUAGE"],"tool":"facts","expected_text":"ru:Retained supporting facts.\n"},
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
    arguments.insert(
      5,
      json!({"kind":"exact","value":"--config=model_reasoning_effort=\"high\""}),
    );
    let last = arguments.len() - 1;
    for option in ["--ephemeral", "--ignore-user-config", MCP_OPTION].into_iter().rev() {
      arguments.insert(last, json!({"kind":"exact","value":option}));
    }
    model.profile.project_permissions = serde_json::from_value(grants).unwrap();
    model.profile.local_permissions = LocalPermissionCeiling::fully_enforced(model.profile.project_permissions.clone());
    let profile = FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &role, serde_json::from_value(json!({
      "name":"Explicit pilot selection","instructions":["Keep the result bounded."],"toolchain":model.profile.toolchain,
      "model_parameters":{"reasoning_effort":"high"},"thinking_parameter":"reasoning_effort",
      "execution":{"environment":{"LANGUAGE":{"kind":"public","value":"ru"}},"cli_arguments":["--ephemeral"],"mcp_servers":{"reader":{"configuration":mcp.reference(),"integration":"reader"}},"integrations":{"reader":integration}},"skills":{"review":skill},"context_requirements":["task","project-rules"],
      "build_configuration":resources.configuration,"credential_purpose":"evaluator",
      "permissions":model.profile.project_permissions,"budget":BudgetLimit::new(1,120_000,1000,1000,16*1024*1024).unwrap()
    })).unwrap()).unwrap();
    let publication = post_management(client, origin, &format!("/api/v1/projects/{project}/factory-role-publications"), "pilot-role-publish", json!({
      "role":{"id":role.reference().id,"expected_current_version":null,"definition":role.definition()},
      "profiles":[{"id":profile.reference().id,"expected_current_version":null,"role":role.reference(),"definition":profile.definition()}]
    })).await;
    assert_eq!(publication["role"], json!(role));
    assert_eq!(publication["profiles"][0], json!(profile));
    Self { role, profile }
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
    catalog["projects"][0]["role_profiles"] = json!({"pilot-selected":{
      "ceiling":self.profile.definition(),"parameters":FlowDataSchema::new(FactoryKey::new("pilot.parameters").unwrap(),FactoryKey::new("v1").unwrap(),FlowValueSchema::Object{fields:BTreeMap::from([(FactoryKey::new("reasoning_effort").unwrap(),FlowFieldSchema::required(FlowValueSchema::Enum{values:vec!["high".into()]}))])}).unwrap(),
      "public_environment":{"LANGUAGE":FlowDataSchema::new(FactoryKey::new("pilot.language").unwrap(),FactoryKey::new("v1").unwrap(),FlowValueSchema::Enum{values:vec!["ru".into()]}).unwrap()},"enforcement":PermissionCategory::ALL
    }});
    catalog["projects"][0]["references"].as_array_mut().unwrap().push(json!({"kind":"node_capability","alias":"pilot-selected","reference":self.profile.definition().toolchain.executable}));
    for reference in self.profile.definition().skills.values().chain(
      self
        .profile
        .definition()
        .execution
        .mcp_servers
        .values()
        .map(|binding| &binding.configuration),
    ) {
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

const MCP_OPTION: &str = "--config=mcp_servers={ reader = { args = [\"/octacity/protected/skills/review/server.mjs\"], command = \"/usr/local/bin/node\", enabled_tools = [\"facts\"], env_vars = [\"LANGUAGE\"], required = true } }";

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
