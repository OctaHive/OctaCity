//! Source qualification uses the existing real Agent resources and Git transport.
//! No application handler or executable is substituted by the test.
use super::*;
#[path = "source_triage_console.rs"]
mod console_journey;

#[allow(dead_code)]
#[path = "../factory_pilot/models.rs"]
mod models;
#[path = "source_triage_profiles.rs"]
mod profiles;
#[path = "source_triage_reporting.rs"]
mod reporting;
#[path = "source_triage_reproduction.rs"]
mod reproduction;
#[allow(dead_code)]
#[path = "../factory_pilot/resources.rs"]
mod resources;
#[path = "../factory_pilot/source.rs"]
mod source;

fn worker_limits(backend: &ReleaseBackend) -> octacity_server_factory::FactoryResourceLimits {
  octacity_server_factory::FactoryResourceLimits::new(3000, 1536 * 1024 * 1024, backend.workspace_bytes(), 128, 120_000)
    .unwrap()
}

struct ExecutionEnvironment {
  environment: source_environment::SourceEnvironment,
  directory: tempfile::TempDir,
  backend: ReleaseBackend,
  release: octacity_release_harness::InstalledRelease,
  resources: resources::PilotResources,
  revision: String,
  run: String,
  plugins: PathBuf,
  agent_directory: PathBuf,
  enrollment: String,
}
impl ExecutionEnvironment {
  async fn start() -> Self {
    let backend = ReleaseBackend::from_environment();
    let release = release::load(&backend);
    let environment = source_environment::SourceEnvironment::start_with_execution(&release.toolchain).await;
    let directory = tempfile::tempdir().unwrap();
    set_mode(directory.path(), 0o700);
    let repository = directory.path().join("repository");
    fs::create_dir(&repository).unwrap();
    fs::write(
      repository.join("Octafile.yml"),
      json!({"version":1,"tasks":{"prepare":{"shell":"printf 'qualified Source worker\\n'"}}}).to_string(),
    )
    .unwrap();
    fs::write(
      repository.join("health.mjs"),
      "import { createServer } from 'node:http';\nexport const server = createServer((_request, response) => { response.writeHead(500); response.end('unexpected health status'); });\n",
    )
    .unwrap();
    let git = required_path("OCTACITY_CONTRACT_GIT_EXECUTABLE", true);
    for arguments in [
      vec!["init", "--quiet", "--initial-branch=main"],
      vec!["add", "--all"],
      vec!["commit", "--quiet", "-m", "qualification input"],
    ] {
      let output = Command::new(&git)
        .arg("-C")
        .arg(&repository)
        .args(arguments)
        .env("GIT_AUTHOR_NAME", "Qualification")
        .env("GIT_COMMITTER_NAME", "Qualification")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.test")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.test")
        .output()
        .await
        .unwrap();
      assert!(output.status.success(), "finite repository setup failed");
    }
    let output = Command::new(&git)
      .arg("-C")
      .arg(&repository)
      .args(["rev-parse", "HEAD"])
      .output()
      .await
      .unwrap();
    assert!(output.status.success());
    let revision = String::from_utf8(output.stdout).unwrap().trim().to_owned();
    let run = Uuid::new_v4().to_string();
    let remote = format!("https://pilot.invalid/{run}.git");
    let plugins = source::install_local_mirror(
      &directory.path().join("mirror"),
      &release.source_plugins,
      &remote,
      &repository,
      &git,
    );
    let resources = resources::create_resources_with_limits(
      &environment.client,
      &environment.origin,
      &run,
      &backend,
      &remote,
      worker_limits(&backend),
    )
    .await;
    let enrollment = resources::issue_enrollment(
      &environment.client,
      &environment.origin,
      &run,
      &resources.pool,
      &backend,
    )
    .await;
    let agent_directory = directory.path().join("agent");
    fs::create_dir(&agent_directory).unwrap();
    Self {
      environment,
      directory,
      backend,
      release,
      resources,
      revision,
      run,
      plugins,
      agent_directory,
      enrollment,
    }
  }

  fn spawn(&self, overrides: &AgentConfigOverrides<'_>, installed: Option<&Path>) -> Child {
    let config = write_agent_config(
      &self.agent_directory,
      &self.environment.agent_origin(),
      &format!("triage-{}", self.run),
      &self.enrollment,
      AgentReleasePaths {
        octa_root: &self.release.octa_root,
        source_plugins: &self.plugins,
      },
      &self.backend,
      overrides,
    );
    if let Some(installed) = installed {
      agent::apply_installed_agent_configuration(&config, installed).unwrap();
    }
    spawn_agent(
      &self.release.agent_binary,
      &config,
      &self.agent_directory.join("stdout.log"),
      &self.agent_directory.join("stderr.log"),
    )
  }

  async fn ordinary_job(&self) -> Value {
    let trigger = post_management(&self.environment.client,&self.environment.origin,"/api/v1/triggers/manual",&format!("triage-bootstrap-{}",self.run),json!({
    "trigger_id":self.resources.trigger,"trigger_version":1,"configuration_id":self.resources.configuration.id(),"configuration_version":1,
    "deduplication_identity":format!("triage-bootstrap-{}",self.run),"source":{"kind":"exact_revision","value":self.revision},"parameters":{},"priority":50
  })).await;
    let build = string(&trigger, "build_id");
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
      let observed = get_json(
        &self.environment.client,
        format!("{}/api/v1/builds/{build}", self.environment.origin),
      )
      .await;
      if matches!(observed["state"].as_str(), Some("succeeded" | "failed" | "cancelled")) {
        break observed;
      }
      assert!(
        Instant::now() < deadline,
        "the qualified Agent must execute the normal Job: {observed}"
      );
      sleep(Duration::from_millis(100)).await;
    }
  }
}

pub(super) async fn agent_bootstrap() {
  let mut execution = ExecutionEnvironment::start().await;
  let objects = required_string("OCTACITY_MINIO_ENDPOINT");
  let origins = [objects.as_str()];
  let mut overrides = AgentConfigOverrides::restricted_without_outputs(&origins);
  overrides.output_limits = Some(octacity_protocol::OutputLimits {
    artifact_count: 16,
    artifact_bytes: 4 * 1024 * 1024,
    report_count: 16,
    report_bytes: 4 * 1024 * 1024,
    single_output_bytes: 4 * 1024 * 1024,
  });
  let mut agent = execution.spawn(&overrides, None);
  let result = execution.ordinary_job().await;
  agent.kill().await.unwrap();
  agent.wait().await.unwrap();
  execution.environment.stop().await;
  assert_eq!(
    result["state"], "succeeded",
    "the Source server must use the actual installed Job contract: {result}"
  );
  fs::write(
    execution.environment.evidence.join("triage-worker-bootstrap.json"),
    serde_json::to_vec_pretty(&result).unwrap(),
  )
  .unwrap();
}

pub(super) async fn pipeline(defect: bool) {
  run_pipeline(defect, false).await;
}

pub(super) async fn console() {
  run_pipeline(false, true).await;
}

pub(super) async fn console_results() {
  // The independently qualified headless setup isolates retained Console reads
  // from first-time authoring, while both paths use the same normal execution.
  run_pipeline(true, false).await;
}

async fn run_pipeline(defect: bool, console: bool) {
  let mut execution = ExecutionEnvironment::start().await;
  let project = execution.resources.project.clone();
  if defect || console {
    execution.environment.review_reporter_labels(&["ready / review"]).await;
  }
  execution
    .environment
    .install_source_completion(&project, &execution.revision)
    .await;
  let connection = source_setup::enroll_resource(
    &execution.environment.client,
    &execution.environment.origin,
    &execution.environment.installation,
    &project,
    &execution.environment.installation.resource,
    true,
  )
  .await;
  let installed = if console {
    profiles::install_for_console(&mut execution, &connection).await
  } else {
    profiles::install(&mut execution, defect, &connection).await
  };
  let mut target = installed["target"].clone();
  {
    let mut replies = execution.environment.provider.replies.lock().await;
    replies.clear();
    let mut candidate = source_provider::issue(9001, 17);
    candidate["title"] = json!(if defect {
      "Defect: health endpoint returns an unexpected status"
    } else {
      "Feature: expose bounded triage evidence"
    });
    for _ in 0..128 {
      replies.push_back(source_provider::Reply::json(json!([candidate.clone()])));
    }
  }
  if defect || console {
    let mut native = source_provider::reporting::Issue::new(&[]);
    native.title = if defect {
      "Defect: health endpoint returns an unexpected status"
    } else {
      "Feature: expose bounded triage evidence"
    }
    .into();
    *execution.environment.provider.reporting.lock().await = Some(native);
  }
  let choices = get_json(
    &execution.environment.client,
    format!(
      "{}/api/v1/projects/{project}/factory-source-choices?connection_id={connection}&connection_version=1",
      execution.environment.origin
    ),
  )
  .await;
  let source = choices["items"]
    .as_array()
    .unwrap()
    .iter()
    .find(|item| item["purpose"] == "task_read")
    .unwrap();
  let repository = get_json(
    &execution.environment.client,
    format!(
      "{}/api/v1/repositories/{}/versions/1",
      execution.environment.origin, execution.resources.repository
    ),
  )
  .await;
  let repository = json!({"project_id":project,"id":execution.resources.repository,"version":1,"definition_digest":repository["definition_digest"]});
  let objects = required_string("OCTACITY_MINIO_ENDPOINT");
  let origins = [objects.as_str()];
  let mut overrides = AgentConfigOverrides::restricted_without_outputs(&origins);
  overrides.allowed_network_hosts = &["host.microsandbox.internal"];
  overrides.output_limits = Some(octacity_protocol::OutputLimits {
    artifact_count: 16,
    artifact_bytes: 4 * 1024 * 1024,
    report_count: 16,
    report_bytes: 4 * 1024 * 1024,
    single_output_bytes: 4 * 1024 * 1024,
  });
  let mut worker = execution.spawn(
    &overrides,
    Some(Path::new(
      installed["authority"]["agent_configuration"].as_str().unwrap(),
    )),
  );
  let generated: toml::Value = toml::from_str(
    &fs::read_to_string(execution.directory.path().join("stand-composition/config/agent.toml")).unwrap(),
  )
  .unwrap();
  let registration_deadline = Instant::now() + Duration::from_secs(30);
  loop {
    let agents = get_json(
      &execution.environment.client,
      format!("{}/api/v1/agents?limit=100", execution.environment.origin),
    )
    .await;
    if let Some(registered) = agents["items"].as_array().unwrap().first() {
      assert_eq!(
        registered["name"].as_str(),
        generated["agent_id"].as_str(),
        "the active Agent must consume the generated installation identity"
      );
      break;
    }
    assert!(Instant::now() < registration_deadline, "installed Agent must register");
    sleep(Duration::from_millis(100)).await;
  }
  let mut console_setup = None;
  if console {
    let result = execution
      .environment
      .browser("factory-triage-setup.spec.ts", &project)
      .await;
    if let Err(error) = result {
      worker.kill().await.unwrap();
      worker.wait().await.unwrap();
      execution.environment.stop().await;
      panic!("{error}");
    }
    let records: Vec<_> = fs::read_dir(execution.environment.evidence.join("browser-results"))
      .unwrap()
      .filter_map(|entry| entry.ok().map(|entry| entry.path().join("triage-console-setup.json")))
      .filter(|path| path.is_file())
      .collect();
    assert_eq!(records.len(), 1);
    let observed: Value = serde_json::from_slice(&fs::read(&records[0]).unwrap()).unwrap();
    target = observed["configuration"].clone();
    console_setup = Some(observed);
  }
  let id = console_setup.as_ref().map_or_else(Uuid::new_v4, |observed| {
    observed["input"]["id"].as_str().unwrap().parse().unwrap()
  });
  let mut intent = json!({"project_id":project,"group":null,"inputs":[{"id":id,"expected_version":null,
    "definition":{"target":target,"repository":repository,"source":source["binding"],"enabled":true,"reporter":null,
      "predicate":{"state":"open","label_presence":"none","all_of":[],"any_of":[],"none_of":[]},
      "connection":{"id":connection,"version":1,"access_version":1,"source_configuration":source["connection_configuration"],"reporter_configuration":null}}}]});
  if defect {
    let selected = &installed["reporting"];
    intent["inputs"][0]["definition"]["reporter"] = selected["reporter"].clone();
    intent["inputs"][0]["definition"]["reporting"] = selected["mapping"].clone();
    intent["inputs"][0]["definition"]["connection"]["reporter_configuration"] = selected["configuration"].clone();
  }
  let input_publication = if let Some(observed) = &console_setup {
    observed["publication"].clone()
  } else {
    let preview = post_management(
      &execution.environment.client,
      &execution.environment.origin,
      &format!("/api/v1/projects/{project}/factory-sources/preview"),
      &format!("{}-raw-preview", execution.run),
      json!({"intent":intent,"limit":10}),
    )
    .await;
    assert_eq!(
      preview["candidates"][0]["summary"]["labels"],
      json!([]),
      "the original task is unlabeled"
    );
    assert_eq!(
      preview["candidates"][0]["summary"]["title"],
      if defect {
        "Defect: health endpoint returns an unexpected status"
      } else {
        "Feature: expose bounded triage evidence"
      }
    );
    source_setup::publish(
      &execution.environment.client,
      &execution.environment.origin,
      &format!("/api/v1/projects/{project}/factory-source-inputs/{id}/versions"),
      &id.to_string(),
      "if-none-match",
      "*",
      json!({"intent":intent}),
    )
    .await
  };
  let deadline = Instant::now() + Duration::from_secs(30);
  let input_version = if console { 2 } else { 1 };
  let admissions_path = format!(
    "{}/api/v1/projects/{project}/factory-source-inputs/{id}/versions/{input_version}/diagnostics",
    execution.environment.origin
  );
  let run = loop {
    let diagnostics = get_json(&execution.environment.client, admissions_path.clone()).await;
    if let Some(run) = diagnostics["admissions"][0]["association"]["run_id"].as_str() {
      break run.to_owned();
    }
    assert!(
      Instant::now() < deadline,
      "raw triage must reach ordinary admission: {diagnostics}"
    );
    sleep(Duration::from_millis(100)).await;
  };
  let deadline = Instant::now() + Duration::from_secs(180);
  let completions = loop {
    let observed = get_json(
      &execution.environment.client,
      format!(
        "{}/api/v1/factory-runs/{run}/node-completions?limit=100",
        execution.environment.origin
      ),
    )
    .await;
    if observed["items"]
      .as_array()
      .unwrap()
      .iter()
      .filter(|item| item["record"]["public_result"].is_object())
      .count()
      >= 2
    {
      break observed;
    }
    if Instant::now() >= deadline {
      break observed;
    }
    let state = get_json(
      &execution.environment.client,
      format!("{}/api/v1/factory-runs/{run}", execution.environment.origin),
    )
    .await;
    if state["state"] == "completed" {
      break observed;
    }

    let builds = get_json(
      &execution.environment.client,
      format!(
        "{}/api/v1/factory-runs/{run}/builds?limit=100",
        execution.environment.origin
      ),
    )
    .await;
    let mut technical_stop = false;
    for item in builds["items"].as_array().unwrap() {
      let build = item["record"]["build_id"].as_str().unwrap();
      let detail = get_json(
        &execution.environment.client,
        format!("{}/api/v1/builds/{build}", execution.environment.origin),
      )
      .await;
      technical_stop |= matches!(detail["state"].as_str(), Some("failed" | "cancelled"));
    }
    if technical_stop {
      break observed;
    }
    sleep(Duration::from_millis(500)).await;
  };
  let mut delivery = json!({"items":[]});
  if defect || console {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
      delivery = get_json(
        &execution.environment.client,
        format!(
          "{}/api/v1/factory-runs/{run}/effect-delivery?limit=100",
          execution.environment.origin
        ),
      )
      .await;
      if delivery["items"].as_array().unwrap().iter().any(|item| {
        item["record"]["kind"] == "work.status.projection"
          && matches!(item["record"]["state"].as_str(), Some("delivered" | "failed"))
      }) || Instant::now() >= deadline
      {
        break;
      }
      sleep(Duration::from_millis(250)).await;
    }
  }
  let terminal_deadline = Instant::now() + Duration::from_secs(10);
  let final_run = loop {
    let observed = get_json(
      &execution.environment.client,
      format!("{}/api/v1/factory-runs/{run}", execution.environment.origin),
    )
    .await;
    if observed["state"] == "completed" || Instant::now() >= terminal_deadline {
      break observed;
    }
    sleep(Duration::from_millis(250)).await;
  };
  fs::write(
    execution.environment.evidence.join("triage-node-completions.json"),
    serde_json::to_vec_pretty(&completions).unwrap(),
  )
  .unwrap();
  let mut diagnostics = serde_json::Map::new();
  diagnostics.insert("effect-delivery".into(), delivery.clone());
  diagnostics.insert("run".into(), final_run.clone());
  for collection in ["node-attempts", "builds", "calls", "flow-runs"] {
    let observed = get_json(
      &execution.environment.client,
      format!(
        "{}/api/v1/factory-runs/{run}/{collection}?limit=100",
        execution.environment.origin
      ),
    )
    .await;
    diagnostics.insert(collection.into(), observed);
  }
  for link in diagnostics["builds"]["items"].as_array().unwrap() {
    let build = link["record"]["build_id"].as_str().unwrap();
    let detail = get_json(
      &execution.environment.client,
      format!("{}/api/v1/builds/{build}", execution.environment.origin),
    )
    .await;
    fs::write(
      execution
        .environment
        .evidence
        .join(format!("triage-build-{build}.json")),
      serde_json::to_vec_pretty(&detail).unwrap(),
    )
    .unwrap();
  }
  fs::write(
    execution.environment.evidence.join("triage-execution-diagnostics.json"),
    serde_json::to_vec_pretty(&diagnostics).unwrap(),
  )
  .unwrap();
  let provider_paths: Vec<_> = execution
    .environment
    .provider
    .requests
    .lock()
    .await
    .iter()
    .map(|request| request.lines().next().unwrap_or("").to_owned())
    .collect();
  fs::write(
    execution.environment.evidence.join("triage-provider-paths.json"),
    serde_json::to_vec_pretty(&provider_paths).unwrap(),
  )
  .unwrap();
  if let Some(setup) = &console_setup {
    console_journey::resume(&mut execution, &mut worker, &installed, &run, setup).await;
  }
  if std::env::var_os("OCTACITY_TEST_TRIAGE_CONSOLE_RESULTS").is_some() {
    let result = browser::run_browser_scenario(
      &required_path("OCTACITY_RELEASE_UI_ROOT", true),
      &execution.environment.origin,
      "factory-triage-results.spec.ts",
      &[
        ("OCTACITY_RELEASE_PROJECT_ID", &project),
        ("OCTACITY_RELEASE_TRIAGE_RUN", &run),
      ],
      &execution.environment.evidence,
    )
    .await;
    if let Err(error) = result {
      worker.kill().await.unwrap();
      worker.wait().await.unwrap();
      execution.environment.stop().await;
      panic!("{error}");
    }
  }
  worker.kill().await.unwrap();
  worker.wait().await.unwrap();
  execution.environment.stop().await;
  let accepted: Vec<_> = completions["items"]
    .as_array()
    .unwrap()
    .iter()
    .filter(|item| item["record"]["public_result"].is_object())
    .collect();
  assert_eq!(
    accepted.len(),
    2,
    "both triage stages must produce accepted public results: {completions}"
  );
  let project_result = accepted
    .iter()
    .find(|item| item["record"]["node_key"] == "eligibility_gate")
    .unwrap();
  let lookup = &project_result["record"]["public_result"]["lookup"];
  assert_eq!(lookup["capture_status"], "complete");
  let calls = lookup["observations"].as_array().unwrap();
  assert_eq!(calls.len(), 4, "retain every native lookup observation");
  let observed: std::collections::BTreeSet<_> = calls
    .iter()
    .map(|call| {
      (
        call["connection"].as_str().unwrap(),
        call["tool"].as_str().unwrap(),
        call["status"].as_str().unwrap(),
      )
    })
    .collect();
  assert_eq!(
    observed,
    [
      ("issues", "search_issues", "completed"),
      ("issues", "read_issue", "completed"),
      ("external", "search_issues", "completed"),
      ("external", "issue_read", "completed")
    ]
    .into_iter()
    .collect()
  );
  if defect {
    assert!(
      completions["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["record"]["node_key"] == "reproduction"),
      "a defect requires an accepted independent command observation before classification: {completions}"
    );
    let classification = accepted
      .iter()
      .find(|item| item["record"]["node_key"] == "classification_gate")
      .unwrap();
    assert_eq!(classification["record"]["outcome"], "requirements");
    assert_eq!(
      classification["record"]["public_result"]["assessment"]["reproducibility"],
      "reproduced"
    );
  }
  if defect || console {
    let requests = execution.environment.provider.requests.lock().await;
    let writes: Vec<_> = requests.iter().filter(|request| !request.starts_with("GET ")).collect();
    assert_eq!(
      writes.len(),
      1,
      "accepted defect triage must deliver the one reviewed native label effect"
    );
    assert!(
      writes[0]
        .to_lowercase()
        .contains(&format!("authorization: bearer {}", source_evidence::WRITER)),
      "use the independently selected Reporter credential"
    );
    let receipt: Vec<_> = delivery["items"]
      .as_array()
      .unwrap()
      .iter()
      .filter(|item| item["record"]["kind"] == "work.status.projection")
      .collect();
    assert_eq!(receipt.len(), 1, "one logical native effect has an independent receipt");
    assert_eq!(receipt[0]["record"]["state"], "delivered");
    assert!(writes[0].starts_with("POST /api/v3/repos/owner/tickets/issues/17/labels HTTP/"));
    let state = execution.environment.provider.reporting.lock().await;
    let state = state.as_ref().expect("retain the actual native task state");
    assert_eq!(state.labels, ["ready / review".to_owned()].into_iter().collect());
    assert_eq!(
      state.state, "open",
      "triage does not claim the reproduced defect was fixed"
    );
  }
  if !defect && !console {
    assert!(
      execution
        .environment
        .provider
        .requests
        .lock()
        .await
        .iter()
        .all(|request| request.starts_with("GET ")),
      "an unconfigured Reporter emits no provider write"
    );
  }
  assert_eq!(final_run["state"], "completed", "the configured triage flow completes");
  if let Some(observed) = &console_setup {
    assert_eq!(
      final_run["issue_association"]["issue_id"], observed["issue"]["id"],
      "explicit import associates the original task"
    );
  } else {
    assert!(
      final_run["issue_association"].is_null(),
      "Source-only triage never imports a canonical card"
    );
  }
  let profiles: std::collections::BTreeSet<_> = accepted
    .iter()
    .flat_map(|item| item["record"]["result_provenance"].as_array().unwrap())
    .filter_map(|call| call["profile"]["selected_profile"]["id"].as_str())
    .collect();
  assert_eq!(profiles.len(), 2, "the stages must invoke distinct exact role profiles");
  fs::write(
    execution.environment.evidence.join("triage-profile-results.json"),
    serde_json::to_vec_pretty(&json!({
      "schema_version":1,"image":execution.backend.immutable_image(),
      "server_sha256":octacity_server_factory::FactoryDigest::content_sha256(&fs::read(required_path("OCTACITY_RELEASE_SERVER_ROOT",false).join("bin/octacity-server")).unwrap()),
      "agent_sha256":octacity_server_factory::FactoryDigest::content_sha256(&fs::read(&execution.release.agent_binary).unwrap()),
      "native_codex_sha256":installed["native_codex_sha256"],"external_mcp_sha256":installed["external_mcp_sha256"],
      "factory_catalog_sha256":installed["factory_catalog_sha256"],"profiles":installed["profiles"],
      "agent_release":execution.release.agent_manifest,"toolchain":execution.release.toolchain,
      "configuration":target,"input_publication":input_publication,
      "input_id":id,"run_id":run,"repository":repository,"source":source["binding"],"node_completions":completions,"execution":diagnostics
    }))
    .unwrap(),
  )
  .unwrap();
  execution
    .environment
    .record(&project, "packaged-triage-products.json")
    .await;
}
