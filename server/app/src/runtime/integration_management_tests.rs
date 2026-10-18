//! Real released composition, disposable database and write-only private publisher.
use super::*;
use serde_json::{Value, json};
use std::{
  fs,
  os::unix::fs::{MetadataExt, PermissionsExt},
};

use super::postgres_test_support as postgres;

#[tokio::test]
#[ignore = "requires OCTACITY_POSTGRES_URL pointing to disposable PostgreSQL"]
async fn runtime_publishes_connection_credentials_and_project_access_without_enabling_inputs() {
  let database = postgres::TestDatabase::migrated().await;
  let mut url = url::Url::parse(&std::env::var("OCTACITY_POSTGRES_URL").unwrap()).unwrap();
  url.set_path(&database.name);
  let (root, connection, mut check) = super::integration_check_tests::fixture("custom-provider");
  check.resource.resource_locator = "owner/project".into();
  check.resource.credential.handle = "reader".into();
  fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
  let registry = root.path().join("adapters");
  fs::create_dir(&registry).unwrap();
  fs::rename(root.path().join("custom-provider"), registry.join("custom-provider")).unwrap();
  let private = root.path().join("private");
  fs::create_dir(&private).unwrap();
  fs::set_permissions(&private, fs::Permissions::from_mode(0o700)).unwrap();
  let definition = connection.definition;
  let installed = octacity_server_store::InstalledIntegration {
    installation: definition.installation.clone(),
    provider: definition.provider,
    adapter: definition.adapter,
    endpoints: vec![definition.endpoint.clone()],
    authentication: vec![definition.authentication.clone()],
    resources: definition.resources.clone(),
  };
  let catalog = root.path().join("integrations.json");
  fs::write(&catalog, serde_json::to_vec(&json!({
    "version":1, "registry":registry, "installations":[installed],
    "checks":[{"installation":check.installation,"resource_reference":check.resource_reference,
      "capability":check.capability,"plugin":check.plugin,"provider_configuration":check.provider_configuration,
      "resource":check.resource,"network_hosts":check.network_hosts}],
    "credential_publishers":[{"kind":"github_private_files", "root":root.path(), "directory":"private", "targets":[{
      "reference": definition.resources[0].capabilities[0].credential, "handle":"reader", "purpose":"task_read", "resources":["owner/project"]
    }]}]
  })).unwrap()).unwrap();
  fs::set_permissions(&catalog, fs::Permissions::from_mode(0o600)).unwrap();
  let socket = root.path().join("management.sock");
  let extra = format!(
    "[integration_management]\nsocket_path={:?}\npeer_uid={}\nprincipal='test-proxy'\ncatalog_file={:?}\n",
    socket.to_str().unwrap(),
    fs::metadata(root.path()).unwrap().uid(),
    catalog.to_str().unwrap()
  );
  let config = crate::ServerConfig::parse_toml(&super::test_runtime_config::runtime_config_source(
    root.path(),
    url.as_str(),
    "http://127.0.0.1:9",
    &extra,
  ))
  .unwrap();
  let runtime = ServerRuntime::start(config)
    .await
    .expect("installed publishers must compose in the released runtime");
  let client = reqwest::Client::builder()
    .unix_socket(socket.as_path())
    .build()
    .unwrap();
  let id = uuid::Uuid::new_v4().to_string();
  let path = format!("http://localhost/api/v1/integrations/{id}");
  let intent = json!({"id":id, "expected_current_version":null, "draft":{
    "name":"Private integration", "installation":definition.installation, "endpoint":definition.endpoint,
    "authentication":definition.authentication, "resources":[{"reference":definition.resources[0].reference,"purposes":["task_read"]}]
  }});
  let response = client
    .post(format!("{path}/versions"))
    .header("idempotency-key", "create-connection")
    .header("if-none-match", "*")
    .json(&json!({"intent":intent}))
    .send()
    .await
    .unwrap();
  assert!(response.status().is_success(), "{}", response.text().await.unwrap());
  let body = json!({"intent":{"id":id,"expected_metadata_version":1,"expected_revision":0,
    "action":{"operation":"credential","reference":definition.resources[0].capabilities[0].credential,"kind":"provision","activate_connection":false}},
    "value":"runtime-private-token-canary"});
  let response = client
    .post(format!("{path}/credentials"))
    .header("idempotency-key", "provision-reader")
    .header("if-none-match", "*")
    .json(&body)
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::OK,
    "{}",
    response.text().await.unwrap()
  );
  assert!(fs::read_dir(&private).unwrap().any(|entry| {
    let path = entry.unwrap().path();
    path.is_file()
      && fs::read(&path)
        .unwrap()
        .windows(b"runtime-private-token-canary".len())
        .any(|s| s == b"runtime-private-token-canary")
      && fs::metadata(path).unwrap().mode() & 0o777 == 0o600
  }));
  let state: Value = client
    .get(format!("{path}/state"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(state["enabled"], false);
  assert!(!state.to_string().contains("runtime-private-token-canary"));
  let connections: Value = client
    .get("http://localhost/api/v1/integrations")
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(connections["items"].as_array().unwrap().len(), 1);
  assert!(!connections.to_string().contains("runtime-private-token-canary"));
  let response = client
    .post(format!("{path}/checks"))
    .json(&json!({
      "version":1,"resource":definition.resources[0].reference,"purpose":"task_read"
    }))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  let checked: Value = response.json().await.unwrap();
  assert_eq!(checked["readiness"]["authentication"], "ready");
  assert_eq!(checked["readiness"]["resource_access"], "ready");
  assert_eq!(checked["readiness"]["capability"], "ready");
  let projects = reqwest::Client::new();
  let response = projects
    .post(format!("http://{}/api/v1/projects", runtime.management_addr()))
    .header("idempotency-key", "integration-access-project")
    .json(&json!({"name":"Manual tasks"}))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::CREATED);
  let project: Value = response.json().await.unwrap();
  let project_id = project["resource"]["id"].as_str().unwrap();
  let project_path = format!("{path}/projects/{project_id}");
  let response = client.get(format!("{project_path}/dependencies")).send().await.unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::OK,
    "{}",
    response.text().await.unwrap()
  );
  assert_eq!(
    response.json::<Value>().await.unwrap(),
    json!({"items":[],"next_cursor":null})
  );
  let grant = json!({"connection_id":id,"project_id":project_id,"connection_version":1,
    "expected_current_version":null,"resources":[{"reference":definition.resources[0].reference,"purposes":["task_read"]}]});
  let response = client
    .post(format!("{project_path}/access"))
    .header("idempotency-key", "grant-project-reading")
    .header("if-none-match", "*")
    .json(&json!({"intent":grant}))
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::OK,
    "{}",
    response.text().await.unwrap()
  );
  let access: Value = client
    .get(format!("{project_path}/access"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(access["access"]["project_id"], project_id);
  assert_eq!(access["access"]["resources"], grant["resources"]);
  runtime.shutdown().await.unwrap();
  database.cleanup().await;
}
