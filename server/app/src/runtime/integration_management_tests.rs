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
  let runtime = ServerRuntime::start(config.clone())
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
  // A read/check installation alone does not install automatic Source execution.
  let choices = format!("/api/v1/projects/{project_id}/factory-source-choices?connection_id={id}&connection_version=1");
  for source_path in [
    choices.clone(),
    format!("/api/v1/projects/{project_id}/factory-sources"),
    format!("/api/v1/projects/{project_id}/factory-source-inputs/{id}/versions/1/diagnostics"),
  ] {
    let unavailable = client
      .get(format!("http://localhost{source_path}"))
      .send()
      .await
      .unwrap();
    assert_eq!(
      unavailable.status(),
      reqwest::StatusCode::UNPROCESSABLE_ENTITY,
      "{source_path}"
    );
    assert_eq!(
      unavailable.json::<Value>().await.unwrap()["code"],
      "capability_unavailable"
    );
  }
  assert_eq!(
    projects
      .get(format!("http://{}{choices}", runtime.management_addr()))
      .send()
      .await
      .unwrap()
      .status(),
    reqwest::StatusCode::FORBIDDEN
  );
  let manual = projects
    .post(format!(
      "http://{}/api/v1/projects/{project_id}/issues",
      runtime.management_addr()
    ))
    .header("idempotency-key", "manual-without-source")
    .json(&json!({"title":"Local task","description":"","acceptance":null,"priority":null}))
    .send()
    .await
    .unwrap();
  assert_eq!(manual.status(), reqwest::StatusCode::CREATED);
  runtime.shutdown().await.unwrap();
  // Trusted deployment intent is retained even when the advertised profile file is absent.
  let mut declaration: Value = serde_json::from_slice(&fs::read(&catalog).unwrap()).unwrap();
  declaration["source_profiles_file"] = json!(root.path().join("missing-source-profiles.json"));
  fs::write(&catalog, serde_json::to_vec(&declaration).unwrap()).unwrap();
  let runtime = ServerRuntime::start(config.clone()).await.unwrap();
  let response = client.get(format!("http://localhost{choices}")).send().await.unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
  assert_eq!(response.json::<Value>().await.unwrap()["code"], "unavailable");
  let health = client.get("http://localhost/health/factory").send().await.unwrap();
  assert_eq!(health.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
  assert_eq!(health.json::<Value>().await.unwrap(), json!({"status":"unavailable"}));
  let manual = projects
    .post(format!(
      "http://{}/api/v1/projects/{project_id}/issues",
      runtime.management_addr()
    ))
    .header("idempotency-key", "manual-with-missing-advertised-source")
    .json(&json!({"title":"Another local task","description":"","acceptance":null,"priority":null}))
    .send()
    .await
    .unwrap();
  assert_eq!(manual.status(), reqwest::StatusCode::CREATED);
  runtime.shutdown().await.unwrap();
  // Installing Source support exposes choices for current grants, including Projects added later.
  let profiles = root.path().join("sources.json");
  fs::write(
    &profiles,
    serde_json::to_vec(&json!({"version":1,"readers":[{
      "identity":"task-drafts","version":"v1",
      "read":{"installation":check.installation,"resource_reference":check.resource_reference,
        "capability":check.capability,"plugin":check.plugin,"provider_configuration":check.provider_configuration,
        "resource":check.resource,"network_hosts":check.network_hosts},
      "selection":{"identity":"all-tasks","sha256":"b".repeat(64)},
      "readable":{"states":["native-state"],"labels":[]},
      "selection_labels":{"state":"all-native","all_of":[],"any_of":[],"none_of":[]},
      "predicate_states":["native-state"],
      "polling":{"page_items":2,"max_pages":1,"max_items":2,"max_bytes":65536,"max_page_bytes":65536,
        "overlap_millis":20,"reconciliation_interval_millis":1000,"retention_millis":60000}
    }]}))
    .unwrap(),
  )
  .unwrap();
  fs::set_permissions(&profiles, fs::Permissions::from_mode(0o600)).unwrap();
  declaration["source_profiles_file"] = json!(profiles);
  fs::write(&catalog, serde_json::to_vec(&declaration).unwrap()).unwrap();
  let runtime = ServerRuntime::start(config.clone()).await.unwrap();
  let response = client
    .post(format!("{path}/state"))
    .header("idempotency-key", "activate-source-connection")
    .header("if-match", "\"1\"")
    .json(
      &json!({"intent":{"id":id,"expected_metadata_version":1,"expected_revision":1,
      "action":{"operation":"activate"}}}),
    )
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::OK,
    "{}",
    response.text().await.unwrap()
  );
  let response = client.get(format!("http://localhost{choices}")).send().await.unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::OK,
    "valid installed Source must compose at normal startup"
  );
  let selected: Value = response.json().await.unwrap();
  assert_eq!(
    selected["items"].as_array().unwrap().len(),
    0,
    "read-only Source profiles without preparation cannot authorize automatic Factory inputs"
  );
  let task_choices_path = format!(
    "http://localhost/api/v1/projects/{project_id}/issue-source-choices?connection_id={id}&connection_version=1"
  );
  let response = client.get(&task_choices_path).send().await.unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::OK,
    "Project task-source choices require no Issue or Factory preparation"
  );
  let task_choices: Value = response.json().await.unwrap();
  assert_eq!(task_choices["items"].as_array().unwrap().len(), 1);
  assert_eq!(task_choices["items"][0]["purpose"], "task_read");
  assert_eq!(
    task_choices["items"][0]["connection_configuration"],
    json!(check.capability.configuration)
  );
  assert!(!task_choices.to_string().contains("reader"));
  super::source_management_tests::assert_project_preview_retry_bound(
    database.pool.clone(),
    project_id.parse().unwrap(),
    &task_choices,
    &profiles,
    &registry,
    &installed,
  )
  .await;
  let import_controls =
    super::source_management_tests::assert_project_import_controls(&client, project_id, &task_choices).await;
  super::source_management_tests::assert_project_preview(
    &client,
    project_id,
    &task_choices,
    &registry.join("custom-provider"),
  )
  .await;
  assert_eq!(
    projects
      .get(format!(
        "http://{}/api/v1/projects/{project_id}/issue-source-choices?connection_id={id}&connection_version=1",
        runtime.management_addr()
      ))
      .send()
      .await
      .unwrap()
      .status(),
    reqwest::StatusCode::FORBIDDEN
  );
  runtime.shutdown().await.unwrap();
  let mut source_profiles: Value = serde_json::from_slice(&fs::read(&profiles).unwrap()).unwrap();
  source_profiles["readers"][0]["preparation"] = json!({
    "mapping":{"source":{"installation":check.installation,"resource":check.resource_reference,
      "capability":check.capability.capability,"configuration":check.capability.configuration},
      "acceptance":{"kind":"body"},"require_body":true,"max_title_bytes":4096,"max_body_bytes":262144,
      "max_artifact_bytes":1048576,"retention_millis":60000},
    "source":{"kind":"default_reference"},"classification":{"priority":10,"risk":"medium","metadata":{}}
  });
  fs::write(&profiles, serde_json::to_vec(&source_profiles).unwrap()).unwrap();
  let runtime = ServerRuntime::start(config.clone()).await.unwrap();
  let response = client.get(format!("http://localhost{choices}")).send().await.unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  let selected: Value = response.json().await.unwrap();
  assert_eq!(selected["items"].as_array().unwrap().len(), 1);
  assert_eq!(selected["items"][0]["purpose"], "task_read");
  assert_ne!(
    selected["items"][0]["binding"]["configuration"],
    selected["items"][0]["connection_configuration"]
  );
  assert!(!selected.to_string().contains("reader"));
  let task_choices_after_preparation: Value = client
    .get(&task_choices_path)
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(
    task_choices_after_preparation["items"][0]["binding"], task_choices["items"][0]["binding"],
    "optional Factory preparation must not replace an independent task reader"
  );
  assert_ne!(
    task_choices_after_preparation["items"][0]["binding"],
    selected["items"][0]["binding"]
  );
  let published = super::source_management_tests::assert_input_publication(
    &client,
    database.pool.clone(),
    project_id.parse().unwrap(),
    &selected,
    &registry.join("custom-provider"),
  )
  .await;
  let response = projects
    .post(format!("http://{}/api/v1/projects", runtime.management_addr()))
    .header("idempotency-key", "late-project")
    .json(&json!({"name":"Created after installation"}))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::CREATED);
  let second: Value = response.json().await.unwrap();
  let second_id = second["resource"]["id"].as_str().unwrap();
  let second_choices = format!(
    "http://localhost/api/v1/projects/{second_id}/factory-source-choices?connection_id={id}&connection_version=1"
  );
  assert_eq!(
    client.get(&second_choices).send().await.unwrap().status(),
    reqwest::StatusCode::UNPROCESSABLE_ENTITY
  );
  let response = client
    .post(format!("{path}/projects/{second_id}/access"))
    .header("idempotency-key", "late-project-grant")
    .header("if-none-match", "*")
    .json(
      &json!({"intent":{"connection_id":id,"project_id":second_id,"connection_version":1,
      "expected_current_version":null,"resources":grant["resources"]}}),
    )
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::OK,
    "{}",
    response.text().await.unwrap()
  );
  let response = client.get(&second_choices).send().await.unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  let late: Value = response.json().await.unwrap();
  assert_eq!(late["items"].as_array().unwrap().len(), 1);
  let task_choices_second: Value = client
    .get(format!(
      "http://localhost/api/v1/projects/{second_id}/issue-source-choices?connection_id={id}&connection_version=1"
    ))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_ne!(
    task_choices_second["items"][0]["binding"], task_choices_after_preparation["items"][0]["binding"],
    "a task read choice is scoped to its original Project"
  );
  super::source_management_tests::assert_project_preview(
    &client,
    second_id,
    &task_choices_second,
    &registry.join("custom-provider"),
  )
  .await;
  super::source_management_tests::assert_project_preview_rejects_foreign_scope(
    &client,
    second_id,
    &task_choices_after_preparation,
  )
  .await;
  let history: Value = client
    .get(format!("http://localhost/api/v1/projects/{second_id}/factory-sources"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(history, json!({"items":[],"next_cursor":null}));
  let hidden = client
    .get(format!(
      "http://localhost/api/v1/projects/{second_id}/factory-source-inputs/{}/versions/1",
      published["input"].as_str().unwrap()
    ))
    .send()
    .await
    .unwrap();
  assert_eq!(hidden.status(), reqwest::StatusCode::NOT_FOUND);
  let hidden = client
    .get(format!(
      "http://localhost/api/v1/projects/{second_id}/issue-source-imports/{}",
      import_controls["id"].as_str().unwrap()
    ))
    .send()
    .await
    .unwrap();
  assert_eq!(hidden.status(), reqwest::StatusCode::NOT_FOUND);
  // Project grants are current authority, while a retained input remains immutable history.
  let revoke = json!({"connection_id":id,"project_id":project_id,"connection_version":1,
    "expected_current_version":1,"resources":[]});
  let response = client
    .post(format!("{project_path}/access"))
    .header("idempotency-key", "revoke-source-authoring")
    .header("if-match", "\"1\"")
    .json(&json!({"intent":revoke}))
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::OK,
    "{}",
    response.text().await.unwrap()
  );
  let revoked: Value = client
    .get(format!("http://localhost{choices}"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(revoked["items"], json!([]));
  super::source_management_tests::assert_import_disable_after_revocation(&client, project_id, &import_controls).await;
  let response = client
    .post(format!(
      "http://localhost/api/v1/projects/{project_id}/issue-sources/preview"
    ))
    .json(&json!({"connection_id":id,"connection_version":1,"access_version":1,
      "source":task_choices_after_preparation["items"][0]["binding"],"limit":2}))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::PRECONDITION_FAILED);
  let response = client
    .post(format!(
      "http://localhost/api/v1/projects/{project_id}/factory-sources/validate"
    ))
    .json(&json!({"intent":published["intent"]}))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::PRECONDITION_FAILED);
  let response = client
    .post(format!("{project_path}/access"))
    .header("idempotency-key", "restore-source-authoring")
    .header("if-match", "\"2\"")
    .json(
      &json!({"intent":{"connection_id":id,"project_id":project_id,"connection_version":1,
      "expected_current_version":2,"resources":grant["resources"]}}),
    )
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    reqwest::StatusCode::OK,
    "{}",
    response.text().await.unwrap()
  );
  runtime.shutdown().await.unwrap();
  source_profiles["readers"][0]["polling"]["max_pages"] = json!(2);
  fs::write(&profiles, serde_json::to_vec(&source_profiles).unwrap()).unwrap();
  let runtime = ServerRuntime::start(config).await.unwrap();
  let replaced: Value = client
    .get(format!("http://localhost{choices}"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_ne!(replaced["items"][0]["binding"], selected["items"][0]["binding"]);
  assert_eq!(
    replaced["items"][0]["connection_configuration"],
    selected["items"][0]["connection_configuration"]
  );
  super::source_management_tests::assert_history_after_replacement(&client, project_id.parse().unwrap(), &published)
    .await;
  runtime.shutdown().await.unwrap();
  database.cleanup().await;
}
