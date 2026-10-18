//! Read-only checks through verified provider processes, without a Factory or task store.
use super::*;
use octacity_server_application::*;
use octacity_server_domain::{IntegrationId, Timestamp};
use octacity_server_factory::*;
use octacity_server_store::{testing::integration_connection_fixture, *};
use octacity_server_work_source::WorkAdapterRegistry;
use octacity_work_source_protocol::{CredentialClass, CredentialHandle, ResourceAccess};
use sha2::{Digest as _, Sha256};
use std::{fs, os::unix::fs::PermissionsExt as _, time::Duration};

#[tokio::test]
async fn released_runtime_exposes_only_verified_installed_providers_on_protected_ingress() {
  let (root, connection, _) = fixture("custom-provider");
  let registry = root.path().join("adapters");
  fs::create_dir(&registry).unwrap();
  fs::rename(root.path().join("custom-provider"), registry.join("custom-provider")).unwrap();
  let definition = connection.definition;
  let installed = InstalledIntegration {
    installation: definition.installation,
    provider: definition.provider,
    adapter: definition.adapter,
    endpoints: vec![definition.endpoint],
    authentication: vec![definition.authentication],
    resources: definition.resources,
  };
  let catalog = root.path().join("integrations.json");
  fs::write(
    &catalog,
    serde_json::to_vec(&serde_json::json!({
      "version":1, "registry":registry, "installations":[installed]
    }))
    .unwrap(),
  )
  .unwrap();
  fs::set_permissions(&catalog, fs::Permissions::from_mode(0o600)).unwrap();
  let socket = root.path().join("management.sock");
  use std::os::unix::fs::MetadataExt;
  let extra = format!(
    "[integration_management]\nsocket_path={:?}\npeer_uid={}\nprincipal='test-proxy'\ncatalog_file={:?}\n",
    socket.to_str().unwrap(),
    fs::metadata(root.path()).unwrap().uid(),
    catalog.to_str().unwrap()
  );
  let config = crate::ServerConfig::parse_toml(&super::tests::local_runtime_config::runtime_config_source(
    root.path(),
    "postgres://test@127.0.0.1:1/unused",
    "http://127.0.0.1:9",
    &extra,
  ))
  .expect("installed integration catalog must be configurable");
  let runtime = ServerRuntime::start(config).await.unwrap();
  let client = reqwest::Client::builder()
    .unix_socket(socket.as_path())
    .build()
    .unwrap();
  let response = client
    .get("http://localhost/api/v1/integration-installations")
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  let body: serde_json::Value = response.json().await.unwrap();
  assert_eq!(body["items"].as_array().unwrap().len(), 1);
  assert_eq!(body["items"][0]["provider"], "custom-provider");
  assert_eq!(body["items"][0]["resources"][0]["native_id"], "external-project-42");
  assert!(!body.to_string().contains("private-installed-reader"));
  runtime.shutdown().await.unwrap();
}

pub(super) fn fixture(
  provider: &str,
) -> (
  tempfile::TempDir,
  PublishedIntegrationConnection,
  IntegrationCheckProfile,
) {
  let root = tempfile::tempdir().unwrap();
  let directory = root.path().join(provider);
  fs::create_dir(&directory).unwrap();
  let script = r#"#!/bin/sh
IFS= read -r request || exit 90
case "$request" in *'"operation":"check"'*) ;; *) exit 91 ;; esac
printf '%s' "$request" > captured.json
printf 'called\n' >> calls
id=$(printf '%s' "$request" | /usr/bin/sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
check=$(printf '%s' "$request" | /usr/bin/sed -n 's/.*"check_id":"\([^"]*\)".*/\1/p')
profile=$(printf '%s' "$request" | /usr/bin/sed -n 's/.*"configuration_digest":"\([^"]*\)".*/\1/p')
printf '{"family":"source","message":{"protocol_version":1,"request_id":"%s","outcome":' "$id"
/usr/bin/sed -e "s/CHECK_ID/$check/g" -e "s/PROFILE/$profile/g" outcome.json
printf '}}\n'
"#;
  let outcome = serde_json::json!({"status":"checked","payload":{"check_id":"CHECK_ID","configuration_digest":"PROFILE",
    "resource_id":"external-project-42","configuration":"ready","authentication":"ready","resource_access":"ready","capability":"ready","retry_after_millis":null}});
  fs::write(directory.join("outcome.json"), outcome.to_string()).unwrap();
  fs::write(directory.join("plugin"), script).unwrap();
  fs::set_permissions(directory.join("plugin"), fs::Permissions::from_mode(0o700)).unwrap();
  let digest = FactoryDigest::from_bytes(Sha256::digest(script).into());
  fs::write(directory.join("adapter.toml"), format!("manifest_version = 1\nadapter_id = {provider:?}\nexecutable = \"plugin\"\nexecutable_sha256 = \"{digest}\"\n[protocol.source]\nmin = 1\nmax = 1\n[capabilities.source]\ncheck = true\ncancel = false\ncredential_class = \"work_source_read\"\nconfiguration_digest = \"{}\"\n", "e".repeat(64))).unwrap();
  let (mut installed, draft) = integration_connection_fixture(provider);
  installed.adapter = ImmutableReference::new(
    FactoryKey::new(provider).unwrap(),
    FactoryKey::new("v1").unwrap(),
    digest,
  );
  let connection = PublishIntegrationConnection {
    intent: IntegrationConnectionIntent {
      id: IntegrationId::generate(),
      expected_current_version: None,
      draft,
    },
    installed,
    idempotency_key: IdempotencyKey::new("fixture").unwrap(),
    published_at: Timestamp::from_unix_millis(1).unwrap(),
  }
  .resolve()
  .unwrap();
  let resource = &connection.definition.resources[0];
  let profile = IntegrationCheckProfile {
    installation: connection.definition.installation.clone(),
    resource_reference: resource.reference.clone(),
    capability: resource.capabilities[0].clone(),
    plugin: connection.definition.adapter.clone(),
    provider_configuration: "e".repeat(64).parse().unwrap(),
    resource: ResourceAccess {
      resource_id: resource.native_id.to_string(),
      resource_locator: "private-installed-alias".into(),
      credential: CredentialHandle {
        class: CredentialClass::WorkSourceRead,
        handle: "private-installed-reader".into(),
      },
    },
    network_hosts: vec!["tracker.invalid".into()],
  };
  (root, connection, profile)
}

#[tokio::test]
async fn installed_connection_checks_use_the_verified_host_without_factory_inputs_or_provider_specific_dispatch() {
  for provider in ["gitlab", "jira", "custom-provider"] {
    let (root, connection, profile) = fixture(provider);
    let checker = HostedIntegrationConnectionChecker::new(
      Arc::new(WorkAdapterRegistry::discover(root.path()).unwrap()),
      vec![profile],
      Duration::from_secs(10),
      Duration::from_secs(1),
      CancellationToken::new(),
    )
    .unwrap();
    let result = checker
      .check(
        &connection,
        &connection.definition.resources[0],
        IntegrationPurpose::TaskRead,
      )
      .await
      .unwrap();
    assert_eq!(result.configuration, IntegrationCheckState::Ready);
    assert_eq!(result.authentication, IntegrationCheckState::Ready);
    assert_eq!(result.resource_access, IntegrationCheckState::Ready);
    assert_eq!(result.capability, IntegrationCheckState::Ready);
    let wire: serde_json::Value =
      serde_json::from_slice(&fs::read(root.path().join(provider).join("captured.json")).unwrap()).unwrap();
    assert_eq!(wire["message"]["command"]["operation"], "check");
    let intent = &wire["message"]["command"]["payload"];
    assert_eq!(intent["resource"]["resource_id"], "external-project-42");
    assert_eq!(intent["endpoint"], "https://tracker.invalid/api/");
    for absent in [
      "cursor",
      "selection",
      "mapping",
      "item_id",
      "work_id",
      "input_id",
      "issue_id",
    ] {
      assert!(intent.get(absent).is_none());
    }
  }
}

#[tokio::test]
async fn connection_checks_cannot_substitute_native_adapter_or_credential_pins_or_borrow_another_purpose() {
  for changed in ["native", "adapter", "credential", "purpose"] {
    let (root, mut connection, profile) = fixture("opaque-provider");
    match changed {
      "native" => {
        connection.definition.resources[0].native_id = ExternalWorkIdentity::new("replacement-native").unwrap()
      }
      "adapter" => {
        connection.definition.adapter = ImmutableReference::new(
          FactoryKey::new("different-adapter").unwrap(),
          FactoryKey::new("v1").unwrap(),
          FactoryDigest::from_bytes([8; 32]),
        )
      }
      "credential" => {
        connection.definition.resources[0].capabilities[0].credential =
          serde_json::from_value(serde_json::json!({"provider":"installed","reference":"different-logical-grant"}))
            .unwrap()
      }
      _ => {}
    }
    connection.digest = connection.definition.content_digest().unwrap();
    let checker = HostedIntegrationConnectionChecker::new(
      Arc::new(WorkAdapterRegistry::discover(root.path()).unwrap()),
      vec![profile],
      Duration::from_secs(10),
      Duration::from_secs(1),
      CancellationToken::new(),
    )
    .unwrap();
    let purpose = if changed == "purpose" {
      IntegrationPurpose::Delivery
    } else {
      IntegrationPurpose::TaskRead
    };
    let result = checker
      .check(&connection, &connection.definition.resources[0], purpose)
      .await
      .unwrap();
    assert_ne!(
      result.capability,
      IntegrationCheckState::Ready,
      "{changed} must not use another scope's handle"
    );
    if changed == "purpose" {
      assert_eq!(result.capability, IntegrationCheckState::Unsupported);
    }
    assert!(
      !root.path().join("opaque-provider/captured.json").exists(),
      "{changed} must fail before spawn"
    );
  }
}

#[test]
fn check_installation_rejects_ambiguous_profiles_and_zero_process_bounds() {
  let (root, _, profile) = fixture("opaque-provider");
  let registry = Arc::new(WorkAdapterRegistry::discover(root.path()).unwrap());
  assert!(
    HostedIntegrationConnectionChecker::new(
      registry.clone(),
      vec![profile.clone(), profile.clone()],
      Duration::from_secs(10),
      Duration::from_secs(1),
      CancellationToken::new()
    )
    .is_err()
  );
  assert!(
    HostedIntegrationConnectionChecker::new(
      registry.clone(),
      vec![profile.clone()],
      Duration::ZERO,
      Duration::from_secs(1),
      CancellationToken::new()
    )
    .is_err()
  );
  let mut newer = profile.clone();
  newer.capability.configuration = ImmutableReference::new(
    FactoryKey::new("new-profile").unwrap(),
    FactoryKey::new("v2").unwrap(),
    FactoryDigest::from_bytes([9; 32]),
  );
  assert!(
    HostedIntegrationConnectionChecker::new(
      registry,
      vec![profile, newer],
      Duration::from_secs(10),
      Duration::from_secs(1),
      CancellationToken::new()
    )
    .is_ok(),
    "distinct immutable configuration pins must coexist without replacing old scope"
  );
}

#[tokio::test]
async fn classified_provider_failures_remain_safe_readiness_and_never_trigger_another_operation_or_retry() {
  for (class, retry, expected) in [
    ("transient", None, IntegrationCheckState::Unavailable),
    ("transient", Some(3000), IntegrationCheckState::Throttled),
    ("unsupported", None, IntegrationCheckState::Unsupported),
  ] {
    let (root, connection, profile) = fixture("custom-provider");
    fs::write(
      root.path().join("custom-provider/outcome.json"),
      serde_json::json!({"status":"failure","payload":{"class":class,"retry_after_millis":retry}}).to_string(),
    )
    .unwrap();
    let checker = HostedIntegrationConnectionChecker::new(
      Arc::new(WorkAdapterRegistry::discover(root.path()).unwrap()),
      vec![profile],
      Duration::from_secs(10),
      Duration::from_secs(1),
      CancellationToken::new(),
    )
    .unwrap();
    let result = checker
      .check(
        &connection,
        &connection.definition.resources[0],
        IntegrationPurpose::TaskRead,
      )
      .await
      .unwrap();
    assert_eq!(result.capability, expected, "{class}/{retry:?}");
    assert_eq!(result.authentication, IntegrationCheckState::Unverified);
    assert_eq!(result.retry_after_millis, retry);
    assert_eq!(
      fs::read_to_string(root.path().join("custom-provider/calls")).unwrap(),
      "called\n"
    );
    let wire: serde_json::Value =
      serde_json::from_slice(&fs::read(root.path().join("custom-provider/captured.json")).unwrap()).unwrap();
    assert_eq!(wire["message"]["command"]["operation"], "check");
  }
}

#[tokio::test]
async fn invalid_installed_check_scope_is_distinct_from_a_provider_outage_and_never_spawns() {
  for changed in ["profile", "class"] {
    let (root, connection, mut profile) = fixture("custom-provider");
    if changed == "profile" {
      profile.provider_configuration = FactoryDigest::from_bytes([15; 32]);
    } else {
      profile.resource.credential.class = CredentialClass::WorkReporterWrite;
    }
    let checker = HostedIntegrationConnectionChecker::new(
      Arc::new(WorkAdapterRegistry::discover(root.path()).unwrap()),
      vec![profile],
      Duration::from_secs(10),
      Duration::from_secs(1),
      CancellationToken::new(),
    )
    .unwrap();
    let result = checker
      .check(
        &connection,
        &connection.definition.resources[0],
        IntegrationPurpose::TaskRead,
      )
      .await
      .unwrap();
    assert_eq!(result.configuration, IntegrationCheckState::Invalid, "{changed}");
    assert_eq!(result.authentication, IntegrationCheckState::Unverified);
    assert_eq!(result.capability, IntegrationCheckState::Unverified);
    assert!(!root.path().join("custom-provider/captured.json").exists());
  }
}
