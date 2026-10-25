//! Plugin discovery is hosted by the existing verified bounded adapter process.
use octacity_server_factory::*;
use serde_json::json;
#[tokio::test]
async fn installed_plugin_owns_model_names_and_parameter_shapes() {
  let (_directory, program, reference) = plugin_fixture();
  let source = super::factory_model_catalog::PluginModelDiscovery::load(
    &program,
    &reference,
    &reference.digest().to_string(),
    Default::default(),
  )
  .unwrap();
  let (catalog, parameters) = source.discover().await.unwrap();
  assert_eq!(catalog.models[0].identifier, "Vendor/future:preview");
  assert!(catalog.manual_model);
  assert!(FlowPayload::new(&parameters, json!({"deliberation":"deep"})).is_ok());
  assert!(FlowPayload::new(&parameters, json!({"deliberation":"medium"})).is_err());
  assert!(FlowPayload::new(&catalog.models[0].parameters, json!({"deliberation":3})).is_ok());
  std::fs::write(&program, "substituted").unwrap();
  assert!(source.discover().await.is_err());
}

fn plugin_fixture() -> (tempfile::TempDir, std::path::PathBuf, ImmutableReference) {
  let payload = json!({"available":true,"manual_model":true,"parameters":{"type":"object","additionalProperties":false,"properties":{"deliberation":{"type":"string","enum":["quick","deep"]}}},"models":[{"id":"Vendor/future:preview","label":"Future preview","parameters":{"type":"object","additionalProperties":false,"properties":{"deliberation":{"type":"integer","minimum":1,"maximum":8}}}}]});
  plugin_fixture_with_payload(payload)
}

fn plugin_fixture_with_payload(
  payload: serde_json::Value,
) -> (tempfile::TempDir, std::path::PathBuf, ImmutableReference) {
  plugin_fixture_with_script(payload, "")
}

fn plugin_fixture_with_script(
  payload: serde_json::Value,
  before_reply: &str,
) -> (tempfile::TempDir, std::path::PathBuf, ImmutableReference) {
  use sha2::Digest;
  use std::os::unix::fs::PermissionsExt;
  let directory = tempfile::tempdir().unwrap();
  let program = directory.path().join("plugin");
  let body = format!(
    "#!/bin/sh\n[ \"$1\" = '--describe-models' ] || exit 64\nread request\n{before_reply}\nprintf '%s\\n' '{}'\nif IFS= read -r extra; then exit 65; fi\n",
    payload
  );
  std::fs::write(&program, body).unwrap();
  std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
  let sha = hex::encode(sha2::Sha256::digest(std::fs::read(&program).unwrap()));
  let reference = ImmutableReference::new(
    FactoryKey::new("plugin").unwrap(),
    FactoryKey::new("v1").unwrap(),
    FactoryDigest::from_lower_hex(&sha).unwrap(),
  );
  (directory, program, reference)
}

#[tokio::test]
async fn file_runtime_model_discovery_is_cached_and_refresh_never_rewrites_profiles() {
  use super::FactoryRuntimeInstaller;
  let (_directory, program, reference) = plugin_fixture();
  let (document, project, definition, alias) = runtime_catalog_document(&program, &reference);
  let installer =
    super::factory_profile_file::FileProfileInstaller::parse_json(&serde_json::to_vec(&document).unwrap(), &[])
      .expect("installed plugins attach discovery to the existing capability");
  let installed = installer.install(super::factory_tests::resources()).await.unwrap();
  let before = installed.capabilities.availability(project).await.unwrap();
  let octacity_server_store::FactoryConfigurationAvailability::Available(choices) = &before else {
    panic!("missing choices")
  };
  let cap = &choices.role_profile_capabilities()[&alias];
  assert_eq!(cap.ceiling, definition);
  assert_eq!(
    cap.model_catalog.as_ref().unwrap().models[0].identifier,
    "Vendor/future:preview"
  );
  std::fs::remove_file(program).unwrap();
  assert_eq!(
    installed.capabilities.availability(project).await.unwrap(),
    before,
    "cached reads perform no discovery"
  );
  assert!(
    installed
      .capabilities
      .refresh_role_models(project, &alias)
      .await
      .is_err()
  );
  assert_eq!(installed.capabilities.availability(project).await.unwrap(), before);
}

#[tokio::test]
async fn plugin_parameter_constraints_are_never_silently_broadened() {
  let payload = json!({"available":true,"manual_model":true,"models":[],"parameters":{
    "type":"object","additionalProperties":false,"properties":{"thinking":{"type":"string","enum":["deep",7]}}
  }});
  let (_directory, program, reference) = plugin_fixture_with_payload(payload);
  let source = super::factory_model_catalog::PluginModelDiscovery::load(
    &program,
    &reference,
    &reference.digest().to_string(),
    Default::default(),
  )
  .unwrap();
  assert!(
    source.discover().await.is_err(),
    "a mixed enum cannot override the plugin's string constraint"
  );
}

#[tokio::test]
async fn plugin_unicode_minimum_length_is_not_weakened_to_a_byte_minimum() {
  let payload = json!({"available":true,"manual_model":true,"models":[],"parameters":{
    "type":"object","additionalProperties":false,"properties":{"name":{"type":"string","minLength":2,"maxLength":10}}
  }});
  let (_directory, program, reference) = plugin_fixture_with_payload(payload);
  let source = super::factory_model_catalog::PluginModelDiscovery::load(
    &program,
    &reference,
    &reference.digest().to_string(),
    Default::default(),
  )
  .unwrap();
  assert!(
    source.discover().await.is_err(),
    "the common byte grammar cannot enforce a Unicode character minimum"
  );
}

fn runtime_catalog_document(
  program: &std::path::Path,
  reference: &ImmutableReference,
) -> (
  serde_json::Value,
  octacity_server_domain::ProjectId,
  FactoryRoleProfileDefinition,
  FactoryKey,
) {
  let project = octacity_server_domain::ProjectId::generate();
  let mut definition = octacity_server_store::testing::role_profile_contract_definition(project);
  let discovery_sha = reference.digest().to_string();
  // The server's discovery program may be native while the task plugin targets a VM.
  definition.toolchain.plugin = ImmutableReference::new(
    reference.identity().clone(),
    reference.version().clone(),
    FactoryDigest::from_bytes([99; 32]),
  );
  let parameters = FlowDataSchema::new(
    FactoryKey::new("params").unwrap(),
    FactoryKey::new("v1").unwrap(),
    FlowValueSchema::Object {
      fields: Default::default(),
    },
  )
  .unwrap();
  let alias = FactoryKey::new("runtime-profile").unwrap();
  let document = json!({"version":1,"codex":[],"projects":[{
    "project_id":project,"references":[{"kind":"node_capability","alias":alias,"reference":definition.toolchain.executable}],
    "build_configurations":[{"alias":"build","reference":definition.build_configuration}],
    "role_profiles":{"runtime-profile":{"ceiling":definition,"parameters":parameters,"public_environment":{},"enforcement":PermissionCategory::ALL,
      "model_discovery":{"executable":program,"executable_sha256":discovery_sha,"environment":{}}}}
  }]});
  (document, project, definition, alias)
}

#[tokio::test]
async fn model_refresh_is_independent_and_rejects_overlapping_requests_without_queueing() {
  use super::FactoryRuntimeInstaller;
  use std::time::Duration;
  let gate = tempfile::tempdir().unwrap();
  let (_directory, program, reference) = plugin_fixture_with_script(
    json!({"available":true,"manual_model":true,"models":[],"parameters":{"type":"object","additionalProperties":false,"properties":{}}}),
    r#"if [ -n "$DISCOVERY_GATE" ] && [ -f "$DISCOVERY_GATE/block" ]; then
      : > "$DISCOVERY_GATE/entered"
      while [ ! -f "$DISCOVERY_GATE/release" ]; do /bin/sleep 0.01; done
    fi"#,
  );
  let (mut document, project, _, slow) = runtime_catalog_document(&program, &reference);
  let other = FactoryKey::new("other-runtime").unwrap();
  let declaration = document["projects"][0]["role_profiles"]["runtime-profile"].clone();
  document["projects"][0]["role_profiles"]["other-runtime"] = declaration;
  document["projects"][0]["role_profiles"]["runtime-profile"]["model_discovery"]["environment"] =
    json!({"DISCOVERY_GATE":gate.path()});
  let mut other_reference = document["projects"][0]["references"][0].clone();
  other_reference["alias"] = json!(other);
  document["projects"][0]["references"]
    .as_array_mut()
    .unwrap()
    .push(other_reference);
  let installer =
    super::factory_profile_file::FileProfileInstaller::parse_json(&serde_json::to_vec(&document).unwrap(), &[])
      .unwrap();
  let installed = installer.install(super::factory_tests::resources()).await.unwrap();
  std::fs::write(gate.path().join("block"), "").unwrap();
  let capabilities = installed.capabilities.clone();
  let same_source = slow.clone();
  let before = installed.capabilities.availability(project).await.unwrap();
  let refreshing = tokio::spawn(async move { capabilities.refresh_role_models(project, &slow).await });
  let entered = tokio::time::timeout(Duration::from_secs(2), async {
    while !gate.path().join("entered").exists() {
      tokio::time::sleep(Duration::from_millis(10)).await;
    }
  })
  .await;
  let independent = tokio::time::timeout(
    Duration::from_secs(1),
    installed.capabilities.refresh_role_models(project, &other),
  )
  .await;
  let overlapping = tokio::time::timeout(
    Duration::from_secs(1),
    installed.capabilities.refresh_role_models(project, &same_source),
  )
  .await;
  let during = installed.capabilities.availability(project).await.unwrap();
  std::fs::write(gate.path().join("release"), "").unwrap();
  let completed = refreshing.await.unwrap();
  assert!(entered.is_ok(), "slow source did not enter discovery");
  assert!(completed.is_ok());
  assert_eq!(
    during, before,
    "an overlapping request must preserve the usable snapshot"
  );
  assert!(
    overlapping.is_ok_and(|result| result.is_err()),
    "an overlapping refresh must fail immediately instead of queuing"
  );
  assert!(
    independent.is_ok_and(|result| result.is_ok()),
    "an independent source waited behind the stalled refresh"
  );
}

#[tokio::test]
async fn file_runtime_installs_qualified_settings_support_without_changing_model_discovery() {
  use super::FactoryRuntimeInstaller;
  let (_directory, program, reference) = plugin_fixture();
  let (mut document, project, definition, alias) = runtime_catalog_document(&program, &reference);
  let support =
    json!({"mcp_transports":["http","stdio"],"mcp_executables":{"/runtime/reader":definition.toolchain.executable}});
  document["projects"][0]["role_profiles"][alias.as_str()]["settings_support"] = support.clone();
  let installer =
    super::factory_profile_file::FileProfileInstaller::parse_json(&serde_json::to_vec(&document).unwrap(), &[])
      .expect("file installation accepts the shared Runtime support contract");
  let installed = installer.install(super::factory_tests::resources()).await.unwrap();
  let octacity_server_store::FactoryConfigurationAvailability::Available(choices) =
    installed.capabilities.availability(project).await.unwrap()
  else {
    panic!("missing installed choices")
  };
  assert_eq!(
    serde_json::to_value(choices.runtime_settings_support(&alias).unwrap()).unwrap(),
    support
  );
  let capability = &choices.role_profile_capabilities()[&alias];
  assert_eq!(
    capability.ceiling, definition,
    "support does not create MCP bindings or permissions"
  );
  assert_eq!(
    capability.model_catalog.as_ref().unwrap().models[0].identifier,
    "Vendor/future:preview"
  );
}
