//! Real packaged Console authoring, with no browser HTTP interception or saved Flow.
use super::*;

pub(super) async fn author(
  client: &Client,
  management: &str,
  project: &str,
  directory: &Path,
  locale: &str,
  code: &code::CodeInstallation,
) -> String {
  let release =
    octacity_release_harness::validate_installed_browser_release(&octacity_release_harness::BrowserReleaseBundles {
      server: required_path("OCTACITY_RELEASE_SERVER_ROOT", true),
      console: required_path("OCTACITY_RELEASE_CONSOLE_ROOT", true),
    })
    .unwrap();
  assert_eq!(
    release.server_binary.canonicalize().unwrap(),
    required_path("OCTACITY_MANUAL_SERVER_BINARY", true)
      .canonicalize()
      .unwrap()
  );
  let address = unused_loopback_addresses().0;
  let config = browser_proxy::write_nginx_config_with_upstream(
    directory,
    &release,
    management.strip_prefix("http://").unwrap(),
    address,
    &directory.join("nginx.error.log"),
  );
  let mut nginx = browser_proxy::spawn_nginx(
    &required_path("OCTACITY_RELEASE_NGINX", true),
    &config,
    &directory.join("nginx.stdout.log"),
    &directory.join("nginx.stderr.log"),
  );
  let origin = format!("http://{address}");
  browser_proxy::wait_for_proxy(client, &origin, &mut nginx, &directory.join("nginx.stderr.log")).await;
  browser_proxy::run_browser_scenario(
    &required_path("OCTACITY_RELEASE_UI_ROOT", true),
    &origin,
    "factory-manual-authoring.spec.ts",
    &[
      ("OCTACITY_RELEASE_PROJECT_ID", project),
      ("OCTACITY_RELEASE_AUTHORING_LOCALE", locale),
      ("OCTACITY_RELEASE_CODE_REVISION", &code.revision),
      ("OCTACITY_RELEASE_CODE_REPOSITORY", &code.repository),
    ],
    directory,
  )
  .await
  .unwrap();
  browser_proxy::shutdown_process(&mut nginx, &directory.join("nginx.stderr.log"), "nginx").await;
  let runs = get_json(
    client,
    format!("{management}/api/v1/projects/{project}/factory-runs?limit=100"),
  )
  .await;
  assert_eq!(runs["items"].as_array().unwrap().len(), 1);
  string(&runs["items"][0], "id")
}
