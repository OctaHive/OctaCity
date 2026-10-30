//! Shared packaged environment for Source runtime and integration-setup qualification.
//! Processes start through their installed executables, never alternate application installers.
use super::*;
use octacity_release_harness::InstalledBrowserRelease;
use std::os::unix::fs::MetadataExt as _;

pub(super) struct SourceEnvironment {
  pub(super) client: Client,
  pub(super) origin: String,
  pub(super) installation: source_installation::Installation,
  pub(super) provider: source_provider::Forge,
  pub(super) evidence: PathBuf,
  release: InstalledBrowserRelease,
  _directory: tempfile::TempDir,
  config: PathBuf,
  direct: String,
  agent_addr: SocketAddr,
  generation: u32,
  server: Child,
  nginx: Child,
}
impl SourceEnvironment {
  pub(super) async fn start() -> Self {
    Self::start_installed(source_installation::Scope::Tasks).await
  }

  /// Install native status support; connection credentials/purposes remain a separate review.
  pub(super) async fn start_with_reporter() -> Self {
    Self::start_installed(source_installation::Scope::Status).await
  }

  pub(super) async fn start_with_factory() -> Self {
    Self::start_installed(source_installation::Scope::Factory).await
  }

  /// Execution qualification uses the exact installed Job contract, independently
  /// of the metadata-only browser fixture's deliberately unused runner pins.
  pub(super) async fn start_with_execution(toolchain: &Toolchain) -> Self {
    Self::start_configured(source_installation::Scope::Factory, toolchain).await
  }

  pub(super) fn agent_origin(&self) -> String {
    format!("http://{}", self.agent_addr)
  }

  /// Explicit operator bootstrap; the normal runtime validates the private reader
  /// reference. Investigation receives one Project/Issue and no mutation scope.
  pub(super) async fn install_issue_reader(&mut self, project: &str, issue: &str, token: &str) {
    self.install_issue_readers(project, &[issue], token).await;
  }

  /// Keep an explicitly reviewed finite Issue scope for pagination qualification.
  pub(super) async fn install_issue_readers(&mut self, project: &str, issues: &[&str], token: &str) {
    let credential = private_file(self._directory.path(), "issue-mcp-reader", token);
    let mut config: toml::Value = toml::from_str(&fs::read_to_string(&self.config).unwrap()).unwrap();
    config.as_table_mut().unwrap().insert(
      "issue_mcp_readers".into(),
      toml::Value::try_from(json!([{
        "id":"packaged-triage", "project_id":project, "issue_ids":issues,
        "credential_file":credential, "expires_at_unix_ms":chrono::Utc::now().timestamp_millis()+600_000
      }]))
      .unwrap(),
    );
    fs::write(&self.config, toml::to_string(&config).unwrap()).unwrap();
    self.restart().await;
  }

  /// Review the bundled process criteria for this Project through the existing
  /// deployment-owned Source mapping. No candidate supplies acceptance policy.
  pub(super) async fn install_source_completion(&mut self, project: &str, revision: &str) {
    use octacity_server_application::{SourceCompletionDocument, TRIAGE_COMPLETION_PRESET};
    use sha2::{Digest as _, Sha256};
    let catalog: Value = serde_json::from_slice(&fs::read(&self.installation.catalog).unwrap()).unwrap();
    let path = Path::new(catalog["source_profiles_file"].as_str().unwrap());
    let mut profiles: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let reader = profiles["readers"]
      .as_array_mut()
      .unwrap()
      .iter_mut()
      .find(|reader| reader["identity"] == "github-issues")
      .unwrap();
    reader["preparation"]["mapping"]["require_body"] = json!(false);
    reader["preparation"]["source"] = json!({"kind":"exact_revision","value":revision});
    reader["preparation"]["mapping"]["acceptance"] = json!({"kind":"process_completion",
      "value":SourceCompletionDocument::review_preset(project.parse().unwrap(),TRIAGE_COMPLETION_PRESET).unwrap()});
    let bytes = serde_json::to_vec(&profiles).unwrap();
    fs::write(path, &bytes).unwrap();
    self.installation.profiles_sha256 = hex::encode(Sha256::digest(&bytes));
    self.restart().await;
  }

  pub(super) async fn review_reporter_labels(&mut self, labels: &[&str]) {
    self.installation.review_reporter_labels(labels);
    self.restart().await;
  }

  async fn start_installed(scope: source_installation::Scope) -> Self {
    Self::start_configured(scope, &browser::browser_toolchain()).await
  }

  async fn start_configured(scope: source_installation::Scope, toolchain: &Toolchain) -> Self {
    let evidence = required_path("OCTACITY_RELEASE_EVIDENCE_DIR", false).join(format!("source-{}", Uuid::new_v4()));
    fs::create_dir_all(&evidence).unwrap();
    let release = browser::load_browser_release();
    let directory = tempfile::tempdir().unwrap();
    let forge = source_provider::Forge::start().await;
    let mut candidate = source_provider::issue(9001, 17);
    if scope == source_installation::Scope::Factory {
      candidate["body"] = json!("Bounded source task\n\n## Acceptance\nPreserve the reviewed native task identity.");
    }
    for _ in 0..128 {
      forge
        .replies
        .lock()
        .await
        .push_back(source_provider::Reply::json(json!([candidate.clone()])));
    }
    let installation = source_installation::install(directory.path(), &forge, scope);
    let (management_addr, agent_addr) = unused_loopback_addresses();
    let socket = directory.path().join("management.sock");
    let config = write_server_config(
      directory.path(),
      &ServerConfigInput {
        postgres_url: &required_string("OCTACITY_POSTGRES_URL"),
        object_endpoint: &required_string("OCTACITY_MINIO_ENDPOINT"),
        cache_endpoint: "https://cache.invalid",
        toolchain,
        management_addr,
        agent_addr,
        cache_addr: None,
      },
    );
    let mut contents = fs::read_to_string(&config).unwrap();
    contents.push_str(&format!(
      "\n[integration_management]\nsocket_path={}\npeer_uid={}\nprincipal=\"packaged-nginx\"\ncatalog_file={}\n",
      toml_string(&socket),
      fs::metadata(directory.path()).unwrap().uid(),
      toml_string(&installation.catalog)
    ));
    fs::write(&config, contents).unwrap();
    let server_stdout = evidence.join("source-server.stdout.log");
    let server_stderr = evidence.join("source-server.stderr.log");
    let mut server = spawn_server(&release.server_binary, &config, &server_stdout, &server_stderr);
    let direct = format!("http://{management_addr}");
    let client = Client::new();
    wait_for_server_ready(&client, &direct, &mut server, &server_stdout).await;
    assert_eq!(
      client
        .get(format!("{direct}/api/v1/integration-installations"))
        .send()
        .await
        .unwrap()
        .status(),
      403
    );
    let proxy_addr = browser::unused_loopback_address();
    let nginx_config = browser::write_nginx_config_with_upstream(
      directory.path(),
      &release,
      &format!("unix:{}", socket.display()),
      proxy_addr,
      &evidence.join("source-nginx.error.log"),
    );
    let nginx_log = evidence.join("source-nginx.stderr.log");
    let mut nginx = browser::spawn_nginx(
      &required_path("OCTACITY_RELEASE_NGINX", true),
      &nginx_config,
      &evidence.join("source-nginx.stdout.log"),
      &nginx_log,
    );
    let origin = format!("http://{proxy_addr}");
    browser::wait_for_proxy(&client, &origin, &mut nginx, &nginx_log).await;
    Self {
      client,
      origin,
      installation,
      provider: forge,
      evidence,
      release,
      _directory: directory,
      config,
      direct,
      agent_addr,
      generation: 0,
      server,
      nginx,
    }
  }

  /// Deployment bootstrap is explicit; subsequent Project grants/input publications need no restart.
  pub(super) async fn install_factory(&mut self, profiles: &Value) {
    let profiles = private_file(self._directory.path(), "factory-profiles.json", &profiles.to_string());
    self.install_factory_file(&profiles).await;
  }

  /// Keep the generated installation path authoritative across atomic updates.
  pub(super) async fn install_factory_file(&mut self, profiles: &Path) {
    let mut config: toml::Value = toml::from_str(&fs::read_to_string(&self.config).unwrap()).unwrap();
    let fields = config.as_table_mut().unwrap();
    fields.insert(
      "factory_profiles_file".into(),
      toml::Value::String(profiles.to_str().unwrap().to_owned()),
    );
    fields.insert(
      "factory".into(),
      toml::Value::try_from(json!({"poll_interval_milliseconds":50,
      "claim_lifetime_milliseconds":30_000,"batch_size":4,"concurrency":2}))
      .unwrap(),
    );
    fs::write(&self.config, toml::to_string(&config).unwrap()).unwrap();
    self.restart().await;
  }

  /// Restart the same normal executable/configuration over its original PostgreSQL/S3 journals.
  pub(super) async fn restart(&mut self) {
    self.stop_server().await;
    self.start_server().await;
  }

  /// Crash injection for durable recovery qualification; no shutdown handler acknowledges pending work.
  pub(super) async fn crash_and_restart(&mut self) -> (u32, u32) {
    let previous = self.server.id().expect("packaged server is running");
    self.server.kill().await.unwrap();
    self.server.wait().await.unwrap();
    self.start_server().await;
    let current = self.server.id().expect("replacement packaged server is running");
    assert_ne!(previous, current);
    (previous, current)
  }

  pub(super) async fn stop_server(&mut self) {
    shutdown_server(&mut self.server, &self.evidence.join("source-server.stdout.log")).await;
  }

  pub(super) fn configuration_path(&self) -> &Path {
    &self.config
  }

  pub(super) fn server_binary(&self) -> &Path {
    &self.release.server_binary
  }

  pub(super) async fn start_server(&mut self) {
    self.generation += 1;
    let stdout = self
      .evidence
      .join(format!("source-server-{}.stdout.log", self.generation));
    let stderr = self
      .evidence
      .join(format!("source-server-{}.stderr.log", self.generation));
    self.server = spawn_server(&self.release.server_binary, &self.config, &stdout, &stderr);
    wait_for_server_ready(&self.client, &self.direct, &mut self.server, &stdout).await;
  }

  pub(super) async fn browser(&self, scenario: &str, project: &str) -> Result<(), String> {
    browser::run_browser_scenario(
      &required_path("OCTACITY_RELEASE_UI_ROOT", true),
      &self.origin,
      scenario,
      &[("OCTACITY_RELEASE_PROJECT_ID", project)],
      &self.evidence,
    )
    .await
  }

  pub(super) async fn browser_phase(&self, scenario: &str, project: &str) -> Result<(), String> {
    let evidence = self.evidence.join(scenario.trim_end_matches(".spec.ts"));
    fs::create_dir_all(&evidence).unwrap();
    browser::run_browser_scenario(
      &required_path("OCTACITY_RELEASE_UI_ROOT", true),
      &self.origin,
      scenario,
      &[("OCTACITY_RELEASE_PROJECT_ID", project)],
      &evidence,
    )
    .await
  }

  pub(super) async fn stop(&mut self) {
    browser::shutdown_process(&mut self.nginx, &self.evidence.join("source-nginx.stderr.log"), "nginx").await;
    shutdown_server(&mut self.server, &self.evidence.join("source-server.stdout.log")).await;
  }

  pub(super) async fn record(&self, project: &str, file_name: &str) {
    fs::write(self.evidence.join(file_name), serde_json::to_vec_pretty(&json!({
      "server_release":self.release.server_manifest,"console_release":self.release.console_manifest,
      "adapter_sha256":self.installation.adapter_sha256,"provider_configuration_sha256":self.installation.configuration_sha256,
      "source_profiles_sha256":self.installation.profiles_sha256,
      "project_id":project,"same_origin":self.origin,"provider_requests":self.provider.requests.lock().await.len(),
      "reporter_installed":self.installation.reporter,
      "provider_boundary":"hermetic HTTPS GitHub API; real verified adapter process"
    })).unwrap()).unwrap();
    source_evidence::verify(&self.evidence).expect("only public qualification metadata may be retained");
  }
}
