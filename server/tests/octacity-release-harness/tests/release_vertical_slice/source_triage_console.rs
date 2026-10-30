//! Console qualification resumes the original installation, Agent and journals.
//! The upstream updater owns staging/preflight/exchange; this fixture only stops
//! and starts its already installed processes at that owner's barriers.
use super::*;

async fn inspect(execution: &ExecutionEnvironment, run: &str, storage: &Path, phase: &str) {
  let evidence = execution.environment.evidence.join(phase);
  fs::create_dir(&evidence).unwrap();
  browser::run_browser_scenario(
    &required_path("OCTACITY_RELEASE_UI_ROOT", true),
    &execution.environment.origin,
    "factory-triage-results.spec.ts",
    &[
      ("OCTACITY_RELEASE_PROJECT_ID", &execution.resources.project),
      ("OCTACITY_RELEASE_TRIAGE_RUN", run),
      ("OCTACITY_RELEASE_TRIAGE_KIND", "feature"),
      ("OCTACITY_RELEASE_TRIAGE_PHASE", phase),
      ("OCTACITY_RELEASE_TRIAGE_STORAGE", storage.to_str().unwrap()),
    ],
    &evidence,
  )
  .await
  .unwrap();
}

async fn snapshot(execution: &ExecutionEnvironment, run: &str) -> Value {
  let environment = &execution.environment;
  let mut records = serde_json::Map::new();
  for collection in ["node-completions", "builds", "calls", "effect-delivery"] {
    let page = get_json(
      &environment.client,
      format!(
        "{}/api/v1/factory-runs/{run}/{collection}?limit=100",
        environment.origin
      ),
    )
    .await;
    records.insert(collection.into(), page["items"].clone());
  }
  records.insert(
    "run".into(),
    get_json(
      &environment.client,
      format!("{}/api/v1/factory-runs/{run}", environment.origin),
    )
    .await,
  );
  let issue = records["run"]["issue_association"]["issue_id"].as_str().unwrap();
  records.insert(
    "issue".into(),
    get_json(
      &environment.client,
      format!(
        "{}/api/v1/projects/{}/issues/{issue}",
        environment.origin, execution.resources.project
      ),
    )
    .await,
  );
  let agents = get_json(
    &environment.client,
    format!("{}/api/v1/agents?limit=100", environment.origin),
  )
  .await;
  assert_eq!(agents["items"].as_array().unwrap().len(), 1);
  records.insert("agent_id".into(), agents["items"][0]["id"].clone());
  Value::Object(records)
}

async fn barrier(process: &mut Child, root: &Path, name: &str) {
  let deadline = Instant::now() + Duration::from_secs(60);
  while !root.join(name).exists() {
    if process.try_wait().unwrap().is_some() {
      let diagnostic = private_file(
        &env::temp_dir(),
        &format!("octacity-update-{}.stderr.log", Uuid::new_v4()),
        &fs::read_to_string(root.join("stderr.log")).unwrap_or_default(),
      );
      panic!(
        "installation updater exited before {name}; private diagnostics: {}",
        diagnostic.display()
      );
    }
    assert!(Instant::now() < deadline, "installation updater did not reach {name}");
    sleep(Duration::from_millis(50)).await;
  }
}

pub(super) async fn resume(
  execution: &mut ExecutionEnvironment,
  worker: &mut Child,
  installed: &Value,
  run: &str,
  setup: &Value,
) {
  let state = fs::read_dir(execution.environment.evidence.join("browser-results"))
    .unwrap()
    .filter_map(|entry| entry.ok().map(|entry| entry.path().join("triage-console-state.json")))
    .find(|path| path.is_file())
    .unwrap();
  inspect(execution, run, &state, "before-update").await;
  let before = snapshot(execution, run).await;
  let active_configuration = execution.agent_directory.join("agent.toml");
  let generated_configuration = Path::new(installed["authority"]["agent_configuration"].as_str().unwrap());
  let original: toml::Table = toml::from_str(&fs::read_to_string(&active_configuration).unwrap()).unwrap();
  let credential_path = Path::new(original["credential_file"].as_str().unwrap());
  assert_eq!(
    credential_path.canonicalize().unwrap(),
    execution
      .directory
      .path()
      .join("stand-composition/agent/credential")
      .canonicalize()
      .unwrap()
  );
  let credential = fs::read(credential_path).unwrap();
  let root = execution.directory.path().join("installation-update");
  fs::create_dir(&root).unwrap();
  set_mode(&root, 0o700);
  let request = private_file(
    &root,
    "request.json",
    &json!({
      "root":execution.directory.path().join("stand-composition"),
      "native":execution.directory.path().join("stand-native"),
      "catalog":installed["authority"]["catalog"],
      "server":execution.environment.server_binary(),
      "configuration":execution.environment.configuration_path(),
      "barriers":root
    })
    .to_string(),
  );
  let repository = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap();
  let mut update = Command::new("python3").current_dir(repository)
    .args(["-c", "from tools.tests.test_factory_triage_pipeline import update_installation; import sys; update_installation(sys.argv[1])"])
    .arg(request).kill_on_drop(true)
    .stdout(Stdio::from(File::create(root.join("stdout.log")).unwrap()))
    .stderr(Stdio::from(File::create(root.join("stderr.log")).unwrap())).spawn().unwrap();
  barrier(&mut update, &root, "preflight-rejected").await;
  assert_eq!(
    snapshot(execution, run).await,
    before,
    "failed preflight leaves the original processes and records usable"
  );
  fs::write(root.join("checked"), "").unwrap();
  barrier(&mut update, &root, "stop").await;
  worker.kill().await.unwrap();
  worker.wait().await.unwrap();
  execution.environment.stop_server().await;
  fs::write(root.join("stopped"), "").unwrap();
  barrier(&mut update, &root, "updated").await;
  execution.environment.start_server().await;
  // The update must preserve the credential before the normal coordinator
  // promotes it to the resumed registration session during Agent startup.
  assert!(
    fs::read(credential_path).unwrap() == credential,
    "the original Agent credential is retained"
  );
  let previous = get_json(
    &execution.environment.client,
    format!("{}/api/v1/agents?limit=100", execution.environment.origin),
  )
  .await;
  agent::apply_installed_agent_configuration(&active_configuration, generated_configuration).unwrap();
  let resumed: toml::Table = toml::from_str(&fs::read_to_string(&active_configuration).unwrap()).unwrap();
  for field in [
    "agent_id",
    "credential_file",
    "work_root",
    "state_root",
    "cache",
    "factory_permissions",
    "factory_credential_profiles",
    "tool_executables",
    "allowed_network_hosts",
  ] {
    assert!(
      resumed.get(field) == original.get(field),
      "the reviewed installation must retain Agent {field}"
    );
  }
  *worker = spawn_agent(
    &execution.release.agent_binary,
    &active_configuration,
    &execution.agent_directory.join("resumed.stdout.log"),
    &execution.agent_directory.join("resumed.stderr.log"),
  );
  // Observe authenticated registration from the baseline before startup.
  // Idle Agents do not send Job heartbeats; a new last-seen observation with
  // the original identity proves that the retained credential is usable.
  let deadline = Instant::now() + Duration::from_secs(30);
  loop {
    let agents = get_json(
      &execution.environment.client,
      format!("{}/api/v1/agents?limit=100", execution.environment.origin),
    )
    .await;
    assert_eq!(agents["items"].as_array().unwrap().len(), 1);
    assert_eq!(agents["items"][0]["id"], before["agent_id"]);
    if agents["items"][0]["last_seen_at_unix_ms"] != previous["items"][0]["last_seen_at_unix_ms"] {
      break;
    }
    assert!(
      Instant::now() < deadline,
      "the original Agent must register with its retained credential after restart"
    );
    sleep(Duration::from_millis(200)).await;
  }
  assert_eq!(
    snapshot(execution, run).await,
    before,
    "startup preserves original invocations, accepted results and effect receipts"
  );
  let capabilities = get_json(
    &execution.environment.client,
    format!(
      "{}/api/v1/projects/{}/factory-profile-capabilities/triage-runtime",
      execution.environment.origin, execution.resources.project
    ),
  )
  .await;
  assert!(
    capabilities["public_environment"]["QUALIFICATION_REVIEWED"].is_object(),
    "the restarted server must consume the exchanged catalog, not the previous generation"
  );
  let inputs = get_json(
    &execution.environment.client,
    format!(
      "{}/api/v1/projects/{}/factory-sources",
      execution.environment.origin, execution.resources.project
    ),
  )
  .await;
  assert!(
    inputs["items"]
      .as_array()
      .unwrap()
      .iter()
      .any(|input| input["definition"] == setup["input"]),
    "the operator's enabled Source selection remains unchanged: {inputs}"
  );
  inspect(execution, run, &state, "after-update").await;
  fs::write(root.join("resumed"), "").unwrap();
  let status = timeout(Duration::from_secs(10), update.wait()).await.unwrap().unwrap();
  assert!(
    status.success(),
    "installation update failed; private diagnostics retained until fixture cleanup"
  );
  fs::write(
    execution.environment.evidence.join("triage-console-restart.json"),
    serde_json::to_vec_pretty(&json!({
      "before":before,"after":snapshot(execution,run).await,"preflight_rejected_before_stop":true,
      "operator_preferences":"russian, dark; retained browser context", "update_owner":"factory_installation_update"
    }))
    .unwrap(),
  )
  .unwrap();
}
