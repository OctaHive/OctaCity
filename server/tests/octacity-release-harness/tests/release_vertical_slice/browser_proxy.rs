//! Shared packaged same-origin nginx and browser process boundary.
use super::*;
use octacity_release_harness::InstalledBrowserRelease;

pub(super) fn write_nginx_config_with_upstream(
  directory: &Path,
  release: &InstalledBrowserRelease,
  management_upstream: &str,
  proxy_addr: SocketAddr,
  error_log: &Path,
) -> PathBuf {
  let path = |value: &Path| nginx_string(value.to_str().expect("release paths must be UTF-8"));
  let config = format!(
    r#"pid {};
error_log {} notice;
events {{}}
http {{
    include {};
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
    upstream octacity_management {{ server {management_upstream}; }}
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
    path(
      &env::var_os("OCTACITY_RELEASE_NGINX_MIME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/nginx/mime.types"))
    ),
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

pub(super) fn spawn_nginx(binary: &Path, config: &Path, stdout: &Path, stderr: &Path) -> Child {
  Command::new(binary)
    .arg("-c")
    .arg(config)
    .arg("-g")
    // Keep the disposable gateway in the owned foreground process. Killing a
    // master on failed qualification otherwise leaves its worker serving forever.
    .arg("daemon off; master_process off;")
    .stdout(Stdio::from(File::create(stdout).unwrap()))
    .stderr(Stdio::from(File::create(stderr).unwrap()))
    .kill_on_drop(true)
    .spawn()
    .unwrap()
}

pub(super) async fn wait_for_proxy(client: &Client, origin: &str, nginx: &mut Child, log: &Path) {
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

/// Run only the requested journey, keeping traces/logs with its packaged evidence.
pub(super) async fn run_browser_scenario(
  ui_root: &Path,
  origin: &str,
  scenario: &str,
  environment: &[(&str, &str)],
  evidence: &Path,
) -> Result<(), String> {
  let stdout = evidence.join("browser.stdout.log");
  let stderr = evidence.join("browser.stderr.log");
  let status = Command::new("pnpm")
    .args([
      "exec",
      "playwright",
      "test",
      "--config",
      "playwright.released.config.ts",
      scenario,
      "--output",
    ])
    .arg(evidence.join("browser-results"))
    .current_dir(ui_root)
    .env("OCTACITY_RELEASE_CONSOLE_ORIGIN", origin)
    .envs(environment.iter().copied())
    .stdout(Stdio::from(File::create(&stdout).unwrap()))
    .stderr(Stdio::from(File::create(&stderr).unwrap()))
    .status()
    .await
    .unwrap();
  if status.success() {
    Ok(())
  } else {
    Err(format!(
      "released browser scenario {scenario} failed: {}{}",
      fs::read_to_string(stdout).unwrap_or_default(),
      fs::read_to_string(stderr).unwrap_or_default()
    ))
  }
}

pub(super) async fn shutdown_process(process: &mut Child, log: &Path, name: &str) {
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
