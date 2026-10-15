//! Installed reader and server-only Shadow connector for the real Agent pilot.
use super::*;
use octacity_server_application::{FactoryCodexToolRiskPolicy, FactoryToolRiskBinding};
use octacity_server_factory::*;
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;

pub(super) const PUBLIC_FILE: &str = "/workspace/source/public/facts.txt";
pub(super) const MODEL_CANARY: &str = "finite-model-private-canary";
const SIGNAL_CANARY: &str = "sk-glm5-pilot-private-tool-signal";
const SOURCE_CANARY: &str = "unselected-source-canary";
fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn pin(name: &str, bytes: &[u8]) -> ImmutableReference {
  ImmutableReference::new(key(name), key("1"), FactoryDigest::content_sha256(bytes))
}

pub(super) struct InstalledTools {
  pub(super) binding: FactoryToolRiskBinding,
  pub(super) reader: toml::Table,
  pub(super) fixture: jev::JevHttpsFixture,
  pub(super) permissions: FactoryPermissionSet,
}
impl InstalledTools {
  pub(super) async fn install(directory: &Path, model: &mut models::InstalledModel, backend: &ReleaseBackend) -> Self {
    let distribution = directory.join("reader-release");
    fs::create_dir(&distribution).unwrap();
    set_mode(&distribution, 0o700);
    for (environment, name) in [
      ("OCTACITY_CONTRACT_PROTECTED_READER", "worker"),
      ("OCTACITY_CONTRACT_CODEX_READER", "bridge"),
    ] {
      fs::copy(required_path(environment, true), distribution.join(name)).unwrap();
      set_mode(&distribution.join(name), 0o500);
    }
    let worker = pin(
      "installed-reader-worker",
      &fs::read(distribution.join("worker")).unwrap(),
    );
    let tool = pin("installed-read-operation", b"bounded-rooted-read-operation");
    let profile = &mut model.profile;
    profile.consumer = FactoryCredentialConsumer::ToolReasoning;
    profile.instructions=FactoryText::new(json!({"fixture_version":1,"tools":[{"path":PUBLIC_FILE}],"checks":[{"path":PUBLIC_FILE,"readable":false}],"result":{"$tool":0}}).to_string()).unwrap();
    profile.toolchain.model = ImmutableReference::new(
      profile.toolchain.model.identity().clone(),
      profile.toolchain.model.version().clone(),
      FactoryDigest::content_sha256(profile.instructions.as_str().as_bytes()),
    );
    profile.usage_policy = octacity_server_application::FactoryCodexUsagePolicy::TokenRates {
      model: profile.toolchain.model.clone(),
      input_micro_units_per_million: 1_000_000,
      cached_input_micro_units_per_million: 1_000_000,
      output_micro_units_per_million: 1_000_000,
    };
    profile.tool_response_limits = Some(octacity_protocol::FactoryToolResponseLimitsV3 {
      per_call_bytes: 5,
      total_bytes: 5,
    });
    let mut grants = serde_json::to_value(&profile.project_permissions).unwrap();
    grants["secret_profiles"] = json!([profile.credentials.profile(FactoryCredentialPurpose::Model)]);
    grants["executables"].as_array_mut().unwrap().push(json!(worker));
    grants["tools"] = json!([tool]);
    grants["commands"]
      .as_array_mut()
      .unwrap()
      .push(json!({"executable":worker,"arguments":[{"kind":"exact","value":PUBLIC_FILE}]}));
    grants["mounts"]
      .as_array_mut()
      .unwrap()
      .push(json!({"root":octacity_protocol::FACTORY_SOURCE_ROOT,"mode":"read_only"}));
    grants["resources"] = json!({"cpu_millis":3000,"memory_bytes":1536*1024*1024_u64,"disk_bytes":backend.workspace_bytes(),"process_count":128,"elapsed_millis":120000});
    grants["max_descendants"] = json!(127);
    profile.project_permissions = serde_json::from_value(grants).unwrap();
    profile.local_permissions = LocalPermissionCeiling::fully_enforced(profile.project_permissions.clone());
    let schema = FlowDataSchema::new(
      key("public-redacted-proposal"),
      key("v1"),
      FlowValueSchema::Object {
        fields: BTreeMap::new(),
      },
    )
    .unwrap();
    let choices = DecisionSignalChoices::try_new(vec![key("a-deny"), key("z-allow")]).unwrap();
    let mut plan = signals::plan(
      DecisionSignalPurpose::ToolRisk,
      schema,
      FlowDataMapping::new(FlowValueMapping::Constant { value: json!({}) }).unwrap(),
      choices.as_slice(),
    );
    let old = &plan.profile;
    plan.profile = DecisionSignalProfile::new(DecisionSignalProfileDefinition {
      purpose: DecisionSignalPurpose::ToolRisk,
      provider: old.provider().clone(),
      adapter: old.adapter().clone(),
      model: old.model().clone(),
      question_set: old.question_set().clone(),
      policy: old.policy().clone(),
      mode: DecisionSignalMode::Shadow,
      fallback: DecisionSignalFallback::Deny,
      budget: old.budget(),
      routes: None,
    })
    .unwrap();
    let fixture = jev::JevHttpsFixture::start(plan.capability.model().version().as_str(), SIGNAL_CANARY).await;
    let mut signal_grants = serde_json::to_value(FactoryPermissionSet::deny_all()).unwrap();
    signal_grants["plugins"] = json!([plan.capability.adapter()]);
    signal_grants["secret_profiles"] = json!(["pilot-signal-credential"]);
    signal_grants["network_hosts"] = json!(["localhost"]);
    let signal_grants: FactoryPermissionSet = serde_json::from_value(signal_grants).unwrap();
    let binding = FactoryToolRiskBinding {
      project_id: profile.build_configuration.project_id(),
      plan,
      mapping: ToolRiskChoiceMapping::try_new(
        &choices,
        vec![
          (key("a-deny"), DecisionSignalDisposition::Deny),
          (key("z-allow"), DecisionSignalDisposition::Allow),
        ],
      )
      .unwrap(),
      max_calls: 1,
      credential_profile: key("pilot-signal-credential"),
      network_host: NetworkHost::new("localhost").unwrap(),
      project_permissions: signal_grants.clone(),
      local_permissions: LocalPermissionCeiling::fully_enforced(signal_grants),
    };
    profile.tool_risk = Some(FactoryCodexToolRiskPolicy::from_binding(&binding).unwrap());
    let mut full = serde_json::to_value(&profile.project_permissions).unwrap();
    full["plugins"]
      .as_array_mut()
      .unwrap()
      .push(json!(binding.plan.capability.adapter()));
    full["secret_profiles"]
      .as_array_mut()
      .unwrap()
      .push(json!(binding.credential_profile));
    full["network_hosts"] = json!([binding.network_host.as_str()]);
    let permissions = serde_json::from_value(full).unwrap();
    let reader_pin = |reference: &ImmutableReference| json!({"identity":reference.identity(),"version":reference.version(),"sha256":reference.digest().to_string()});
    let reader=toml::Value::try_from(json!({"release_root":distribution,"entrypoint":"worker","platform":model.platform,"tool":reader_pin(&tool),"executable":reader_pin(&worker),"resources":{"cpu_millis":1000,"memory_bytes":512*1024*1024_u64,"disk_bytes":16*1024*1024_u64,"process_count":8,"elapsed_millis":30000},"outputs":{"kinds":[],"max_artifact_count":0,"max_artifact_bytes":0,"max_report_count":0,"max_report_bytes":0},"bridge":{"entrypoint":"bridge","sha256":hex::encode(Sha256::digest(fs::read(distribution.join("bridge")).unwrap())),"hook_tool_name":"mcp__qualified_fs__read_file"}})).unwrap().as_table().unwrap().clone();
    Self {
      binding,
      reader,
      fixture,
      permissions,
    }
  }
  pub(super) fn catalog(&self, catalog: &mut Value, profile: &octacity_server_application::FactoryCodexBuildProfile) {
    let binding = &self.binding;
    catalog["codex"][0]["credential_purpose"] = json!("model");
    catalog["codex"][0]["execution_mode"] = json!("protected_tools");
    catalog["codex"][0]["tool_response_limits"] = json!(profile.tool_response_limits);
    catalog["tool_risk"] = json!([{"project_id":binding.project_id,"plan":binding.plan,"mapping":binding.mapping,"max_calls":binding.max_calls,"credential_profile":binding.credential_profile,"network_host":binding.network_host.as_str(),"project_permissions":binding.project_permissions,"local_permissions":{"grants":binding.local_permissions.grants(),"enforceable":PermissionCategory::ALL},"base_task_digest":profile.base_task_digest(),"build_configuration":profile.build_configuration}]);
    catalog["projects"][0]["references"].as_array_mut().unwrap().extend([
      json!({"kind":"node_capability","alias":"pilot-signal-adapter","reference":binding.plan.capability.adapter()}),
      json!({"kind":"secret_profile","alias":binding.credential_profile,"reference":binding.plan.profile.policy()}),
    ]);
  }
  pub(super) fn connectors(&self, directory: &Path) -> PathBuf {
    let configuration = private_file(
      directory,
      "jev.json",
      &self
        .fixture
        .connector_configuration(&self.binding.plan.capability)
        .to_string(),
    );
    let credential = private_file(directory, "jev-key", SIGNAL_CANARY);
    private_file(directory,"connectors.json",&json!({"version":1,"connectors":[{"kind":"jev","credential_profile":self.binding.credential_profile,"configuration_file":configuration,"credential_file":credential}]}).to_string())
  }
  pub(super) async fn require_ready(&self, client: &Client, origin: &str, pool: &str, name: &str, worker: &mut Child) {
    timeout(Duration::from_secs(30), async {
      loop {
        let agents = get_json(client, format!("{origin}/api/v1/agents?pool_id={pool}&limit=100")).await;
        if let Some(agent) = agents["items"]
          .as_array()
          .unwrap()
          .iter()
          .find(|agent| agent["name"] == name)
        {
          let routes = agent["inventory"]["factory_executions"].as_array().unwrap();
          assert_eq!(routes.len(), 1, "the pilot Agent qualifies only its selected MSB route");
          assert_eq!(routes[0]["execution"]["provider"], "microsandbox");
          assert_eq!(routes[0]["execution"]["mode"], "virtualization");
          assert_eq!(
            routes[0]["controlled_tools"], true,
            "the normal Agent must install its real lease-owned reader before advertising"
          );
          return;
        }
        assert!(
          worker.try_wait().unwrap().is_none(),
          "the installed controlled Agent exited before registration"
        );
        sleep(Duration::from_millis(100)).await;
      }
    })
    .await
    .expect("the installed controlled Agent must register before Work admission");
  }
  pub(super) async fn verify(&self, client: &Client, origin: &str, run: &str, builds: &Value, postgres: &str) {
    let captured = self.fixture.captured();
    assert_eq!(
      captured.len(),
      1,
      "same canonical proposal reuses its frozen Shadow receipt"
    );
    assert!(captured[0].credential_matched);
    assert_eq!(captured[0].body["state"], json!({}));
    let pool = sqlx::PgPool::connect(postgres).await.unwrap();
    let receipts: Vec<(Vec<u8>, Vec<u8>)> = sqlx::query_as(
      "SELECT receipt_digest,document FROM factory_decision_signal_receipts WHERE run_id=$1 ORDER BY request_id",
    )
    .bind(run.parse::<Uuid>().unwrap())
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(receipts.len(), 1, "one real durable ToolRisk receipt");
    let receipt: DecisionSignalReceipt = serde_json::from_slice(&receipts[0].1).unwrap();
    receipt.validate_integrity().unwrap();
    assert_eq!(receipt.receipt_digest().as_bytes().as_slice(), receipts[0].0.as_slice());
    assert_eq!(receipt.consumption().kind(), DecisionSignalConsumptionKind::Shadow);
    assert_eq!(receipt.consumption().baseline(), &DecisionSignalDisposition::Allow);
    assert_eq!(
      receipt.consumption().final_disposition(),
      &DecisionSignalDisposition::Allow
    );
    assert_eq!(
      captured[0].idempotency_key.as_deref(),
      Some(receipt.request_id().to_string().as_str())
    );
    pool.close().await;
    for build in builds["items"].as_array().unwrap() {
      let build = string(&build["record"], "build_id");
      let artifacts = get_json(client, format!("{origin}/api/v1/builds/{build}/artifacts?limit=100")).await;
      let mut observed = None;
      for artifact in artifacts["items"].as_array().unwrap() {
        let download = client
          .post(format!("{origin}/api/v1/artifacts/{}/download", string(artifact, "id")))
          .send()
          .await
          .unwrap()
          .error_for_status()
          .unwrap()
          .json::<Value>()
          .await
          .unwrap();
        let bytes = client
          .get(string(&download, "get_url"))
          .send()
          .await
          .unwrap()
          .error_for_status()
          .unwrap()
          .bytes()
          .await
          .unwrap();
        for canary in [SOURCE_CANARY, MODEL_CANARY, SIGNAL_CANARY] {
          assert!(
            !bytes.windows(canary.len()).any(|window| window == canary.as_bytes()),
            "private canary reached retained output"
          );
        }
        if let Ok(value) = serde_json::from_slice::<Value>(&bytes)
          && let Some(result) = value.get("structured_result")
        {
          observed = Some(result.clone());
        }
      }
      assert_eq!(
        observed,
        Some(json!("facts")),
        "actual retained model result came from the confined reader"
      );
    }
    let rendered = serde_json::to_string(&captured.iter().map(|entry| &entry.body).collect::<Vec<_>>()).unwrap();
    for canary in [SOURCE_CANARY, MODEL_CANARY, SIGNAL_CANARY] {
      assert!(!rendered.contains(canary));
    }
    assert!(!rendered.contains(PUBLIC_FILE));
  }
}
