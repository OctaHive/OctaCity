//! Released server configuration and process lifecycle shared by pilot scenarios.
use super::*;

pub(super) struct FactoryServerFiles<'a> {
  pub(super) profiles: &'a Path,
  pub(super) connectors: &'a Path,
}

pub(super) fn write_factory_server_config(
  directory: &Path,
  input: &ServerConfigInput<'_>,
  files: &FactoryServerFiles<'_>,
) -> PathBuf {
  let path = write_server_config(directory, input);
  let mut config: toml::Value = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
  let fields = config.as_table_mut().unwrap();
  fields.insert(
    "factory_profiles_file".into(),
    toml::Value::String(files.profiles.to_str().unwrap().to_owned()),
  );
  fields.insert(
    "factory_signal_connectors_file".into(),
    toml::Value::String(files.connectors.to_str().unwrap().to_owned()),
  );
  fields.insert(
    "factory".into(),
    toml::Value::try_from(serde_json::json!({
      "poll_interval_milliseconds": 50,
      "claim_lifetime_milliseconds": 30000,
      "batch_size": 4,
      "concurrency": 2,
    }))
    .unwrap(),
  );
  fs::write(&path, toml::to_string(&config).unwrap()).unwrap();
  path
}

pub(super) struct ServerConfigInput<'a> {
  pub(super) postgres_url: &'a str,
  pub(super) object_endpoint: &'a str,
  pub(super) cache_endpoint: &'a str,
  pub(super) toolchain: &'a Toolchain,
  pub(super) management_addr: SocketAddr,
  pub(super) agent_addr: SocketAddr,
  pub(super) cache_addr: Option<SocketAddr>,
}

pub(super) fn write_server_config(directory: &Path, input: &ServerConfigInput<'_>) -> PathBuf {
  let postgres = private_file(directory, "postgres-url", input.postgres_url);
  let access_key = private_file(directory, "object-access-key", "octacity");
  let secret_key = private_file(directory, "object-secret-key", "octacity-secret");
  let signing_key = private_file(directory, "signing-key", &STANDARD.encode(SIGNING_SEED));
  let enrollment_key = private_file(directory, "agent-enrollment-key", &STANDARD.encode([8_u8; 32]));
  let cache_key = private_file(directory, "cache-credential-key", &STANDARD.encode([9_u8; 32]));
  let policy = directory.join("job-spec-policy.json");
  fs::write(
    &policy,
    serde_json::to_vec(&input.toolchain.job_spec_policy(900).unwrap()).unwrap(),
  )
  .unwrap();
  let cache_bind = input
    .cache_addr
    .map_or_else(String::new, |address| format!("cache_bind = \"{address}\"\n"));
  let contents = format!(
    r#"
management_bind = "{}"
agent_bind = "{}"
{}
shutdown_grace_milliseconds = 5000
readiness_check_interval_milliseconds = 100
readiness_check_timeout_milliseconds = {RELEASE_READINESS_TIMEOUT_MILLISECONDS}
agent_registration_lifetime_milliseconds = 900000
agent_enrollment_lifetime_milliseconds = 900000
agent_lease_lifetime_milliseconds = {RELEASE_JOB_AUTHORITY_LIFETIME_MILLISECONDS}
supported_pipeline_capabilities = ["native", "oci.process", "oci.hypervisor", "shell"]

[postgres]
url_file = {}

[object_storage]
endpoint = {}
region = "us-east-1"
bucket = "octacity-artifacts"
force_path_style = true
operation_timeout_milliseconds = {RELEASE_OBJECT_OPERATION_TIMEOUT_MILLISECONDS}
access_key_file = {}
secret_key_file = {}

[signing]
key_id = "test-key"
key_file = {}

[agent_credentials]
enrollment_key_file = {}

[cache]
endpoint = {}
credential_key_file = {}
session_lifetime_milliseconds = {RELEASE_JOB_AUTHORITY_LIFETIME_MILLISECONDS}

[job_spec]
policy_file = {}
"#,
    input.management_addr,
    input.agent_addr,
    cache_bind,
    toml_string(&postgres),
    toml_text(input.object_endpoint),
    toml_string(&access_key),
    toml_string(&secret_key),
    toml_string(&signing_key),
    toml_string(&enrollment_key),
    toml_text(input.cache_endpoint),
    toml_string(&cache_key),
    toml_string(&policy)
  );
  let path = directory.join("server.toml");
  fs::write(&path, contents).unwrap();
  path
}

pub(super) fn spawn_server(binary: &Path, config: &Path, stdout: &Path, stderr: &Path) -> Child {
  Command::new(binary)
    .arg("--log-format")
    .arg("json")
    .arg("--log-filter")
    .arg("info")
    .arg("run")
    .arg(config)
    .stdout(Stdio::from(File::create(stdout).unwrap()))
    .stderr(Stdio::from(File::create(stderr).unwrap()))
    .kill_on_drop(true)
    .spawn()
    .unwrap()
}

pub(super) async fn wait_for_server_ready(client: &Client, origin: &str, server: &mut Child, log: &Path) {
  let deadline = Instant::now() + Duration::from_secs(60);
  loop {
    if let Some(status) = server.try_wait().unwrap() {
      panic!(
        "released server exited before readiness with {status}: {}",
        fs::read_to_string(log).unwrap_or_default()
      );
    }
    if client
      .get(format!("{origin}/health/ready"))
      .send()
      .await
      .is_ok_and(|response| response.status().is_success())
    {
      return;
    }
    assert!(
      Instant::now() < deadline,
      "timed out waiting for released server readiness: {}",
      fs::read_to_string(log).unwrap_or_default()
    );
    sleep(Duration::from_millis(100)).await;
  }
}

pub(super) async fn shutdown_server(server: &mut Child, log: &Path) {
  let process_id = server.id().expect("released server must still be running");
  let process_id = rustix::process::Pid::from_raw(process_id as i32).unwrap();
  rustix::process::kill_process(process_id, rustix::process::Signal::TERM).unwrap();
  let status = timeout(Duration::from_secs(30), server.wait())
    .await
    .expect("released server did not stop before its graceful-shutdown deadline")
    .unwrap();
  assert!(
    status.success(),
    "released server exited unsuccessfully: {}",
    fs::read_to_string(log).unwrap_or_default()
  );
}

pub(super) fn unused_loopback_addresses() -> (SocketAddr, SocketAddr) {
  let management = TcpListener::bind("127.0.0.1:0").unwrap();
  let agent = TcpListener::bind("127.0.0.1:0").unwrap();
  (management.local_addr().unwrap(), agent.local_addr().unwrap())
}
