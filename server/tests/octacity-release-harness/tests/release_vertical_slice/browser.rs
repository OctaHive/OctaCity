//! Released console acceptance scenario over the packaged same-origin proxy.

use octacity_release_harness::{BrowserReleaseBundles, InstalledBrowserRelease, validate_installed_browser_release};

use super::*;

pub(super) async fn run() {
  assert_eq!(env::consts::OS, "linux", "released console slice requires Linux");
  let postgres_url = required_string("OCTACITY_POSTGRES_URL");
  let object_endpoint = required_string("OCTACITY_MINIO_ENDPOINT");
  let evidence = required_path("OCTACITY_RELEASE_EVIDENCE_DIR", false);
  let ui_root = required_path("OCTACITY_RELEASE_UI_ROOT", true);
  let nginx_binary = required_path("OCTACITY_RELEASE_NGINX", true);
  fs::create_dir_all(&evidence).unwrap();

  let release = load_browser_release();
  let temporary = tempfile::tempdir().unwrap();
  let client = Client::new();
  let (management_addr, agent_addr) = unused_loopback_addresses();
  let proxy_addr = unused_loopback_address();
  let server_config = write_server_config(
    temporary.path(),
    &ServerConfigInput {
      postgres_url: &postgres_url,
      object_endpoint: &object_endpoint,
      cache_endpoint: "https://cache.example",
      toolchain: &browser_toolchain(),
      management_addr,
      agent_addr,
      cache_addr: None,
    },
  );
  let server_stdout = evidence.join("server.stdout.log");
  let server_stderr = evidence.join("server.stderr.log");
  let mut server = spawn_server(&release.server_binary, &server_config, &server_stdout, &server_stderr);
  let management_origin = format!("http://{management_addr}");
  wait_for_server_live(&client, &management_origin, &mut server, &server_stdout).await;

  let run = Uuid::new_v4().simple().to_string();
  let backend = browser_backend(temporary.path());
  let resources = create_pipeline_resources(&client, &management_origin, &run, &backend).await;
  let attempt = get_json(
    &client,
    format!("{management_origin}/api/v1/attempts/{}", resources.attempt_id),
  )
  .await;
  let job_id = string(
    attempt["jobs"]
      .as_array()
      .and_then(|jobs| jobs.first())
      .expect("browser fixture must materialize at least one Job"),
    "id",
  );
  let build = get_json(
    &client,
    format!("{management_origin}/api/v1/builds/{}", resources.build_id),
  )
  .await;
  let project_id = string(&build, "project_id");

  let nginx_config = write_nginx_config(
    temporary.path(),
    &release,
    management_addr,
    proxy_addr,
    &evidence.join("nginx.error.log"),
  );
  let nginx_stdout = evidence.join("nginx.stdout.log");
  let nginx_stderr = evidence.join("nginx.stderr.log");
  let mut nginx = spawn_nginx(&nginx_binary, &nginx_config, &nginx_stdout, &nginx_stderr);
  let console_origin = format!("http://{proxy_addr}");
  wait_for_proxy(&client, &console_origin, &mut nginx, &nginx_stderr).await;

  run_browser(
    &ui_root,
    &console_origin,
    &project_id,
    &resources.build_id,
    &job_id,
    &evidence,
  )
  .await;
  shutdown_process(&mut nginx, &nginx_stderr, "nginx").await;

  // The console is optional: after the static proxy is gone, a fresh headless
  // workflow still succeeds directly against the released management API.
  let headless_project = post_management(
    &client,
    &management_origin,
    "/api/v1/projects",
    &format!("{run}-headless-project"),
    json!({"parent_id": null, "name": format!("headless-{run}")}),
  )
  .await;
  let headless_project_id = resource_id(&headless_project);
  let observed = get_json(
    &client,
    format!("{management_origin}/api/v1/projects/{headless_project_id}"),
  )
  .await;
  assert_eq!(string(&observed["project"], "id"), headless_project_id);

  fs::write(
    evidence.join("released-console.json"),
    serde_json::to_vec_pretty(&json!({
      "server_release": release.server_manifest,
      "console_release": release.console_manifest,
      "project_id": project_id,
      "build_id": resources.build_id,
      "job_id": job_id,
      "headless_project_id": headless_project_id,
      "same_origin": console_origin,
    }))
    .unwrap(),
  )
  .unwrap();
  shutdown_server(&mut server, &server_stdout).await;
}

fn load_browser_release() -> InstalledBrowserRelease {
  let release = validate_installed_browser_release(&BrowserReleaseBundles {
    server: required_path("OCTACITY_RELEASE_SERVER_ROOT", true),
    console: required_path("OCTACITY_RELEASE_CONSOLE_ROOT", true),
  })
  .unwrap_or_else(|error| panic!("released browser installation failed validation: {error}"));
  assert_eq!(
    release.server_manifest.platform(),
    format!("linux-{}", host_architecture())
  );
  assert_eq!(release.console_manifest.platform(), "any");
  release
}

fn browser_toolchain() -> Toolchain {
  Toolchain {
    source_version: "0.1.0".to_owned(),
    source_digest: "1".repeat(64),
    octa_version: "0.4.0".to_owned(),
    runner_digest: "2".repeat(64),
    runner_protocol: octacity_runner::RUNNER_PROTOCOL_VERSION,
    event_schema: octacity_runner::RUNNER_EVENT_SCHEMA_VERSION,
    plugin_protocol: 1,
    plugin_digests: std::collections::BTreeMap::from([("shell".to_owned(), "3".repeat(64))]),
  }
}

fn browser_backend(root: &Path) -> ReleaseBackend {
  ReleaseBackend::Native {
    cgroup_root: root.join("unused-cgroups"),
    work_root: root.join("unused-work"),
    cache_root: root.join("unused-cache"),
    bubblewrap: root.join("unused-bwrap"),
    path: "/usr/bin:/bin".to_owned(),
    environment_identity: "released-console-native-v1".to_owned(),
    workspace_bytes: 1024 * 1024 * 1024,
  }
}

fn unused_loopback_address() -> SocketAddr {
  TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap()
}

async fn wait_for_server_live(client: &Client, origin: &str, server: &mut Child, log: &Path) {
  let deadline = Instant::now() + Duration::from_secs(60);
  loop {
    if let Some(status) = server.try_wait().unwrap() {
      panic!(
        "released server exited before liveness with {status}: {}",
        fs::read_to_string(log).unwrap_or_default()
      );
    }
    if client
      .get(format!("{origin}/health/live"))
      .send()
      .await
      .is_ok_and(|response| response.status().is_success())
    {
      return;
    }
    assert!(
      Instant::now() < deadline,
      "timed out waiting for released server liveness: {}",
      fs::read_to_string(log).unwrap_or_default()
    );
    sleep(Duration::from_millis(100)).await;
  }
}

fn write_nginx_config(
  directory: &Path,
  release: &InstalledBrowserRelease,
  management_addr: SocketAddr,
  proxy_addr: SocketAddr,
  error_log: &Path,
) -> PathBuf {
  let path = |value: &Path| nginx_string(value.to_str().expect("release paths must be UTF-8"));
  let config = format!(
    r#"pid {};
error_log {} notice;
events {{}}
http {{
    include /etc/nginx/mime.types;
    default_type application/octet-stream;
    include {};
    access_log off;
    server_tokens off;
    proxy_http_version 1.1;
    proxy_set_header Host $http_host;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Host $http_host;
    proxy_set_header X-Forwarded-Proto http;
    proxy_set_header Connection "";
    upstream octacity_management {{ server {management_addr}; }}
    server {{
        listen {proxy_addr};
        server_name _;
        root {};
        index index.html;
        include {};
        include {};
    }}
}}
"#,
    path(&directory.join("nginx.pid")),
    path(error_log),
    path(&release.console_nginx_root.join("cache-map.conf")),
    path(&release.console_root),
    path(&release.console_nginx_root.join("security-headers.conf")),
    path(&release.console_nginx_root.join("routes.conf")),
  );
  let output = directory.join("nginx.conf");
  fs::write(&output, config).unwrap();
  output
}

fn nginx_string(value: &str) -> String {
  format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn spawn_nginx(binary: &Path, config: &Path, stdout: &Path, stderr: &Path) -> Child {
  Command::new(binary)
    .arg("-c")
    .arg(config)
    .arg("-g")
    .arg("daemon off;")
    .stdout(Stdio::from(File::create(stdout).unwrap()))
    .stderr(Stdio::from(File::create(stderr).unwrap()))
    .kill_on_drop(true)
    .spawn()
    .unwrap()
}

async fn wait_for_proxy(client: &Client, origin: &str, nginx: &mut Child, log: &Path) {
  let deadline = Instant::now() + Duration::from_secs(30);
  loop {
    if let Some(status) = nginx.try_wait().unwrap() {
      panic!(
        "nginx exited before serving the released console with {status}: {}",
        fs::read_to_string(log).unwrap_or_default()
      );
    }
    if client
      .get(origin)
      .send()
      .await
      .is_ok_and(|response| response.status().is_success())
    {
      return;
    }
    assert!(
      Instant::now() < deadline,
      "timed out waiting for released console proxy: {}",
      fs::read_to_string(log).unwrap_or_default()
    );
    sleep(Duration::from_millis(100)).await;
  }
}

async fn run_browser(ui_root: &Path, origin: &str, project_id: &str, build_id: &str, job_id: &str, evidence: &Path) {
  let stdout = evidence.join("browser.stdout.log");
  let stderr = evidence.join("browser.stderr.log");
  let status = Command::new("pnpm")
    .arg("exec")
    .arg("playwright")
    .arg("test")
    .arg("--config")
    .arg("playwright.released.config.ts")
    .current_dir(ui_root)
    .env("OCTACITY_RELEASE_CONSOLE_ORIGIN", origin)
    .env("OCTACITY_RELEASE_PROJECT_ID", project_id)
    .env("OCTACITY_RELEASE_BUILD_ID", build_id)
    .env("OCTACITY_RELEASE_JOB_ID", job_id)
    .stdout(Stdio::from(File::create(&stdout).unwrap()))
    .stderr(Stdio::from(File::create(&stderr).unwrap()))
    .status()
    .await
    .unwrap();
  assert!(
    status.success(),
    "released browser slice failed: {}{}",
    fs::read_to_string(stdout).unwrap_or_default(),
    fs::read_to_string(stderr).unwrap_or_default()
  );
}

async fn shutdown_process(process: &mut Child, log: &Path, name: &str) {
  let process_id = process.id().unwrap_or_else(|| panic!("{name} must still be running"));
  let process_id = rustix::process::Pid::from_raw(process_id as i32).unwrap();
  rustix::process::kill_process(process_id, rustix::process::Signal::TERM).unwrap();
  let status = timeout(Duration::from_secs(30), process.wait())
    .await
    .unwrap_or_else(|_| panic!("{name} did not stop before its shutdown deadline"))
    .unwrap();
  assert!(
    status.success(),
    "{name} exited unsuccessfully: {}",
    fs::read_to_string(log).unwrap_or_default()
  );
}
