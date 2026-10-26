//! Offline installation contracts through the public validator; no provider or database access.
use super::assembly::validate_runtime_files;
use std::{os::unix::fs::PermissionsExt as _, path::Path};

fn source_profile_config(
  root: &Path,
  document: &serde_json::Value,
  installations: &[octacity_server_store::InstalledIntegration],
) -> (std::path::PathBuf, crate::ServerConfig) {
  let profiles = root.join("source-profiles.json");
  let catalog = root.join("integrations.json");
  let registry = root.join("adapters");
  std::fs::create_dir_all(&registry).unwrap();
  std::fs::write(&profiles, serde_json::to_vec(document).unwrap()).unwrap();
  std::fs::write(
    &catalog,
    serde_json::to_vec(&serde_json::json!({
      "version":1,"registry":registry,"installations":installations,
      "source_profiles_file":profiles
    }))
    .unwrap(),
  )
  .unwrap();
  for path in [&profiles, &catalog] {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
  }
  let config = crate::ServerConfig::parse_toml(&crate::runtime::test_runtime_config::runtime_config_source(
    root,
    "postgres://fixture:fixture@127.0.0.1:9/postgres",
    "http://127.0.0.1:9",
    &format!(
      "[integration_management]\nsocket_path={:?}\npeer_uid={}\nprincipal='test-proxy'\ncatalog_file={:?}\n",
      root.join("management.sock").to_str().unwrap(),
      rustix::process::geteuid().as_raw(),
      catalog.to_str().unwrap()
    ),
  ))
  .unwrap();
  (profiles, config)
}

#[tokio::test]
async fn offline_validation_checks_source_profile_versions_without_provider_or_database_access() {
  let root = tempfile::tempdir().unwrap();
  let (profiles, config) = source_profile_config(root.path(), &serde_json::json!({"version":1}), &[]);
  assert!(validate_runtime_files(&config).await.is_ok());
  std::fs::write(&profiles, r#"{"version":99}"#).unwrap();
  assert!(
    validate_runtime_files(&config).await.is_err(),
    "unsupported installed Source profile versions must fail offline validation"
  );
}

fn installed_source_profile_fixture() -> (
  tempfile::TempDir,
  octacity_server_store::InstalledIntegration,
  serde_json::Value,
) {
  let (root, connection, read) = crate::runtime::integration_check_tests::fixture("opaque-source");
  let registry = root.path().join("adapters");
  std::fs::create_dir(&registry).unwrap();
  std::fs::rename(root.path().join("opaque-source"), registry.join("opaque-source")).unwrap();
  let definition = connection.definition;
  let installed = octacity_server_store::InstalledIntegration {
    installation: definition.installation,
    provider: definition.provider,
    adapter: definition.adapter,
    endpoints: vec![definition.endpoint],
    authentication: vec![definition.authentication],
    resources: definition.resources,
  };
  let document = serde_json::json!({"version":1,"readers":[{
    "identity":"read-tasks","version":"v1",
    "read":{"installation":read.installation,"resource_reference":read.resource_reference,
      "capability":read.capability,"plugin":read.plugin,"provider_configuration":read.provider_configuration,
      "resource":read.resource,"network_hosts":read.network_hosts},
    "selection":{"identity":"all-tasks","sha256":"b".repeat(64)},
    "readable":{"states":["native-state"],"labels":[]},
    "polling":{"page_items":2,"max_pages":1,"max_items":2,"max_bytes":65536,"max_page_bytes":65536,
      "overlap_millis":20,"reconciliation_interval_millis":1000,"retention_millis":60000}
  }]});
  (root, installed, document)
}

#[tokio::test]
async fn offline_source_profiles_accept_read_only_installation_and_reject_unbounded_polling() {
  let (root, installed, mut document) = installed_source_profile_fixture();
  let (profiles, config) = source_profile_config(root.path(), &document, &[installed]);
  assert!(
    validate_runtime_files(&config).await.is_ok(),
    "bounded native reads must require neither a Factory target, preparation nor Reporter"
  );
  document["readers"][0]["polling"]["max_pages"] = serde_json::json!(0);
  std::fs::write(profiles, serde_json::to_vec(&document).unwrap()).unwrap();
  assert!(
    validate_runtime_files(&config).await.is_err(),
    "zero polling bounds must fail before dispatch"
  );
  assert!(!root.path().join("adapters/opaque-source/captured.json").exists());
}

#[tokio::test]
async fn offline_source_profiles_reject_foreign_installation_resource_plugin_and_configuration_pins() {
  let (root, installed, document) = installed_source_profile_fixture();
  let (profiles, config) = source_profile_config(root.path(), &document, &[installed]);
  assert!(validate_runtime_files(&config).await.is_ok());
  for pointer in [
    "/readers/0/read/installation/digest",
    "/readers/0/read/resource_reference/digest",
    "/readers/0/read/plugin/digest",
    "/readers/0/read/capability/configuration/digest",
    "/readers/0/read/provider_configuration",
  ] {
    let mut foreign = document.clone();
    *foreign.pointer_mut(pointer).unwrap() = serde_json::json!("f".repeat(64));
    std::fs::write(&profiles, serde_json::to_vec(&foreign).unwrap()).unwrap();
    assert!(
      validate_runtime_files(&config).await.is_err(),
      "foreign pin {pointer} must fail offline"
    );
  }
  assert!(!root.path().join("adapters/opaque-source/captured.json").exists());
}

#[tokio::test]
async fn offline_source_profiles_reject_ambiguous_reader_and_installation_declarations() {
  for duplicate in ["reader", "installation"] {
    let (root, installed, mut document) = installed_source_profile_fixture();
    let installations = if duplicate == "reader" {
      let reader = document["readers"][0].clone();
      document["readers"].as_array_mut().unwrap().push(reader);
      vec![installed]
    } else {
      vec![installed.clone(), installed]
    };
    let (_, config) = source_profile_config(root.path(), &document, &installations);
    assert!(
      validate_runtime_files(&config).await.is_err(),
      "ambiguous {duplicate} must not select the first match"
    );
  }
}

fn factory_source_profile_fixture() -> (
  tempfile::TempDir,
  octacity_server_store::InstalledIntegration,
  serde_json::Value,
) {
  let (root, installed, mut document) = installed_source_profile_fixture();
  let read = &document["readers"][0]["read"];
  let preparation = serde_json::json!({
    "mapping":{
      "source":{"installation":read["installation"],"capability":read["capability"]["capability"],
        "configuration":read["capability"]["configuration"],"resource":read["resource_reference"]},
      "acceptance":{"kind":"body_section","value":"## Acceptance"},"require_body":true,
      "max_title_bytes":4096,"max_body_bytes":262144,"max_artifact_bytes":1048576,"retention_millis":60000
    },
    "source":{"kind":"default_reference"},
    "classification":{"priority":10,"risk":"medium","metadata":{}}
  });
  document["readers"][0]["preparation"] = preparation;
  (root, installed, document)
}

#[tokio::test]
async fn offline_factory_preparation_requires_explicit_mapping_bound_to_its_reader() {
  let (root, installed, mut document) = factory_source_profile_fixture();
  let (profiles, config) = source_profile_config(root.path(), &document, &[installed]);
  assert!(validate_runtime_files(&config).await.is_ok());
  let mut incomplete = document.clone();
  incomplete["readers"][0]["preparation"]
    .as_object_mut()
    .unwrap()
    .remove("mapping");
  std::fs::write(&profiles, serde_json::to_vec(&incomplete).unwrap()).unwrap();
  assert!(
    validate_runtime_files(&config).await.is_err(),
    "Factory preparation must not invent a mapping"
  );
  document["readers"][0]["preparation"]["mapping"]["source"]["configuration"]["digest"] =
    serde_json::json!("f".repeat(64));
  std::fs::write(profiles, serde_json::to_vec(&document).unwrap()).unwrap();
  assert!(
    validate_runtime_files(&config).await.is_err(),
    "preparation cannot borrow another reader binding"
  );
}

#[tokio::test]
async fn offline_factory_preparation_rejects_invalid_mapping_limits_and_acceptance_sources() {
  let (root, installed, document) = factory_source_profile_fixture();
  let (profiles, config) = source_profile_config(root.path(), &document, &[installed]);
  assert!(validate_runtime_files(&config).await.is_ok());
  for (field, value) in [
    ("max_title_bytes", serde_json::json!(0)),
    ("max_body_bytes", serde_json::json!(0)),
    ("max_artifact_bytes", serde_json::json!(0)),
    ("retention_millis", serde_json::json!(0)),
    ("acceptance", serde_json::json!({"kind":"field","value":""})),
    (
      "acceptance",
      serde_json::json!({"kind":"body_section","value":"Missing heading marker"}),
    ),
  ] {
    let mut invalid = document.clone();
    invalid["readers"][0]["preparation"]["mapping"][field] = value;
    std::fs::write(&profiles, serde_json::to_vec(&invalid).unwrap()).unwrap();
    assert!(
      validate_runtime_files(&config).await.is_err(),
      "invalid preparation {field} must fail before dispatch"
    );
  }
}

#[tokio::test]
async fn offline_predicate_editing_requires_an_explicit_unrestricted_base_and_unique_states() {
  let (root, installed, mut document) = installed_source_profile_fixture();
  document["readers"][0]["selection_labels"] = serde_json::json!({
    "state":"unrestricted-native","all_of":[],"any_of":[],"none_of":[]
  });
  document["readers"][0]["predicate_states"] = serde_json::json!(["native-open", "native-closed"]);
  let (profiles, config) = source_profile_config(root.path(), &document, &[installed]);
  assert!(validate_runtime_files(&config).await.is_ok());
  for (pointer, value) in [
    ("/readers/0/selection_labels", serde_json::Value::Null),
    ("/readers/0/selection_labels/state", serde_json::Value::Null),
    ("/readers/0/selection_labels/all_of", serde_json::json!(["fixed-label"])),
    (
      "/readers/0/predicate_states",
      serde_json::json!(["native-open", "native-open"]),
    ),
  ] {
    let mut invalid = document.clone();
    *invalid.pointer_mut(pointer).unwrap() = value;
    std::fs::write(&profiles, serde_json::to_vec(&invalid).unwrap()).unwrap();
    assert!(
      validate_runtime_files(&config).await.is_err(),
      "invalid editing authority {pointer}"
    );
  }
}

#[tokio::test]
async fn offline_optional_reporter_requires_its_own_write_class_and_original_delivery_bounds() {
  let (root, mut installed, reader_document) = installed_source_profile_fixture();
  let manifest = root.path().join("adapters/opaque-source/adapter.toml");
  let mut source = std::fs::read_to_string(&manifest).unwrap();
  source.push_str(&format!(
    "\n[protocol.reporter]\nmin=2\nmax=2\n[capabilities.reporter]\nobserve=true\nproject=true\ncancel=true\ncustom_effects=true\nidempotency='observe_before_retry'\ncredential_class='work_reporter_write'\nconfiguration_digest='{}'\n",
    "e".repeat(64)
  ));
  std::fs::write(manifest, source).unwrap();
  installed.resources[0].capabilities[0].purpose = octacity_server_store::IntegrationPurpose::TaskUpdate;
  let mut write = reader_document["readers"][0]["read"].clone();
  write["capability"]["purpose"] = serde_json::json!("task_update");
  write["resource"]["credential"]["class"] = serde_json::json!("work_reporter_write");
  let document = serde_json::json!({"version":1,"reporters":[{
    "identity":"write-effects","version":"v1","write":write,
    "mapping":{"identity":"native-effects","sha256":"c".repeat(64)},"credential_profile":"writer",
    "reporting":{"mapping":{"statuses":[{"status":"reviewed-native","add":["reviewed"],"remove":[],"state":null}]},
      "editable":{"states":["open","closed"],"labels":["reviewed"]}},
    "issue_delivery":{"retry":{"max_attempts":3,"retry_delay_millis":100},"window_millis":30000,
      "budget":{"max_attempts":1,"max_elapsed_millis":5000,"max_tokens":0,"max_cost_micro_units":0,"max_output_bytes":10000}}
  }]});
  let (profiles, config) = source_profile_config(root.path(), &document, &[installed]);
  assert!(validate_runtime_files(&config).await.is_ok());
  for (pointer, value) in [
    (
      "/reporters/0/write/resource/credential/class",
      serde_json::json!("work_source_read"),
    ),
    ("/reporters/0/issue_delivery/window_millis", serde_json::json!(0)),
  ] {
    let mut invalid = document.clone();
    *invalid.pointer_mut(pointer).unwrap() = value;
    std::fs::write(&profiles, serde_json::to_vec(&invalid).unwrap()).unwrap();
    assert!(
      validate_runtime_files(&config).await.is_err(),
      "invalid Reporter authority {pointer}"
    );
  }
  assert!(!root.path().join("adapters/opaque-source/captured.json").exists());
}

#[tokio::test]
async fn offline_source_profiles_reject_repeated_network_authority() {
  let (root, installed, mut document) = installed_source_profile_fixture();
  let (profiles, config) = source_profile_config(root.path(), &document, &[installed]);
  assert!(validate_runtime_files(&config).await.is_ok());
  let host = document["readers"][0]["read"]["network_hosts"][0].clone();
  document["readers"][0]["read"]["network_hosts"]
    .as_array_mut()
    .unwrap()
    .push(host);
  std::fs::write(profiles, serde_json::to_vec(&document).unwrap()).unwrap();
  assert!(
    validate_runtime_files(&config).await.is_err(),
    "ambiguous network ceilings must fail before any dispatch"
  );
}
